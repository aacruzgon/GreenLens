//! Slack Web API client for the farm connectors (docs/EON E2.1; [connectors 5], [6]).
//!
//! Read methods only, plus the OAuth exchange and the revoke: `auth.test`, `conversations.list`,
//! `conversations.history`, `conversations.replies`, `users.info`, `auth.revoke` and
//! `oauth.v2.access`. One client serves every workspace, so the token is an argument of each
//! call, never part of the client.
//!
//! **Errors.** Slack refuses with HTTP 200 and `{"ok": false, "error": "<code>"}`; that is
//! always an error, never success. `not_authed`, `invalid_auth`, `token_revoked`,
//! `token_expired` and `account_inactive` are [`FarmClientError::AuthRefused`], and so are
//! `missing_scope`, `no_permission`, `team_access_not_granted`, `ekm_access_denied`,
//! `not_allowed_token_type` and `org_login_required`: the owner must reconnect to grant the scope;
//! `channel_not_found`, `not_in_channel`, `thread_not_found` and `user_not_found` are
//! [`FarmClientError::NotFound`]; `ratelimited` or HTTP `429` is retried after the
//! `Retry-After` the server sent, then [`FarmClientError::RateLimited`]; a 5xx, `internal_error`
//! or a timeout is retried, then [`FarmClientError::Unavailable`]; any other code is
//! [`FarmClientError::Refused`], except that `oauth.v2.access` maps `invalid_code`,
//! `code_already_used` and `code_expired` (a bad user code) to `AuthRefused`. A response body is never kept in an error or logged, because
//! it may echo the farm's records ([connectors 8]): the log lines carry the method, the status
//! and the short code, and a code that isn't a short `snake_case` word is replaced.
//!
//! **Pacing.** `conversations.history` and `conversations.replies` are Tier 1 for new installs
//! of apps not in the Slack Marketplace: about one call a minute per method and workspace
//! ([connectors 3]). Each (team, method) pair has its own [`Pacer`], so every connector holding
//! one team's token shares it. The pacers live in a `moka` cache, bounded by
//! [`SlackConfig::max_pacers`] and dropped after idling for twice the interval (at least 10
//! minutes), so the map can't grow without bound and a pacer is never dropped while its last
//! call is still inside its interval. A call waits for its turn; the sync bounds a run and
//! continues from its cursor on the next one. [`SlackConfig::era`] sets no pacing, but a
//! `Retry-After` is still honoured.
//!
//! [connectors 3]: ../../../docs/connectors/README.md#3-decisions
//! [connectors 5]: ../../../docs/connectors/README.md#5-connecting-and-tokens
//! [6]: ../../../docs/connectors/README.md#6-sync
//! [connectors 8]: ../../../docs/connectors/README.md#8-privacy-and-security

mod wire;

#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use moka::future::Cache;
use reqwest::{Method, StatusCode, header::CONTENT_TYPE};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;

use crate::{
    farm::FarmClientError,
    http::{self, Attempt, Pacer, RetryPolicy, TransportError},
};

/// The `vendor` of every [`FarmClientError`] this client returns.
pub const VENDOR: &str = "slack";

/// Slack's Web API host. A constant in `api` too: no route can point a connector elsewhere.
pub const SLACK_BASE_URL: &str = "https://slack.com";

/// Tier 1: one `conversations.history` or `conversations.replies` call a minute per workspace
/// for a new install of an app not in the Slack Marketplace ([connectors 3]).
///
/// [connectors 3]: ../../../docs/connectors/README.md#3-decisions
pub const TIER_1_INTERVAL: Duration = Duration::from_mins(1);

/// Tier 1's largest page of `conversations.history` and `conversations.replies`: 15 messages.
pub const TIER_1_PAGE_LIMIT: u32 = 15;

/// Pages read by one call that reads every page, before the answer is treated as malformed
/// (a cursor that never ends).
const MAX_PAGES: u32 = 1_000;

/// The longest vendor code kept; a longer one is replaced by [`UNRECOGNISED`].
const MAX_CODE_LEN: usize = 64;

/// What a missing or odd-looking code becomes.
const UNRECOGNISED: &str = "unrecognised";

/// The pacers are kept at least this long after their last use.
const MIN_PACER_IDLE: Duration = Duration::from_mins(10);

