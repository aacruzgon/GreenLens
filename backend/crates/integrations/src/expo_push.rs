//! Expo Push: the [`Notifier`] ([contracts 9], push notifications).
//!
//! `POST /--/api/v2/push/send` takes at most 100 messages; each message here has exactly one
//! token in `to`, so ticket *i* answers message *i*. A ticket error `DeviceNotRegistered` is
//! returned in [`PushReport::unregistered`] at once; ok tickets are kept in memory with their
//! token, and [`Notifier::check_receipts`] (called by `api`'s scheduler) asks
//! `getReceipts` about those at least 15 minutes old and returns the tokens whose receipt says
//! `DeviceNotRegistered`. The adapter never touches the store: `api` forgets the tokens. A push
//! is a hint, so the pending list is not persisted: a restart loses at most 15 minutes of
//! receipts, and the next push to a dead token reports it again.
//!
//! A request-level error (`errors`, with HTTP 400) refuses the whole batch, so one message
//! can't silence the others: a batch refused with `PUSH_TOO_MANY_EXPERIENCE_IDS` (tokens of two
//! Expo projects) is sent again as one batch per project, from the error's `details` (project
//! → its tokens), and any other refusal, or a project batch refused again, is sent one message
//! at a time. A single message refused is dropped and counted in [`PushReport::failed`]. A
//! transport error, `429` or a `5xx` drops its batch without splitting: sending more requests
//! wouldn't help.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use domain::{
    garden::push::{PushMessage, PushPayload},
    id::PushToken,
    ports::{Notifier, ProviderError, PushReport, ReceiptReport},
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::http::{self, TransportError};

/// The `upstream` name.
pub const PROVIDER: &str = "expo_push";

/// Messages per send request.
pub const MAX_MESSAGES: usize = 100;

/// Ids per receipts request.
pub const MAX_RECEIPT_IDS: usize = 1000;

/// Settings.
#[derive(Debug, Clone)]
pub struct ExpoPushConfig {
    /// `https://exp.host/--/api/v2/push`.
    pub base_url: String,
    /// Needed only when "enhanced push security" is on for the Expo project.
    pub access_token: Option<SecretString>,
    /// Deadline per request.
    pub timeout: Duration,
    /// How old a ticket must be before its receipt is asked for: 15 minutes.
    pub receipt_delay: Duration,
    /// Tickets without a receipt after this are dropped: Expo keeps receipts 24 hours.
    pub receipt_expiry: Duration,
}

impl Default for ExpoPushConfig {
    fn default() -> Self {
        Self {
            base_url: "https://exp.host/--/api/v2/push".to_owned(),
            access_token: None,
            timeout: Duration::from_secs(15),
            receipt_delay: Duration::from_mins(15),
            receipt_expiry: Duration::from_hours(24),
        }
    }
}

/// One message on the wire. Borrows the domain message (`'m`) for the length of the request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpoMessage<'m> {
    /// One token.
    pub to: &'m str,
    /// Title.
    pub title: &'m str,
    /// Body.
    pub body: &'m str,
    /// `PushPayload`.
    pub data: &'m PushPayload,
    /// `"default"`: the iOS sound.
    pub sound: &'static str,
    /// `"high"` for questions and alerts, so they wake the device.
    pub priority: &'static str,
}

impl<'m> From<&'m PushMessage> for ExpoMessage<'m> {
    fn from(m: &'m PushMessage) -> Self {
        Self {
            to: m.to.as_str(),
            title: &m.title,
            body: &m.body,
            data: &m.data,
            sound: "default",
            priority: "high",
        }
    }
}

/// `send`'s response.
#[derive(Debug, Deserialize)]
pub struct SendResponse {
    /// One ticket per message, in order.
    #[serde(default)]
    pub data: Vec<Ticket>,
    /// Request-level errors.
    #[serde(default)]
    pub errors: Vec<RequestError>,
}

