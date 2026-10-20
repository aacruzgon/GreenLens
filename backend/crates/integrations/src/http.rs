//! The one shared `reqwest::Client`, the retry helper and the small HTTP utilities every
//! adapter uses.
//!
//! One client per process: `reqwest::Client` is an `Arc` inside, so a clone is a reference
//! count and every adapter shares one connection pool. It has a connect timeout and a *read*
//! timeout (a gap between two reads, reset on every chunk) but no total timeout, so the same
//! client can carry a minutes-long LLM stream. Non-streaming calls set a total deadline per
//! request with `RequestBuilder::timeout`.

use std::{future::Future, time::Duration};

use domain::ports::BoxError;
use reqwest::{StatusCode, header::HeaderMap};

pub use secrecy::{ExposeSecret, SecretString};

/// The `User-Agent` of every request. Nominatim requires an identifying one; the others log it.
pub const USER_AGENT: &str = concat!(
    "PDM/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/aacruzgon/GreenLens)"
);

/// Settings of the shared client.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// TCP and TLS connect deadline.
    pub connect_timeout: Duration,
    /// The longest gap between two reads of one response. Anthropic sends `ping` events while
    /// the model thinks, so 60 s is safe for streams and longer than `SoilGrids`' slowest query.
    pub read_timeout: Duration,
    /// How long an idle pooled connection is kept.
    pub pool_idle_timeout: Duration,
    /// The `User-Agent` header.
    pub user_agent: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(60),
            pool_idle_timeout: Duration::from_secs(90),
            user_agent: USER_AGENT.to_owned(),
        }
    }
}

/// The client could not be built (the TLS backend failed to initialise).
#[derive(Debug, thiserror::Error)]
#[error("HTTP client could not be built")]
pub struct HttpBuildError(#[source] BoxError);

/// Builds the shared client. Call once at start and clone it into every adapter.
pub fn client(config: &HttpConfig) -> Result<reqwest::Client, HttpBuildError> {
    reqwest::Client::builder()
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .pool_idle_timeout(config.pool_idle_timeout)
        .user_agent(config.user_agent.clone())
        .build()
        .map_err(|e| HttpBuildError(Box::new(e)))
}

// ---------------------------------------------------------------------------------------------
// Retries
// ---------------------------------------------------------------------------------------------

/// How often and how long to retry one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Retries after the first attempt; 0 disables retrying.
    pub max_retries: u32,
    /// The first back-off ceiling; it doubles on every retry.
    pub base: Duration,
    /// The largest back-off ceiling.
    pub cap: Duration,
    /// A `Retry-After` longer than this gives up instead of waiting.
    pub max_retry_after: Duration,
}

impl RetryPolicy {
    /// No retries: the caller owns the deadline (the worker, which retries a whole job).
    pub const NONE: Self = Self {
        max_retries: 0,
        base: Duration::ZERO,
        cap: Duration::ZERO,
        max_retry_after: Duration::ZERO,
    };

    /// Two retries, 0.5 s then 1 s ceilings, at most 8 s: what Anthropic's SDKs do.
    pub const STANDARD: Self = Self {
        max_retries: 2,
        base: Duration::from_millis(500),
        cap: Duration::from_secs(8),
        max_retry_after: Duration::from_secs(60),
    };

    /// The wait before retry number `retry` (0-based): "full jitter", a uniform random
    /// duration in `[0, min(cap, base · 2^retry)]`, never shorter than the server's
    /// `Retry-After`. `None` when the server asks for longer than `max_retry_after`.
    pub fn delay(&self, retry: u32, retry_after: Option<Duration>) -> Option<Duration> {
        if let Some(after) = retry_after
            && after > self.max_retry_after
        {
            return None;
        }
        let ceiling = self
            .base
            .saturating_mul(2u32.saturating_pow(retry))
            .min(self.cap);
        let ceiling_ms = u64::try_from(ceiling.as_millis()).unwrap_or(u64::MAX);
        let jitter = Duration::from_millis(if ceiling_ms == 0 {
            0
        } else {
            rand::random_range(0..=ceiling_ms)
        });
        Some(retry_after.map_or(jitter, |after| after.max(jitter)))
    }
}

/// What one attempt of a retried call produced.
#[derive(Debug)]
pub enum Attempt<T, E> {
    /// Success.
    Done(T),
    /// A failure worth another attempt, with the server's `Retry-After` if it sent one.
    Retry {
        /// The error, returned if no retry is left.
        error: E,
        /// The server's requested wait.
        retry_after: Option<Duration>,
    },
    /// A failure that another attempt would repeat.
    Fail(E),
}

/// Runs `attempt` until it is done, fails for good, or the policy runs out.
///
/// Generic rather than boxed: `F` is the closure that makes one attempt (called with the
/// 0-based attempt number), `Fut` the future it returns, `T` and `E` the call's success and
/// error types. Each caller gets a monomorphised copy, so nothing is allocated per attempt and
/// the futures need not be `'static`: `Fut` may borrow the caller's request body. Waits use
/// `tokio::time::sleep`, which tests pause and advance.
pub async fn retry<T, E, F, Fut>(policy: RetryPolicy, mut attempt: F) -> Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Attempt<T, E>>,
{
    let mut n = 0;
    loop {
        match attempt(n).await {
            Attempt::Done(value) => return Ok(value),
            Attempt::Fail(error) => return Err(error),
            Attempt::Retry { error, retry_after } => {
                if n >= policy.max_retries {
                    return Err(error);
                }
                let Some(wait) = policy.delay(n, retry_after) else {
                    return Err(error);
                };
                tracing::debug!(attempt = n + 1, wait_ms = wait.as_millis(), "retrying");
                tokio::time::sleep(wait).await;
                n += 1;
            }
        }
    }
}