/// The token is refused, or lacks a scope: either way only the owner reconnecting fixes it.
/// The codes `oauth.v2.access` answers for a code the user brought that is wrong, spent or old.
const BAD_CODES: [&str; 3] = ["invalid_code", "code_already_used", "code_expired"];

const AUTH_CODES: [&str; 11] = [
    "not_authed",
    "invalid_auth",
    "token_revoked",
    "token_expired",
    "account_inactive",
    "missing_scope",
    "no_permission",
    "team_access_not_granted",
    "ekm_access_denied",
    "not_allowed_token_type",
    "org_login_required",
];

const NOT_FOUND_CODES: [&str; 4] = [
    "channel_not_found",
    "not_in_channel",
    "thread_not_found",
    "user_not_found",
];

const UNAVAILABLE_CODES: [&str; 4] = [
    "internal_error",
    "fatal_error",
    "service_unavailable",
    "request_timeout",
];

/// Settings.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    /// `https://slack.com`, or an Era simulator for an `era` connector and the tests. The
    /// methods are under `<base_url>/api/`.
    pub base_url: String,
    /// Total deadline of one request.
    pub timeout: Duration,
    /// Retries of `429`, 5xx, `internal_error` and transport failures.
    pub retry: RetryPolicy,
    /// The least time between two `conversations.history` (or two `conversations.replies`)
    /// calls for one team. Zero turns pacing off.
    pub history_interval: Duration,
    /// `limit` of a `conversations.history` or `conversations.replies` page.
    pub history_limit: u32,
    /// `limit` of a `conversations.list` page (a Tier 2 method, not paced).
    pub list_limit: u32,
    /// Most (team, method) pacers kept at once.
    pub max_pacers: u64,
}

impl Default for SlackConfig {
    /// The real Slack: Tier 1 pacing and pages, a 20 s deadline, the standard retries.
    fn default() -> Self {
        Self {
            base_url: SLACK_BASE_URL.to_owned(),
            timeout: Duration::from_secs(20),
            retry: RetryPolicy::STANDARD,
            history_interval: TIER_1_INTERVAL,
            history_limit: TIER_1_PAGE_LIMIT,
            list_limit: 200,
            max_pacers: 10_000,
        }
    }
}

impl SlackConfig {
    /// An Era simulator at `base_url`: no pacing and no back-off, but a `Retry-After` is still
    /// honoured (up to 60 s). Pages stay at Tier 1's size, so paging is exercised.
    pub fn era(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            history_interval: Duration::ZERO,
            retry: RetryPolicy {
                base: Duration::ZERO,
                cap: Duration::ZERO,
                ..RetryPolicy::STANDARD
            },
            ..Self::default()
        }
    }
}

/// `auth.test`: who the token is, and the workspace URL permalinks are built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthTest {
    /// The workspace's name.
    pub team: String,
    /// The workspace's id, which keys the pacers.
    pub team_id: String,
    /// The workspace URL, with a trailing slash: `https://maplerow.slack.com/`.
    pub url: String,
    /// The bot's user id.
    pub user_id: String,
    /// The bot's id.
    pub bot_id: Option<String>,
}

/// A channel the bot is a member of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackChannel {
    /// `C…`.
    pub id: String,
    /// The name without `#`.
    pub name: String,
    /// A private channel (`groups:read`).
    pub is_private: bool,
}

/// One message of a channel or a thread. Its `Debug` leaves the text out.
#[derive(Clone, PartialEq, Eq)]
pub struct SlackMessage {
    /// The message's id within its channel: `1791792000.000100`.
    pub ts: String,
    /// The author's user id; absent for some bot and system messages.
    pub user: Option<String>,
    /// The text, in Slack's markup.
    pub text: String,
    /// The thread's parent `ts`: set on a parent that has replies and on each reply.
    pub thread_ts: Option<String>,
    /// Replies in the thread, on a parent.
    pub reply_count: u32,
    /// The newest reply's `ts`, on a parent; the sync reads the replies when it changes.
    pub latest_reply: Option<String>,
    /// `channel_join`, `bot_message` and the like; `None` for a plain message.
    pub subtype: Option<String>,
}

impl SlackMessage {
    /// A reply in a thread, as opposed to a parent or a plain message.
    pub fn is_reply(&self) -> bool {
        self.thread_ts
            .as_ref()
            .is_some_and(|parent| *parent != self.ts)
    }
}