/// A ticket or a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Ticket {
    /// Accepted; a receipt id for tickets (receipts have none).
    Ok {
        /// The receipt id.
        #[serde(default)]
        id: Option<String>,
    },
    /// Refused.
    Error {
        /// Why.
        #[serde(default)]
        message: String,
        /// `error`: `DeviceNotRegistered`, `MessageTooBig`, `MessageRateExceeded`, …
        #[serde(default)]
        details: Option<ErrorDetails>,
    },
}

impl Ticket {
    fn device_not_registered(&self) -> bool {
        matches!(self, Self::Error { details: Some(ErrorDetails { error: Some(e) }), .. } if e == "DeviceNotRegistered")
    }
}

/// `details`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ErrorDetails {
    /// The error code.
    #[serde(default)]
    pub error: Option<String>,
}

/// A request-level error.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RequestError {
    /// `TOO_MANY_REQUESTS`, `PUSH_TOO_MANY_EXPERIENCE_IDS`, `PUSH_TOO_MANY_NOTIFICATIONS`,
    /// `UNAUTHORIZED`, …
    pub code: String,
    /// Text.
    #[serde(default)]
    pub message: String,
    /// For `PUSH_TOO_MANY_EXPERIENCE_IDS`, each Expo project's tokens in the request
    /// (`{ "@owner/slug": ["ExponentPushToken[…]", …] }`; the shape is *unverified*, from
    /// Expo's description, so anything else falls back to one message at a time).
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

/// `PUSH_TOO_MANY_EXPERIENCE_IDS`: the batch mixes tokens of several Expo projects.
pub const TOO_MANY_EXPERIENCE_IDS: &str = "PUSH_TOO_MANY_EXPERIENCE_IDS";

/// How far a refused batch has been split already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Split {
    /// A batch of up to 100 messages, as `push` cut it.
    Batch,
    /// One Expo project's messages of a refused batch.
    Project,
    /// One message.
    Single,
}