/// Spaces calls to one upstream: each [`Pacer::wait`] returns no sooner than `interval` after
/// the previous one returned. Callers queue on a tokio mutex (fair, first come first served),
/// so concurrent fetches line up instead of bursting. Used for Nominatim's 1 request a second
/// and `SoilGrids`' 5 calls a minute. Time is tokio's, so tests pause it.
#[derive(Debug)]
pub struct Pacer {
    interval: Duration,
    last: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}

impl Pacer {
    /// A pacer with this spacing.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: tokio::sync::Mutex::new(None),
        }
    }

    /// Waits for this caller's turn.
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        if let Some(t) = *last {
            tokio::time::sleep_until(t + self.interval).await;
        }
        *last = Some(tokio::time::Instant::now());
    }
}

/// `Retry-After` in delta-seconds. The HTTP-date form is treated as absent: none of our
/// upstreams sends it, and reading it would need the wall clock outside the `Clock` port.
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Whether a status is worth retrying for an idempotent call: 408, 429 and every 5xx.
pub fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

/// A transport failure, without exposing `reqwest::Error` in any public signature.
#[derive(Debug, thiserror::Error)]
#[error("{}", if *.timeout { "request timed out" } else { "request failed" })]
pub struct TransportError {
    /// The deadline or read gap ran out.
    pub timeout: bool,
    /// Whether no connection was made (DNS, refused, TLS).
    pub connect: bool,
    /// The client error.
    #[source]
    pub source: BoxError,
}

impl TransportError {
    /// Wraps a `reqwest` error, without its URL: a URL's query can carry a token or a code,
    /// and the error's `Display` and chain are logged.
    pub(crate) fn from_reqwest(e: reqwest::Error) -> Self {
        let e = e.without_url();
        Self {
            timeout: e.is_timeout(),
            connect: e.is_connect(),
            source: Box::new(e),
        }
    }
}

/// At most `max` bytes of an error body, for logs and error messages, cut on a char boundary.
pub(crate) fn snippet(body: &str, max: usize) -> String {
    if body.len() <= max {
        return body.to_owned();
    }
    let mut end = max;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &body[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_honours_retry_after_and_cap() {
        let p = RetryPolicy::STANDARD;
        for retry in 0..6 {
            let d = p.delay(retry, None).unwrap();
            assert!(d <= p.cap);
        }
        assert_eq!(
            p.delay(0, Some(Duration::from_secs(5))),
            Some(Duration::from_secs(5))
        );
        assert_eq!(p.delay(0, Some(Duration::from_secs(61))), None);
        assert_eq!(RetryPolicy::NONE.delay(0, None), Some(Duration::ZERO));
    }

    #[tokio::test]
    async fn a_transport_error_does_not_carry_the_request_url() {
        use std::fmt::Write as _;
        // Nothing listens on port 1, so the request fails to connect.
        let url = "http://127.0.0.1:1/path?token=s3cret&code=abc";
        let e = reqwest::Client::new().get(url).send().await.unwrap_err();
        assert!(
            e.to_string().contains("s3cret"),
            "the plain error has the URL"
        );
        let e = TransportError::from_reqwest(e);
        let mut rendered = e.to_string();
        let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(&e);
        while let Some(c) = cause {
            let _ = write!(rendered, " / {c} / {c:?}");
            cause = c.source();
        }
        for leak in ["s3cret", "code=abc", "token=", "?"] {
            assert!(!rendered.contains(leak), "{leak} in {rendered}");
        }
        assert!(e.connect);
    }

    #[test]
    fn retry_after_seconds_only() {
        let mut h = HeaderMap::new();
        h.insert(reqwest::header::RETRY_AFTER, "7".parse().unwrap());
        assert_eq!(retry_after(&h), Some(Duration::from_secs(7)));
        h.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after(&h), None);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_stops_after_policy() {
        let mut calls = 0;
        let r: Result<(), &str> = retry(RetryPolicy::STANDARD, |_| {
            calls += 1;
            async {
                Attempt::Retry {
                    error: "busy",
                    retry_after: None,
                }
            }
        })
        .await;
        assert_eq!(r, Err("busy"));
        assert_eq!(calls, 3);
    }

    #[tokio::test(start_paused = true)]
    async fn pacer_spaces_calls() {
        let p = Pacer::new(Duration::from_secs(1));
        let start = tokio::time::Instant::now();
        p.wait().await;
        p.wait().await;
        p.wait().await;
        assert_eq!(start.elapsed(), Duration::from_secs(2));
    }

    #[test]
    fn snippet_cuts_on_char_boundary() {
        assert_eq!(snippet("ééé", 3), "é…");
        assert_eq!(snippet("abc", 5), "abc");
    }
}