impl std::fmt::Debug for SlackMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackMessage")
            .field("ts", &self.ts)
            .field("user", &self.user)
            .field("text_len", &self.text.len())
            .field("thread_ts", &self.thread_ts)
            .field("reply_count", &self.reply_count)
            .field("latest_reply", &self.latest_reply)
            .field("subtype", &self.subtype)
            .finish()
    }
}

/// One page of `conversations.history`, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryPage {
    /// The messages.
    pub messages: Vec<SlackMessage>,
    /// The cursor of the next (older) page; `None` on the last.
    pub next_cursor: Option<String>,
}

/// A Slack user, as the sync keeps one: the id and the names, never an email. Its `Debug`
/// shows the id only, since names are personal data.
#[derive(Clone, PartialEq, Eq)]
pub struct SlackUser {
    /// `U…`.
    pub id: String,
    /// The handle (`maria`).
    pub name: Option<String>,
    /// The full name (`Maria Lopez`).
    pub real_name: Option<String>,
    /// The display name the user chose (`Maria`); often empty in Slack, then `None`.
    pub display_name: Option<String>,
}

impl SlackUser {
    /// The name to show beside a message: the display name, else the full name, else the
    /// handle, else the id.
    pub fn label(&self) -> &str {
        self.display_name
            .as_deref()
            .or(self.real_name.as_deref())
            .or(self.name.as_deref())
            .unwrap_or(&self.id)
    }
}

impl std::fmt::Debug for SlackUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackUser")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// `oauth.v2.access`: the bot token and the workspace it belongs to. The token's `Debug` is
/// redacted.
#[derive(Debug)]
pub struct OAuthAccess {
    /// `xoxb-…`, to be sealed at once.
    pub access_token: SecretString,
    /// `bot`.
    pub token_type: Option<String>,
    /// The granted scopes, comma-separated.
    pub scope: String,
    /// The bot's user id.
    pub bot_user_id: Option<String>,
    /// The Slack app's id.
    pub app_id: Option<String>,
    /// The workspace's id.
    pub team_id: String,
    /// The workspace's name.
    pub team_name: String,
}

/// The Slack Web API client. One per process; `Clone` shares the pacers.
#[derive(Clone)]
pub struct SlackClient {
    http: reqwest::Client,
    config: Arc<SlackConfig>,
    pacers: Cache<(String, &'static str), Arc<Pacer>>,
}

impl std::fmt::Debug for SlackClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackClient")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// One Web API call, as every attempt makes it.
struct Call<'a> {
    method: &'static str,
    verb: Method,
    token: Option<&'a SecretString>,
    query: &'a [(&'a str, &'a str)],
    form: Option<&'a str>,
    /// The team whose pacer this call waits on.
    paced_team: Option<&'a str>,
    /// Safe to send again after a timeout or a 5xx. The code exchange isn't: a code works once.
    idempotent: bool,
}

impl<'a> Call<'a> {
    fn read(
        method: &'static str,
        token: &'a SecretString,
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            method,
            verb: Method::GET,
            token: Some(token),
            query,
            form: None,
            paced_team: None,
            idempotent: true,
        }
    }
}

impl SlackClient {
    /// A client over the shared `reqwest::Client`.
    pub fn new(http: reqwest::Client, config: SlackConfig) -> Self {
        let idle = config
            .history_interval
            .saturating_mul(2)
            .max(MIN_PACER_IDLE);
        let pacers = Cache::builder()
            .max_capacity(config.max_pacers)
            .time_to_idle(idle)
            .build();
        Self {
            http,
            config: Arc::new(config),
            pacers,
        }
    }

    /// The settings.
    pub fn config(&self) -> &SlackConfig {
        &self.config
    }

    /// `auth.test`: the workspace and the bot behind a token.
    pub async fn auth_test(&self, token: &SecretString) -> Result<AuthTest, FarmClientError> {
        let w: wire::AuthTest = self.call(&Call::read("auth.test", token, &[])).await?;
        Ok(AuthTest {
            team: w.team,
            team_id: w.team_id,
            url: w.url,
            user_id: w.user_id,
            bot_id: w.bot_id,
        })
    }