/// What to send again after a request-level refusal of `batch` at `level`, or `None` to drop
/// it: a mixed-project batch splits by project (messages whose token `details` doesn't name are
/// sent alone), anything else one message at a time. A rate limit is never split, and a single
/// message has nothing left to split.
fn split<'m>(
    batch: &[&'m PushMessage],
    errors: &[RequestError],
    level: Split,
) -> Option<Vec<(Vec<&'m PushMessage>, Split)>> {
    if level == Split::Single || batch.len() < 2 {
        return None;
    }
    if errors.iter().any(|e| e.code == "TOO_MANY_REQUESTS") {
        return None;
    }
    let singles = |messages: &[&'m PushMessage]| -> Vec<(Vec<&'m PushMessage>, Split)> {
        messages.iter().map(|m| (vec![*m], Split::Single)).collect()
    };
    let projects = errors
        .iter()
        .find(|e| e.code == TOO_MANY_EXPERIENCE_IDS)
        .and_then(|e| e.details.clone())
        .and_then(|d| serde_json::from_value::<BTreeMap<String, Vec<String>>>(d).ok());
    let Some(projects) = projects.filter(|_| level == Split::Batch) else {
        return Some(singles(batch));
    };
    let mut parts: Vec<(Vec<&'m PushMessage>, Split)> = Vec::new();
    let mut named = std::collections::HashSet::new();
    for tokens in projects.values() {
        let group: Vec<&PushMessage> = batch
            .iter()
            .copied()
            .filter(|m| tokens.iter().any(|t| t == m.to.as_str()))
            .collect();
        named.extend(group.iter().map(|m| m.to.as_str()));
        if !group.is_empty() {
            parts.push((group, Split::Project));
        }
    }
    let unnamed: Vec<&PushMessage> = batch
        .iter()
        .copied()
        .filter(|m| !named.contains(m.to.as_str()))
        .collect();
    parts.extend(singles(&unnamed));
    // A "split" that gives the whole batch back would be refused the same way.
    if parts.len() < 2 {
        return Some(singles(batch));
    }
    Some(parts)
}

#[derive(Debug, Deserialize)]
struct ReceiptsResponse {
    #[serde(default)]
    data: std::collections::HashMap<String, Ticket>,
}

/// A ticket waiting for its receipt.
#[derive(Debug, Clone)]
struct Pending {
    receipt_id: String,
    token: PushToken,
    sent_at: Instant,
}

/// The Expo adapter. `Clone` shares the pending tickets.
#[derive(Clone)]
pub struct ExpoPush {
    http: reqwest::Client,
    config: Arc<ExpoPushConfig>,
    pending: Arc<std::sync::Mutex<Vec<Pending>>>,
}

impl std::fmt::Debug for ExpoPush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpoPush")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ExpoPush {
    /// An adapter over the shared client.
    pub fn new(http: reqwest::Client, config: ExpoPushConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
            pending: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// POSTs JSON and decodes the answer; 429 and 5xx are errors with the status.
    async fn post<B: Serialize + ?Sized, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, ProviderError> {
        let (status, bytes) = self.post_raw(path, body).await?;
        if !status.is_success() {
            return Err(refused(status, &bytes));
        }
        decode(&bytes)
    }

    /// One `send` request. A `400` whose body lists request-level `errors` is an answer, not an
    /// error, so `push` can split the batch.
    async fn send(&self, messages: &[ExpoMessage<'_>]) -> Result<SendResponse, ProviderError> {
        let (status, bytes) = self.post_raw("/send", messages).await?;
        if status.is_success() {
            return decode(&bytes);
        }
        if status == reqwest::StatusCode::BAD_REQUEST
            && let Ok(response) = serde_json::from_slice::<SendResponse>(&bytes)
            && !response.errors.is_empty()
        {
            return Ok(response);
        }
        Err(refused(status, &bytes))
    }

    /// POSTs JSON: the status and body, or an error for a transport failure, `429` or a `5xx`.
    async fn post_raw<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(reqwest::StatusCode, bytes::Bytes), ProviderError> {
        let mut builder = self
            .http
            .post(format!(
                "{}{path}",
                self.config.base_url.trim_end_matches('/')
            ))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(body)
            .timeout(self.config.timeout);
        if let Some(t) = &self.config.access_token {
            builder = builder.bearer_auth(t.expose_secret());
        }
        let response = builder
            .send()
            .await
            .map_err(|e| ProviderError::Unavailable {
                provider: PROVIDER,
                source: Box::new(TransportError::from_reqwest(e)),
            })?;
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                provider: PROVIDER,
                retry_after_s: http::retry_after(response.headers())
                    .and_then(|d| u32::try_from(d.as_secs()).ok()),
            });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ProviderError::Unavailable {
                provider: PROVIDER,
                source: Box::new(TransportError::from_reqwest(e)),
            })?;
        if status.is_server_error() {
            return Err(ProviderError::Unavailable {
                provider: PROVIDER,
                source: format!("HTTP {status}").into(),
            });
        }
        Ok((status, bytes))
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, Vec<Pending>> {
        // A poisoned lock only means another thread panicked mid-push; the list is still usable.
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A `4xx` other than `429`, with the start of its body.
fn refused(status: reqwest::StatusCode, bytes: &[u8]) -> ProviderError {
    ProviderError::BadResponse {
        provider: PROVIDER,
        source: format!(
            "HTTP {status}: {}",
            http::snippet(&String::from_utf8_lossy(bytes), 256)
        )
        .into(),
    }
}

fn decode<R: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<R, ProviderError> {
    serde_json::from_slice(bytes).map_err(|e| ProviderError::BadResponse {
        provider: PROVIDER,
        source: Box::new(e),
    })
}

#[async_trait]
impl Notifier for ExpoPush {
    async fn push(&self, messages: Vec<PushMessage>) -> Result<PushReport, ProviderError> {
        let mut report = PushReport::default();
        let mut last_error = None;
        let mut any_sent = false;
        let mut queue: VecDeque<(Vec<&PushMessage>, Split)> = messages
            .chunks(MAX_MESSAGES)
            .map(|chunk| (chunk.iter().collect(), Split::Batch))
            .collect();
        while let Some((batch, level)) = queue.pop_front() {
            let wire: Vec<ExpoMessage<'_>> = batch.iter().map(|m| ExpoMessage::from(*m)).collect();
            let response = match self.send(&wire).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(provider = PROVIDER, error = %e, messages = batch.len(), "push batch failed; dropped");
                    report.failed += count(batch.len());
                    last_error = Some(e);
                    continue;
                }
            };
            if !response.errors.is_empty() {
                let codes: Vec<&str> = response.errors.iter().map(|e| e.code.as_str()).collect();
                if let Some(parts) = split(&batch, &response.errors, level) {
                    tracing::info!(
                        provider = PROVIDER,
                        ?codes,
                        messages = batch.len(),
                        parts = parts.len(),
                        "push batch refused; sent again in parts"
                    );
                    queue.extend(parts);
                    continue;
                }
                tracing::warn!(
                    provider = PROVIDER,
                    ?codes,
                    messages = batch.len(),
                    "push request refused; dropped"
                );
                report.failed += count(batch.len());
                last_error = Some(ProviderError::BadResponse {
                    provider: PROVIDER,
                    source: codes.join(",").into(),
                });
                continue;
            }
            any_sent = true;
            self.read_tickets(&batch, response.data, &mut report);
        }
        match last_error {
            Some(e) if !any_sent => Err(e),
            _ => Ok(report),
        }
    }

    async fn check_receipts(&self) -> Result<ReceiptReport, ProviderError> {
        let now = Instant::now();
        let due: Vec<Pending> = {
            let mut pending = self.pending();
            pending.retain(|p| now.duration_since(p.sent_at) < self.config.receipt_expiry);
            pending
                .iter()
                .filter(|p| now.duration_since(p.sent_at) >= self.config.receipt_delay)
                .cloned()
                .collect()
        };
        let mut report = ReceiptReport {
            checked: count(due.len()),
            ..ReceiptReport::default()
        };
        let mut answered = std::collections::HashSet::new();
        for chunk in due.chunks(MAX_RECEIPT_IDS) {
            let ids: Vec<&str> = chunk.iter().map(|p| p.receipt_id.as_str()).collect();
            let response: ReceiptsResponse = self
                .post("/getReceipts", &serde_json::json!({ "ids": ids }))
                .await?;
            for p in chunk {
                if let Some(receipt) = response.data.get(&p.receipt_id) {
                    answered.insert(p.receipt_id.clone());
                    match receipt {
                        Ticket::Ok { .. } => report.ok += 1,
                        Ticket::Error { .. } => report.errors += 1,
                    }
                    if receipt.device_not_registered() {
                        report.unregistered.push(p.token.clone());
                    }
                }
            }
        }
        // Checked ids are dropped; ids Expo has no receipt for yet stay until they expire.
        self.pending().retain(|p| !answered.contains(&p.receipt_id));
        Ok(report)
    }
}