    /// `conversations.list`, every page: the public and private channels the bot is a member
    /// of, archived ones left out.
    pub async fn conversations_list(
        &self,
        token: &SecretString,
    ) -> Result<Vec<SlackChannel>, FarmClientError> {
        let limit = self.config.list_limit.to_string();
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![
                ("types", "public_channel,private_channel"),
                ("exclude_archived", "true"),
                ("limit", limit.as_str()),
            ];
            if let Some(c) = cursor.as_deref() {
                query.push(("cursor", c));
            }
            let page: wire::ChannelList = self
                .call(&Call::read("conversations.list", token, &query))
                .await?;
            channels.extend(page.channels.into_iter().filter(|c| c.is_member).map(|c| {
                SlackChannel {
                    id: c.id,
                    name: c.name,
                    is_private: c.is_private,
                }
            }));
            cursor = page
                .response_metadata
                .and_then(wire::ResponseMetadata::next);
            if cursor.is_none() {
                return Ok(channels);
            }
        }
        Err(too_many_pages("conversations.list"))
    }

    /// One page of `conversations.history`, newest first: messages after `oldest` (exclusive)
    /// when given, from `cursor` when given. Paced per `team_id`.
    pub async fn conversations_history(
        &self,
        token: &SecretString,
        team_id: &str,
        channel: &str,
        oldest: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<HistoryPage, FarmClientError> {
        let limit = self.config.history_limit.to_string();
        let mut query = vec![("channel", channel), ("limit", limit.as_str())];
        if let Some(oldest) = oldest {
            query.push(("oldest", oldest));
        }
        if let Some(cursor) = cursor {
            query.push(("cursor", cursor));
        }
        let call = Call {
            paced_team: Some(team_id),
            ..Call::read("conversations.history", token, &query)
        };
        let page: wire::Messages = self.call(&call).await?;
        Ok(HistoryPage {
            messages: page.messages.into_iter().map(message).collect(),
            next_cursor: page
                .response_metadata
                .and_then(wire::ResponseMetadata::next),
        })
    }

    /// `conversations.replies`, every page: the parent, then its replies. Each page is paced
    /// per `team_id`.
    pub async fn conversations_replies(
        &self,
        token: &SecretString,
        team_id: &str,
        channel: &str,
        thread_ts: &str,
    ) -> Result<Vec<SlackMessage>, FarmClientError> {
        let limit = self.config.history_limit.to_string();
        let mut messages = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![
                ("channel", channel),
                ("ts", thread_ts),
                ("limit", limit.as_str()),
            ];
            if let Some(c) = cursor.as_deref() {
                query.push(("cursor", c));
            }
            let call = Call {
                paced_team: Some(team_id),
                ..Call::read("conversations.replies", token, &query)
            };
            let page: wire::Messages = self.call(&call).await?;
            messages.extend(page.messages.into_iter().map(message));
            cursor = page
                .response_metadata
                .and_then(wire::ResponseMetadata::next);
            if cursor.is_none() {
                return Ok(messages);
            }
        }
        Err(too_many_pages("conversations.replies"))
    }

    /// `users.info`: the user's names. Never the email, even when Slack sends one.
    pub async fn users_info(
        &self,
        token: &SecretString,
        user: &str,
    ) -> Result<SlackUser, FarmClientError> {
        let query = [("user", user)];
        let w: wire::UserInfo = self.call(&Call::read("users.info", token, &query)).await?;
        let profile = w.user.profile.unwrap_or_default();
        Ok(SlackUser {
            id: w.user.id,
            name: non_empty(w.user.name),
            real_name: non_empty(profile.real_name).or_else(|| non_empty(w.user.real_name)),
            display_name: non_empty(profile.display_name),
        })
    }

    /// `auth.revoke`: the token stops working. A token that is already revoked answers
    /// `token_revoked` or `invalid_auth`, and that counts as revoked (`Ok`), as Google's
    /// `invalid_token` does: the token is gone either way. Any other refusal is an error.
    pub async fn auth_revoke(&self, token: &SecretString) -> Result<(), FarmClientError> {
        let call = Call {
            verb: Method::POST,
            ..Call::read("auth.revoke", token, &[])
        };
        match self.call::<wire::Empty>(&call).await {
            Ok(_) => Ok(()),
            Err(FarmClientError::AuthRefused { code, .. })
                if code == "token_revoked" || code == "invalid_auth" =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// `oauth.v2.access`: exchanges an authorization code for the bot token. The Slack app is a
    /// confidential client, so the client secret goes with the code and there is no PKCE
    /// ([connectors 5]). Not retried after a timeout or a 5xx, since a code works once.
    ///
    /// [connectors 5]: ../../../docs/connectors/README.md#5-connecting-and-tokens
    pub async fn oauth_v2_access(
        &self,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<OAuthAccess, FarmClientError> {
        let body = form(&[
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret.expose_secret()),
        ]);
        let call = Call {
            method: "oauth.v2.access",
            verb: Method::POST,
            token: None,
            query: &[],
            form: Some(&body),
            paced_team: None,
            idempotent: false,
        };
        // A bad code is the user's (`AuthRefused`); a bad redirect URI, client id or secret is our
        // own configuration and stays `Refused`.
        let w: wire::OAuthAccess = self.call(&call).await.map_err(|error| match error {
            FarmClientError::Refused { vendor, code, .. } if BAD_CODES.contains(&code.as_str()) => {
                FarmClientError::AuthRefused { vendor, code }
            }
            other => other,
        })?;
        Ok(OAuthAccess {
            access_token: SecretString::from(w.access_token),
            token_type: w.token_type,
            scope: w.scope,
            bot_user_id: w.bot_user_id,
            app_id: w.app_id,
            team_id: w.team.id,
            team_name: w.team.name,
        })
    }

    /// The pacer of one team's method; `None` when pacing is off.
    async fn pacer(&self, team: &str, method: &'static str) -> Option<Arc<Pacer>> {
        let interval = self.config.history_interval;
        if interval.is_zero() {
            return None;
        }
        Some(
            self.pacers
                .get_with((team.to_owned(), method), async move {
                    Arc::new(Pacer::new(interval))
                })
                .await,
        )
    }

    /// One call with its retries; each attempt waits for the pacer first.
    async fn call<T: DeserializeOwned>(&self, call: &Call<'_>) -> Result<T, FarmClientError> {
        let pacer = match call.paced_team {
            Some(team) => self.pacer(team, call.method).await,
            None => None,
        };
        let pacer = pacer.as_deref();
        http::retry(self.config.retry, |_| async move {
            if let Some(pacer) = pacer {
                pacer.wait().await;
            }
            self.attempt(call).await
        })
        .await
    }

    async fn attempt<T: DeserializeOwned>(&self, call: &Call<'_>) -> Attempt<T, FarmClientError> {
        let url = format!("{}/api/{}", self.config.base_url, call.method);
        let mut request = self
            .http
            .request(call.verb.clone(), url)
            .timeout(self.config.timeout);
        if !call.query.is_empty() {
            request = request.query(call.query);
        }
        // An empty token sends no `Authorization` at all, which Slack answers `not_authed`.
        if let Some(token) = call.token.map(ExposeSecret::expose_secret)
            && !token.is_empty()
        {
            request = request.bearer_auth(token);
        }
        if let Some(form) = call.form {
            request = request
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(form.to_owned());
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => return transport(call, e),
        };
        let status = response.status();
        let retry_after = http::retry_after(response.headers());
        match response.bytes().await {
            Ok(body) => judge(call.method, call.idempotent, status, retry_after, &body),
            Err(e) => transport(call, e),
        }
    }
}

/// A transport failure: retried when the call is idempotent or nothing reached Slack.
fn transport<T>(call: &Call<'_>, e: reqwest::Error) -> Attempt<T, FarmClientError> {
    // Without the URL: its query holds channel and user ids and cursors.
    let source = TransportError::from_reqwest(e.without_url());
    tracing::debug!(
        method = call.method,
        timeout = source.timeout,
        connect = source.connect,
        "slack request failed"
    );
    let retry = call.idempotent || source.connect;
    let error = FarmClientError::Unavailable {
        vendor: VENDOR,
        source: Some(source),
    };
    if retry {
        Attempt::Retry {
            error,
            retry_after: None,
        }
    } else {
        Attempt::Fail(error)
    }
}

/// What one answer means. The body is read here and dropped: only the short code survives.
fn judge<T: DeserializeOwned>(
    method: &'static str,
    idempotent: bool,
    status: StatusCode,
    retry_after: Option<Duration>,
    body: &[u8],
) -> Attempt<T, FarmClientError> {
    if status == StatusCode::TOO_MANY_REQUESTS {
        log_failure(method, status, "ratelimited");
        return rate_limited(retry_after);
    }
    let envelope = serde_json::from_slice::<wire::Envelope>(body).ok();
    if !status.is_success() {
        let code = envelope
            .and_then(|e| e.error)
            .map_or_else(|| format!("http_{}", status.as_u16()), short_code);
        log_failure(method, status, &code);
        if http::retryable_status(status) {
            return unavailable(idempotent, retry_after);
        }
        return Attempt::Fail(refusal(status, code));
    }
    match envelope {
        Some(wire::Envelope { ok: Some(true), .. }) => serde_json::from_slice::<T>(body)
            .map_or_else(
                |_| {
                    log_failure(method, status, "malformed");
                    Attempt::Fail(malformed())
                },
                Attempt::Done,
            ),
        Some(wire::Envelope {
            ok: Some(false),
            error,
        }) => {
            let code = error.map_or_else(|| UNRECOGNISED.to_owned(), short_code);
            log_failure(method, status, &code);
            match code.as_str() {
                "ratelimited" | "rate_limited" => rate_limited(retry_after),
                c if UNAVAILABLE_CODES.contains(&c) => unavailable(idempotent, retry_after),
                _ => Attempt::Fail(refusal(status, code)),
            }
        }
        _ => {
            log_failure(method, status, "malformed");
            Attempt::Fail(malformed())
        }
    }
}

fn log_failure(method: &'static str, status: StatusCode, code: &str) {
    tracing::debug!(method, status = status.as_u16(), code, "slack call failed");
}

fn rate_limited<T>(retry_after: Option<Duration>) -> Attempt<T, FarmClientError> {
    Attempt::Retry {
        error: FarmClientError::RateLimited {
            vendor: VENDOR,
            retry_after,
        },
        retry_after,
    }
}

fn unavailable<T>(idempotent: bool, retry_after: Option<Duration>) -> Attempt<T, FarmClientError> {
    let error = FarmClientError::Unavailable {
        vendor: VENDOR,
        source: None,
    };
    if idempotent {
        Attempt::Retry { error, retry_after }
    } else {
        Attempt::Fail(error)
    }
}

/// A refusal that another attempt would repeat.
fn refusal(status: StatusCode, code: String) -> FarmClientError {
    if AUTH_CODES.contains(&code.as_str()) {
        FarmClientError::AuthRefused {
            vendor: VENDOR,
            code,
        }
    } else if NOT_FOUND_CODES.contains(&code.as_str()) {
        FarmClientError::NotFound {
            vendor: VENDOR,
            code,
        }
    } else {
        FarmClientError::Refused {
            vendor: VENDOR,
            status: status.as_u16(),
            code,
        }
    }
}

const fn malformed() -> FarmClientError {
    FarmClientError::Malformed { vendor: VENDOR }
}

fn too_many_pages(method: &'static str) -> FarmClientError {
    tracing::debug!(method, pages = MAX_PAGES, "slack cursor never ended");
    malformed()
}

/// Slack's codes are short `snake_case` words. Anything else is replaced, so no text from a
/// body can reach an error, a log line or `last_error`.
fn short_code(raw: String) -> String {
    let plain = !raw.is_empty()
        && raw.len() <= MAX_CODE_LEN
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if plain { raw } else { UNRECOGNISED.to_owned() }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn message(m: wire::Message) -> SlackMessage {
    SlackMessage {
        ts: m.ts,
        user: m.user,
        text: m.text,
        thread_ts: m.thread_ts,
        reply_count: m.reply_count,
        latest_reply: m.latest_reply,
        subtype: m.subtype,
    }
}

/// `application/x-www-form-urlencoded`, without a dependency of its own: `url`'s serializer
/// through `reqwest::Url`.
fn form(pairs: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse("http://form.invalid/").expect("a fixed, valid URL");
    url.query_pairs_mut().extend_pairs(pairs);
    url.query().unwrap_or_default().to_owned()
}

/// A message's permalink, built from `auth.test`'s workspace URL ([connectors 6]):
/// `<url>archives/<channel>/p<ts without the dot>`, with `?thread_ts=<parent>&cid=<channel>`
/// for a reply. A parent (its `thread_ts` equal to its `ts`) gets the plain link.
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub fn permalink(workspace_url: &str, channel: &str, ts: &str, thread_ts: Option<&str>) -> String {
    let base = workspace_url.trim_end_matches('/');
    let id: String = ts.chars().filter(|c| *c != '.').collect();
    match thread_ts.filter(|parent| *parent != ts) {
        Some(parent) => format!("{base}/archives/{channel}/p{id}?thread_ts={parent}&cid={channel}"),
        None => format!("{base}/archives/{channel}/p{id}"),
    }
}