impl ExpoPush {
    /// A sent batch's tickets, in message order: accepted ones wait for their receipt,
    /// `DeviceNotRegistered` tokens are reported, other errors and missing tickets counted.
    fn read_tickets(&self, batch: &[&PushMessage], tickets: Vec<Ticket>, report: &mut PushReport) {
        report.failed += count(batch.len().saturating_sub(tickets.len()));
        let now = Instant::now();
        let mut pending = self.pending();
        for (message, ticket) in batch.iter().zip(tickets) {
            match ticket {
                Ticket::Ok { id } => {
                    report.accepted += 1;
                    if let Some(receipt_id) = id {
                        pending.push(Pending {
                            receipt_id,
                            token: message.to.clone(),
                            sent_at: now,
                        });
                    }
                }
                ref t if t.device_not_registered() => {
                    report.unregistered.push(message.to.clone());
                }
                Ticket::Error { message: why, .. } => {
                    report.failed += 1;
                    tracing::info!(provider = PROVIDER, reason = %why, "push ticket error");
                }
            }
        }
    }
}

/// A count of messages for a report (batches hold at most 100, receipts 1000 at a time).
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use domain::{
        garden::push::PushScanReason,
        id::{GardenId, PlantId, ScanId},
        wire::V1,
    };
    use serde_json::json;
    use ulid::Ulid;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path},
    };

    use super::*;

    fn message(token: &str) -> PushMessage {
        PushMessage {
            to: token.parse().unwrap(),
            title: "Plan ready".into(),
            body: "Your tomato plan is ready.".into(),
            data: PushPayload::Scan {
                v: V1,
                reason: PushScanReason::PlanReady,
                garden_id: GardenId::from_ulid(Ulid::nil()),
                plant_id: PlantId::from_ulid(Ulid::nil()),
                scan_id: ScanId::from_ulid(Ulid::nil()),
            },
        }
    }

    fn adapter(server: &MockServer) -> ExpoPush {
        ExpoPush::new(
            reqwest::Client::new(),
            ExpoPushConfig {
                base_url: server.uri(),
                receipt_delay: Duration::ZERO,
                ..ExpoPushConfig::default()
            },
        )
    }

    #[tokio::test]
    async fn tickets_map_to_tokens_and_receipts_are_checked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/send"))
            .and(body_partial_json(json!([{"to": "ExponentPushToken[a]",
                "data": {"kind": "scan", "reason": "plan_ready", "v": 1}}])))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
                {"status": "ok", "id": "r-a"},
                {"status": "error", "message": "gone", "details": {"error": "DeviceNotRegistered"}},
                {"status": "ok", "id": "r-c"}
            ]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/getReceipts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
                "r-a": {"status": "ok"},
                "r-c": {"status": "error", "message": "gone", "details": {"error": "DeviceNotRegistered"}}
            }})))
            .expect(1)
            .mount(&server)
            .await;
        let expo = adapter(&server);
        let report = expo
            .push(vec![
                message("ExponentPushToken[a]"),
                message("ExponentPushToken[b]"),
                message("ExponentPushToken[c]"),
            ])
            .await
            .unwrap();
        assert_eq!(report.accepted, 2);
        assert_eq!(
            report.unregistered,
            vec!["ExponentPushToken[b]".parse().unwrap()]
        );
        assert_eq!(report.failed, 0);
        let receipts = expo.check_receipts().await.unwrap();
        assert_eq!(
            receipts,
            ReceiptReport {
                checked: 2,
                ok: 1,
                errors: 1,
                unregistered: vec!["ExponentPushToken[c]".parse::<PushToken>().unwrap()],
            }
        );
        assert_eq!(
            expo.check_receipts().await.unwrap(),
            ReceiptReport::default(),
            "checked ids are dropped"
        );
    }

    /// The `to` of every message of a `send` body.
    fn tokens_of(request: &wiremock::Request) -> Vec<String> {
        let body: Vec<serde_json::Value> = serde_json::from_slice(&request.body).unwrap();
        body.iter()
            .map(|m| m["to"].as_str().unwrap().to_owned())
            .collect()
    }

    /// Matches a `send` whose messages go to exactly these tokens, in order.
    fn to(tokens: &'static [&'static str]) -> impl Fn(&wiremock::Request) -> bool {
        move |request: &wiremock::Request| tokens_of(request) == tokens
    }

    fn oks(n: usize) -> ResponseTemplate {
        let ok: Vec<_> = (0..n)
            .map(|i| json!({"status": "ok", "id": format!("r-{i}")}))
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({ "data": ok }))
    }

    /// Tokens of two Expo projects in one batch: Expo refuses the whole request (HTTP 400) and
    /// names each project's tokens; the batch is sent again as one request per project.
    #[tokio::test]
    async fn a_mixed_project_batch_is_split_by_project() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/send"))
            .and(to(&["ExponentPushToken[a]", "ExpoPushToken[b]", "ExponentPushToken[c]"]))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"errors": [{
                "code": "PUSH_TOO_MANY_EXPERIENCE_IDS",
                "message": "All push notification messages in the same request must be for the same project",
                "details": {
                    "@pdm/pdm": ["ExponentPushToken[a]", "ExponentPushToken[c]"],
                    "@someone/other": ["ExpoPushToken[b]"]
                }
            }]})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/send"))
            .and(to(&["ExponentPushToken[a]", "ExponentPushToken[c]"]))
            .respond_with(oks(2))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/send"))
            .and(to(&["ExpoPushToken[b]"]))
            .respond_with(oks(1))
            .expect(1)
            .mount(&server)
            .await;
        let report = adapter(&server)
            .push(vec![
                message("ExponentPushToken[a]"),
                message("ExpoPushToken[b]"),
                message("ExponentPushToken[c]"),
            ])
            .await
            .unwrap();
        assert_eq!(report.accepted, 3);
        assert_eq!(report.failed, 0);
    }

    /// Any other request-level refusal is sent again one message at a time, so only the message
    /// Expo can't take is lost.
    #[tokio::test]
    async fn a_refused_batch_falls_back_to_one_message_at_a_time() {
        let server = MockServer::start().await;
        let refused = || {
            ResponseTemplate::new(400).set_body_json(json!({"errors": [{
                "code": "VALIDATION_ERROR",
                "message": "\"[0].to\" must be a valid push token"
            }]}))
        };
        Mock::given(method("POST"))
            .and(path("/send"))
            .and(to(&[
                "ExponentPushToken[a]",
                "junk",
                "ExponentPushToken[c]",
            ]))
            .respond_with(refused())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/send"))
            .and(to(&["junk"]))
            .respond_with(refused())
            .expect(1)
            .mount(&server)
            .await;
        for token in [&["ExponentPushToken[a]"], &["ExponentPushToken[c]"]] {
            Mock::given(method("POST"))
                .and(path("/send"))
                .and(to(token))
                .respond_with(oks(1))
                .expect(1)
                .mount(&server)
                .await;
        }
        let report = adapter(&server)
            .push(vec![
                message("ExponentPushToken[a]"),
                message("junk"),
                message("ExponentPushToken[c]"),
            ])
            .await
            .unwrap();
        assert_eq!(report.accepted, 2);
        assert_eq!(report.failed, 1, "the junk message alone is lost");
    }

    /// A refusal that isn't about the messages (a bad access token) is not split: every message
    /// would get the same answer.
    #[tokio::test]
    async fn an_unauthorized_batch_is_not_split() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/send"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"errors": [{
                "code": "UNAUTHORIZED", "message": "bad access token"
            }]})))
            .expect(1)
            .mount(&server)
            .await;
        let err = adapter(&server)
            .push(vec![
                message("ExponentPushToken[a]"),
                message("ExponentPushToken[b]"),
            ])
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::BadResponse { .. }));
    }

    #[tokio::test]
    async fn batches_of_100() {
        let server = MockServer::start().await;
        let ok: Vec<_> = (0..100).map(|_| json!({"status": "ok"})).collect();
        Mock::given(method("POST"))
            .and(path("/send"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": ok})))
            .expect(3)
            .mount(&server)
            .await;
        let msgs = (0..250)
            .map(|i| message(&format!("ExponentPushToken[{i}]")))
            .collect();
        adapter(&server).push(msgs).await.unwrap();
    }

    #[tokio::test]
    async fn whole_failure_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let err = adapter(&server)
            .push(vec![message("ExponentPushToken[a]")])
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }
}
