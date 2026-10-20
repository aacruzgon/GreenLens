//! [`SlackSource`]: the `FarmSource` port over [`SlackClient`] ([connectors 6], docs/EON E4.2).
//!
//! One page is one `conversations.history` page of one channel, with the replies of the threads
//! on it that changed and the authors' names. The adapter's state is its [`SyncCursor`], a small
//! JSON object only this module reads:
//!
//! - `newest`: the newest `ts` of every finished pass. A later pass reads only what is after it
//!   (`oldest` is exclusive), so a quiet channel costs one `conversations.history` call.
//! - `pass`: a pass with pages left: its `oldest`, Slack's page cursor and the newest `ts` seen
//!   so far. A throttled first sync resumes from it on a later run, reading no page twice.
//! - `threads`: per thread parent, its `latest_reply`, its `reply_count` and a short hash of its
//!   text when its replies were last read. A parent seen again with all three unchanged isn't
//!   sent again (its id is [`FarmPage::kept`]) and costs no `conversations.replies` call. A
//!   deleted reply changes `reply_count`, so the thread is read again; an edited reply changes
//!   neither and is missed.
//! - `probed`: the parent `ts` of the last older thread checked (below), the round-robin
//!   position.
//!
//! The daily re-read ([`FarmPageRequest::reread_from`]) starts a pass from 7 days back. Slack's
//! history never reports a deletion, so the sync deletes what that pass didn't see. A new reply
//! to a thread shows in its parent's `latest_reply`, which a pass reads when the parent is in
//! its window: a later pass when the parent is new, the daily re-read for the last 7 days.
//!
//! **Replies to older threads.** `conversations.history` lists a parent by its own time, so a
//! reply to a thread whose parent is older than 7 days is seen by no pass. At the end of a pass
//! the adapter therefore checks up to [`FarmPageRequest::probe_limit`] threads from `threads`
//! whose parent is older than 7 days and whose last reply is within 30 days, round-robin by
//! `probed`, with `conversations.replies` and `limit=1`: the parent comes back first with its
//! current `latest_reply` and `reply_count`, and the thread is read in full when either
//! differs. The sync allows two such checks per connector run, over all its channels, so they
//! never crowd out the replies of threads that changed under Tier 1 pacing.
//!
//! Authors: `users.info` once per user and team a day, keeping one name ([`SlackUser::label`]),
//! never the email. Links: built from the workspace URL `auth.test` gave at connect
//! ([`ConnectorAccount::Slack`]'s `url`).
//!
//! [connectors 6]: ../../../docs/connectors/README.md#6-sync

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use aws_lc_rs::digest;
use moka::future::Cache;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use domain::{
    connectors::{ConnectorAccount, ConnectorKind, ConnectorSource, FarmRecordKind},
    ports::{
        FarmItem, FarmPage, FarmPageRequest, FarmSource, FarmSourceError, SyncCursor, VendorToken,
    },
    rules::limits::FARM_RECORD_KEEP,
    wire::Timestamp,
};

use super::{SlackClient, SlackMessage, permalink};
use crate::farm::FarmClientError;

/// The trailing window read again once a day ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const REREAD_WINDOW: Duration = Duration::from_hours(7 * 24);

/// A thread's parent must be older than this to be checked by the round-robin (younger ones are
/// in the daily re-read's window).
const PROBE_PARENT_AGE: Duration = REREAD_WINDOW;

/// A thread is checked only if its last reply is newer than this.
const PROBE_REPLY_AGE: Duration = Duration::from_hours(30 * 24);

/// How long an author's name is kept per team ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const USER_NAME_TTL: Duration = Duration::from_hours(24);

/// Most names kept at once, over every team.
const MAX_NAMES: u64 = 100_000;

/// Message subtypes kept: plain messages and those carrying someone's text. Joins, topic
/// changes, pins and the like are left out; a reply sent to the channel too
/// (`thread_broadcast`) arrives with its thread.
const KEPT_SUBTYPES: [&str; 3] = ["bot_message", "file_share", "me_message"];

/// The farm-connector adapter for Slack: one per process and per endpoint (the real Slack, or
/// an Era simulator), sharing the client's pacers and its own name cache.
#[derive(Clone)]
pub struct SlackSource {
    client: SlackClient,
    /// `(team, user)` → the user's name, or `None` for a user Slack doesn't know.
    names: Cache<(String, String), Option<Arc<str>>>,
}

impl std::fmt::Debug for SlackSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackSource")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

/// The adapter's cursor, serialised into [`SyncCursor`].
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cursor {
    /// The newest `ts` of the finished passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    newest: Option<String>,
    /// The pass with pages left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pass: Option<Pass>,
    /// Thread parent `ts` → what its replies were read at.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    threads: BTreeMap<String, Thread>,
    /// The parent `ts` of the last older thread checked: the round-robin position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    probed: Option<String>,
}

/// What the older-thread checks of one page found.
#[derive(Debug, Default)]
struct Probed {
    /// The threads that changed, read in full.
    items: Vec<FarmItem>,
    /// `conversations.replies` checks made (`limit=1`), failed ones included.
    calls: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Pass {
    /// The pass's `oldest`, which Slack's page cursor belongs to.
    oldest: String,
    /// Slack's `next_cursor`.
    page: String,
    /// The newest `ts` this pass has seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    top: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Thread {
    /// The parent's `latest_reply` when the replies were read.
    latest_reply: String,
    /// The parent's `reply_count` then: a deleted reply lowers it with `latest_reply` unchanged.
    /// Cursors from before it was kept read as 0, so each thread is read once more.
    #[serde(default)]
    reply_count: u32,
    /// The first 8 bytes of the parent text's SHA-256, in hex: an edited parent is read again.
    text: String,
}

impl Cursor {
    /// A cursor this module wrote; anything else (an older shape, a cursor from another
    /// vendor) starts afresh, which re-reads but never loses a record.
    fn parse(cursor: Option<&SyncCursor>) -> Self {
        cursor
            .and_then(|c| serde_json::from_str(&c.0).ok())
            .unwrap_or_default()
    }

    /// After a history page: the pass continues from Slack's `next` page cursor, or it ended
    /// and `newest` moves up to the newest `ts` it saw.
    fn after_page(&mut self, oldest: String, next: Option<String>, top: Option<String>) {
        match next {
            Some(page) => self.pass = Some(Pass { oldest, page, top }),
            None => {
                self.newest = [self.newest.take(), top]
                    .into_iter()
                    .flatten()
                    .max_by_key(|t| ts_key(t));
            }
        }
    }

    fn into_sync_cursor(self) -> SyncCursor {
        SyncCursor(serde_json::to_string(&self).expect("a cursor of strings serialises"))
    }
}

impl SlackSource {
    /// An adapter over `client`.
    pub fn new(client: SlackClient) -> Self {
        Self {
            client,
            names: Cache::builder()
                .max_capacity(MAX_NAMES)
                .time_to_live(USER_NAME_TTL)
                .build(),
        }
    }

    /// The client underneath, for the connect routes (`auth.test`, the code exchange).
    pub fn client(&self) -> &SlackClient {
        &self.client
    }

    /// A user's name, from the cache or `users.info`. `None` for a user Slack doesn't know; any
    /// other failure stops the page, so the sync retries it.
    async fn author(
        &self,
        token: &SecretString,
        team: &str,
        user: &str,
    ) -> Result<Option<Arc<str>>, FarmClientError> {
        let key = (team.to_owned(), user.to_owned());
        if let Some(name) = self.names.get(&key).await {
            return Ok(name);
        }
        let name = match self.client.users_info(token, user).await {
            Ok(u) => Some(Arc::from(u.label())),
            Err(FarmClientError::NotFound { .. }) => None,
            Err(e) => return Err(e),
        };
        self.names.insert(key, name.clone()).await;
        Ok(name)
    }

    async fn author_of(
        &self,
        token: &SecretString,
        team: &str,
        message: &SlackMessage,
    ) -> Result<Option<Arc<str>>, FarmClientError> {
        match message.user.as_deref() {
            Some(user) => self.author(token, team, user).await,
            None => Ok(None),
        }
    }

    /// The replies of a thread, without its parent. A thread gone since the history page
    /// (`thread_not_found`) has none.
    async fn replies(
        &self,
        token: &SecretString,
        team: &str,
        channel: &str,
        parent: &str,
    ) -> Result<Vec<SlackMessage>, FarmClientError> {
        match self
            .client
            .conversations_replies(token, team, channel, parent)
            .await
        {
            Ok(messages) => Ok(messages
                .into_iter()
                .filter(|m| m.ts != parent && kept(m))
                .collect()),
            Err(FarmClientError::NotFound { .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// One history page, as the port's page.
    async fn page(
        &self,
        token: &SecretString,
        request: &FarmPageRequest,
    ) -> Result<FarmPage, FarmClientError> {
        let ConnectorAccount::Slack { team_id, url, .. } = &request.account else {
            return Err(FarmClientError::Malformed {
                vendor: super::VENDOR,
            });
        };
        let channel = request.source.id.as_str();
        let floor = unix_ts(request.oldest);
        let mut cursor = Cursor::parse(request.cursor.as_ref());
        let (oldest, page) = if let Some(pass) = cursor.pass.take() {
            (pass.oldest, Some((pass.page, pass.top)))
        } else {
            let from = match (request.reread_from, cursor.newest.as_deref()) {
                (Some(from), _) => unix_ts(from),
                (None, Some(newest)) => newest.to_owned(),
                (None, None) => floor.clone(),
            };
            (later_ts(from, &floor), None)
        };
        let (page_cursor, top) = match page {
            Some((page, top)) => (Some(page), top),
            None => (None, None),
        };
        let history = self
            .client
            .conversations_history(
                token,
                team_id,
                channel,
                Some(&oldest),
                page_cursor.as_deref(),
            )
            .await?;

        let mut items = Vec::new();
        let mut kept_ids = Vec::new();
        let mut page_top = top;
        for message in dedupe(history.messages) {
            page_top = Some(match page_top {
                Some(t) if ts_key(&t) >= ts_key(&message.ts) => t,
                _ => message.ts.clone(),
            });
            if !kept(&message) || message.is_reply() {
                continue;
            }
            let vendor_id = format!("{channel}/{}", message.ts);
            let replies = if message.reply_count > 0 {
                let latest = message.latest_reply.clone().unwrap_or_default();
                let read = Thread {
                    latest_reply: latest,
                    reply_count: message.reply_count,
                    text: short_hash(&message.text),
                };
                if cursor.threads.get(&message.ts) == Some(&read) {
                    kept_ids.push(vendor_id);
                    continue;
                }
                let replies = self.replies(token, team_id, channel, &message.ts).await?;
                cursor.threads.insert(message.ts.clone(), read);
                replies
            } else {
                Vec::new()
            };
            items.push(
                self.item(token, team_id, url, &request.source, message, replies)
                    .await?,
            );
        }

        let probed = if history.next_cursor.is_none() && request.probe_limit > 0 {
            self.probe_older_threads(token, team_id, url, request, &mut cursor)
                .await?
        } else {
            Probed::default()
        };
        items.extend(probed.items);

        let more = history.next_cursor.is_some();
        cursor.after_page(oldest, history.next_cursor, page_top);
        // Threads whose parent left the 180 days are forgotten with their records.
        let floor_key = ts_key(&floor);
        cursor.threads.retain(|ts, _| ts_key(ts) >= floor_key);
        Ok(FarmPage {
            items,
            removed: Vec::new(),
            kept: kept_ids,
            cursor: cursor.into_sync_cursor(),
            more,
            probes: probed.calls,
        })
    }

    /// Checks up to `request.probe_limit` older threads for replies no pass would see: the
    /// next ones after `cursor.probed` among the threads whose parent is older than 7 days and
    /// whose last reply is within 30 days of the request's now. A thread whose `latest_reply` or
    /// `reply_count` moved is read in full and returned as an item; one Slack no longer has is
    /// forgotten.
    async fn probe_older_threads(
        &self,
        token: &SecretString,
        team_id: &str,
        url: &str,
        request: &FarmPageRequest,
        cursor: &mut Cursor,
    ) -> Result<Probed, FarmClientError> {
        let channel = request.source.id.as_str();
        let now = request.oldest.plus(FARM_RECORD_KEEP);
        let parent_before = now.minus(PROBE_PARENT_AGE).unix_seconds();
        let reply_after = now.minus(PROBE_REPLY_AGE).unix_seconds();
        let secs = |ts: &str| i64::try_from(ts_key(ts).0).unwrap_or(0);
        let mut due: Vec<&String> = cursor
            .threads
            .iter()
            .filter(|(ts, t)| secs(ts) < parent_before && secs(&t.latest_reply) >= reply_after)
            .map(|(ts, _)| ts)
            .collect();
        due.sort_by_key(|ts| ts_key(ts));
        let after = cursor.probed.as_deref().map(ts_key);
        let (later, earlier): (Vec<&String>, Vec<&String>) =
            due.into_iter().partition(|ts| Some(ts_key(ts)) > after);
        let chosen: Vec<String> = later
            .into_iter()
            .chain(earlier)
            .take(usize::try_from(request.probe_limit).unwrap_or(usize::MAX))
            .cloned()
            .collect();
        let mut probed = Probed::default();
        for ts in chosen {
            probed.calls += 1;
            cursor.probed = Some(ts.clone());
            let found = match self
                .client
                .conversations_thread_parent(token, team_id, channel, &ts)
                .await
            {
                Ok(found) => found,
                Err(FarmClientError::NotFound { .. }) => None,
                Err(e) => return Err(e),
            };
            let Some(parent) = found else {
                cursor.threads.remove(&ts);
                continue;
            };
            let latest = parent.latest_reply.clone().unwrap_or_default();
            let stored = cursor.threads.get(&ts);
            if stored
                .is_some_and(|t| t.latest_reply == latest && t.reply_count == parent.reply_count)
            {
                continue;
            }
            let replies = self.replies(token, team_id, channel, &ts).await?;
            if parent.reply_count > 0 {
                cursor.threads.insert(
                    ts.clone(),
                    Thread {
                        latest_reply: latest,
                        reply_count: parent.reply_count,
                        text: short_hash(&parent.text),
                    },
                );
            } else {
                cursor.threads.remove(&ts);
            }
            probed.items.push(
                self.item(token, team_id, url, &request.source, parent, replies)
                    .await?,
            );
        }
        Ok(probed)
    }

    /// A message with its replies joined, each reply on its own paragraph after its author's
    /// name. The chunker cuts it at the record limit.
    async fn item(
        &self,
        token: &SecretString,
        team: &str,
        workspace_url: &str,
        source: &ConnectorSource,
        parent: SlackMessage,
        replies: Vec<SlackMessage>,
    ) -> Result<FarmItem, FarmClientError> {
        let author = self.author_of(token, team, &parent).await?;
        let mut text = parent.text.clone();
        for reply in &replies {
            let name = self.author_of(token, team, reply).await?;
            text.push_str("\n\n");
            text.push_str(name.as_deref().unwrap_or("Someone"));
            text.push_str(": ");
            text.push_str(&reply.text);
        }
        Ok(FarmItem {
            vendor_id: format!("{}/{}", source.id, parent.ts),
            kind: FarmRecordKind::Message,
            title: format!("#{}", source.name),
            author: author.map(|a| a.to_string()),
            link: permalink(workspace_url, &source.id, &parent.ts, None),
            text,
            at: ts_time(&parent.ts),
        })
    }
}

#[async_trait]
impl FarmSource for SlackSource {
    fn kind(&self) -> ConnectorKind {
        ConnectorKind::Slack
    }

    fn reread_window(&self) -> Option<Duration> {
        Some(REREAD_WINDOW)
    }

    async fn available_sources(
        &self,
        token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError> {
        let token = SecretString::from(token.expose());
        let channels = self
            .client
            .conversations_list(&token)
            .await
            .map_err(|e| e.into_source_error(ConnectorKind::Slack, None))?;
        Ok(channels
            .into_iter()
            .map(|c| ConnectorSource {
                id: c.id,
                name: c.name,
            })
            .collect())
    }

    async fn read_page(
        &self,
        token: &VendorToken,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        let secret = SecretString::from(token.expose());
        self.page(&secret, &request)
            .await
            .map_err(|e| e.into_source_error(ConnectorKind::Slack, Some(&request.source.id)))
    }

    async fn revoke(&self, token: &VendorToken) -> Result<(), FarmSourceError> {
        let token = SecretString::from(token.expose());
        self.client
            .auth_revoke(&token)
            .await
            .map_err(|e| e.into_source_error(ConnectorKind::Slack, None))
    }
}

/// Keeps a message that carries someone's text.
fn kept(message: &SlackMessage) -> bool {
    message
        .subtype
        .as_deref()
        .is_none_or(|s| KEPT_SUBTYPES.contains(&s))
}

/// One message per `ts`, in page order. Era's simulator lists a parent injected again (to add
/// a reply) twice; the copy with the newest `latest_reply` wins.
fn dedupe(messages: Vec<SlackMessage>) -> Vec<SlackMessage> {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<SlackMessage> = Vec::with_capacity(messages.len());
    for message in messages {
        if let Some(&i) = at.get(&message.ts) {
            let newer = message.latest_reply.as_deref().map(ts_key)
                > out[i].latest_reply.as_deref().map(ts_key);
            if newer {
                out[i] = message;
            }
        } else {
            at.insert(message.ts.clone(), out.len());
            out.push(message);
        }
    }
    out
}

/// A `ts` (`"1791829800.012100"`) as numbers, for ordering; a malformed one sorts first.
fn ts_key(ts: &str) -> (u64, u32) {
    let (secs, frac) = ts.split_once('.').unwrap_or((ts, "0"));
    let frac: String = frac.chars().chain(std::iter::repeat('0')).take(6).collect();
    (secs.parse().unwrap_or(0), frac.parse().unwrap_or(0))
}

/// The later of two `ts` values.
fn later_ts(a: String, b: &str) -> String {
    if ts_key(&a) >= ts_key(b) {
        a
    } else {
        b.to_owned()
    }
}

/// An instant as a whole-second `ts`, for `oldest`.
fn unix_ts(t: Timestamp) -> String {
    t.unix_seconds().to_string()
}

/// A message's time, to the second.
fn ts_time(ts: &str) -> Timestamp {
    let secs = i64::try_from(ts_key(ts).0).unwrap_or(0);
    chrono::DateTime::from_timestamp(secs, 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .into()
}

/// The first 8 bytes of a text's SHA-256, in hex.
fn short_hash(text: &str) -> String {
    let d = digest::digest(&digest::SHA256, text.as_bytes());
    data_encoding::HEXLOWER.encode(&d.as_ref()[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(ts: &str, latest: Option<&str>) -> SlackMessage {
        SlackMessage {
            ts: ts.into(),
            user: Some("U1".into()),
            text: "t".into(),
            thread_ts: latest.map(|_| ts.into()),
            reply_count: u32::from(latest.is_some()),
            latest_reply: latest.map(Into::into),
            subtype: None,
        }
    }

    #[test]
    fn ts_values_order_by_number_not_by_text() {
        assert_eq!(ts_key("1791829800.012100"), ts_key("1791829800.0121"));
        assert!(ts_key("1791829800.000002") > ts_key("1791829800.000001"));
        assert!(ts_key("999999999.9") < ts_key("1000000000.0"));
        assert_eq!(
            later_ts("1700000000".into(), "1700000000.000100"),
            "1700000000.000100"
        );
    }

    #[test]
    fn a_parent_listed_twice_keeps_its_newest_thread() {
        let out = dedupe(vec![
            message("2.0", Some("3.0")),
            message("1.0", None),
            message("2.0", Some("4.0")),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].latest_reply.as_deref(), Some("4.0"));
    }

    #[test]
    fn a_foreign_cursor_starts_afresh() {
        assert_eq!(
            Cursor::parse(Some(&SyncCursor("page-token-from-drive".into()))),
            Cursor::default()
        );
        let cursor = Cursor {
            newest: Some("1.000100".into()),
            pass: None,
            threads: BTreeMap::new(),
            probed: None,
        };
        let round = cursor.clone().into_sync_cursor();
        assert_eq!(round.0, r#"{"newest":"1.000100"}"#);
        assert_eq!(Cursor::parse(Some(&round)), cursor);
    }

    #[test]
    fn joins_and_topic_changes_are_left_out() {
        let mut m = message("1.0", None);
        assert!(kept(&m));
        m.subtype = Some("channel_join".into());
        assert!(!kept(&m));
        m.subtype = Some("bot_message".into());
        assert!(kept(&m));
    }

    #[test]
    fn a_message_time_is_its_second() {
        assert_eq!(
            ts_time("1791829800.012100").to_string(),
            "2026-10-12T18:30:00Z"
        );
    }
}

/// The thread checks against a mock Slack: a deleted reply, and the round-robin over older
/// threads (T-E4.2, gap G5).
#[cfg(test)]
mod threads {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use wiremock::{
        Mock, MockServer, Request, ResponseTemplate,
        matchers::{method, path},
    };

    use super::*;
    use crate::slack::SlackConfig;

    const DAY: i64 = 86_400;

    fn now() -> Timestamp {
        "2026-10-15T12:00:00Z".parse().expect("a timestamp")
    }

    fn days_ago(d: i64) -> String {
        format!("{}.000100", now().unix_seconds() - d * DAY)
    }

    fn source(server: &MockServer) -> SlackSource {
        let mut config = SlackConfig::era(server.uri());
        config.timeout = Duration::from_secs(2);
        SlackSource::new(SlackClient::new(reqwest::Client::new(), config))
    }

    fn request(cursor: Option<SyncCursor>) -> FarmPageRequest {
        FarmPageRequest {
            account: ConnectorAccount::Slack {
                team_id: "T_MAPLE".into(),
                team_name: "Maple Row Farm".into(),
                url: "https://maplerow.slack.com/".into(),
            },
            source: ConnectorSource {
                id: "C_FIELD".into(),
                name: "field-reports".into(),
            },
            cursor,
            oldest: now().minus(FARM_RECORD_KEEP),
            reread_from: None,
            probe_limit: 2,
        }
    }

    fn parent(ts: &str, replies: u32, latest: &str) -> Value {
        json!({ "ts": ts, "thread_ts": ts, "user": "U1", "text": "parent",
                "reply_count": replies, "latest_reply": latest })
    }

    fn thread(latest: &str, reply_count: u32) -> Thread {
        Thread {
            latest_reply: latest.into(),
            reply_count,
            text: short_hash("parent"),
        }
    }

    async fn users(server: &MockServer) {
        Mock::given(path("/api/users.info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "ok": true, "user": { "id": "U1", "name": "maria", "profile": {} } }),
            ))
            .mount(server)
            .await;
    }

    /// `(ts, limit)` of every `conversations.replies` call.
    type Calls = Arc<Mutex<Vec<(String, String)>>>;

    /// Answers `conversations.replies` from `parents` (by `ts`): the parent alone for
    /// `limit=1`, else the parent and one reply.
    async fn replies(server: &MockServer, parents: Vec<(String, Value)>) -> Calls {
        let calls: Calls = Arc::default();
        let seen = calls.clone();
        Mock::given(method("GET"))
            .and(path("/api/conversations.replies"))
            .respond_with(move |r: &Request| {
                let q = |k: &str| {
                    r.url
                        .query_pairs()
                        .find(|(key, _)| key == k)
                        .map(|(_, v)| v.into_owned())
                        .unwrap_or_default()
                };
                let (ts, limit) = (q("ts"), q("limit"));
                seen.lock().unwrap().push((ts.clone(), limit.clone()));
                let parent = parents
                    .iter()
                    .find(|(t, _)| *t == ts)
                    .map(|(_, p)| p.clone());
                let Some(parent) = parent else {
                    return ResponseTemplate::new(200)
                        .set_body_json(json!({ "ok": false, "error": "thread_not_found" }));
                };
                let mut messages = vec![parent];
                if limit != "1" {
                    messages.push(json!({ "ts": format!("{ts}1"), "thread_ts": ts,
                                          "user": "U1", "text": "reply" }));
                }
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "ok": true, "messages": messages }))
            })
            .mount(server)
            .await;
        calls
    }

    async fn history(server: &MockServer, messages: Vec<Value>) {
        Mock::given(path("/api/conversations.history"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "ok": true, "messages": messages })),
            )
            .mount(server)
            .await;
    }

    fn cursor_of(threads: Vec<(&str, Thread)>) -> SyncCursor {
        Cursor {
            threads: threads
                .into_iter()
                .map(|(ts, t)| (ts.to_owned(), t))
                .collect(),
            ..Cursor::default()
        }
        .into_sync_cursor()
    }

    #[tokio::test]
    async fn a_deleted_reply_changes_the_count_and_reads_the_thread_again() {
        let server = MockServer::start().await;
        users(&server).await;
        let (ts, latest) = (days_ago(1), days_ago(0));
        history(&server, vec![parent(&ts, 1, &latest)]).await;
        let calls = replies(&server, vec![(ts.clone(), parent(&ts, 1, &latest))]).await;
        let source = source(&server);

        // Same latest reply and same count: kept, no call.
        let same = cursor_of(vec![(&ts, thread(&latest, 1))]);
        let page = source.page(&token(), &request(Some(same))).await.unwrap();
        assert_eq!(page.items.len(), 0);
        assert_eq!(page.kept, [format!("C_FIELD/{ts}")]);
        assert!(calls.lock().unwrap().is_empty());

        // The count was 2: a reply was deleted, so the thread is read again.
        let was_two = cursor_of(vec![(&ts, thread(&latest, 2))]);
        let page = source
            .page(&token(), &request(Some(was_two)))
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].text.contains("reply"));
        assert_eq!(page.kept.len(), 0);
        let stored = Cursor::parse(Some(&page.cursor));
        assert_eq!(stored.threads[&ts].reply_count, 1);
    }

    fn token() -> SecretString {
        SecretString::from("xoxb-test")
    }

    #[tokio::test]
    async fn older_threads_are_checked_round_robin_up_to_the_limit() {
        let server = MockServer::start().await;
        users(&server).await;
        history(&server, Vec::new()).await;
        // Three checkable threads (parent over 7 days old, reply within 30), one whose last
        // reply is too old, and one whose parent is inside the re-read window.
        let (p1, p2, p3) = (days_ago(60), days_ago(50), days_ago(40));
        let stale = days_ago(90);
        let young = days_ago(3);
        let (l_old, l_new) = (days_ago(20), days_ago(2));
        let mut answers = vec![
            (p1.clone(), parent(&p1, 2, &l_new)), // a reply arrived
            (p2.clone(), parent(&p2, 1, &l_old)),
            (p3.clone(), parent(&p3, 1, &l_old)),
            (stale.clone(), parent(&stale, 5, &l_new)),
            (young.clone(), parent(&young, 5, &l_new)),
        ];
        answers.reverse();
        let calls = replies(&server, answers).await;
        let source = source(&server);
        let start = cursor_of(vec![
            (&p1, thread(&l_old, 1)),
            (&p2, thread(&l_old, 1)),
            (&p3, thread(&l_old, 1)),
            (&stale, thread(&days_ago(45), 1)),
            (&young, thread(&l_old, 1)),
        ]);

        // Pass 1 checks P1 and P2: P1 changed, so it is read in full and sent.
        let page = source.page(&token(), &request(Some(start))).await.unwrap();
        let probes = |calls: &Calls| -> Vec<String> {
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, limit)| limit == "1")
                .map(|(ts, _)| ts.clone())
                .collect()
        };
        assert_eq!(probes(&calls), [p1.clone(), p2.clone()]);
        assert_eq!(page.probes, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].vendor_id, format!("C_FIELD/{p1}"));
        let cursor = Cursor::parse(Some(&page.cursor));
        assert_eq!(cursor.threads[&p1].reply_count, 2);
        assert_eq!(cursor.probed.as_deref(), Some(p2.as_str()));

        // Pass 2 continues with P3, then wraps to P1, which is unchanged now.
        calls.lock().unwrap().clear();
        let page = source
            .page(&token(), &request(Some(page.cursor)))
            .await
            .unwrap();
        assert_eq!(probes(&calls), [p3.clone(), p1.clone()]);
        assert_eq!(page.items.len(), 0);
        assert_eq!(
            Cursor::parse(Some(&page.cursor)).probed.as_deref(),
            Some(p1.as_str())
        );

        // The sync's limit: one check, then none.
        calls.lock().unwrap().clear();
        let mut one = request(Some(page.cursor));
        one.probe_limit = 1;
        let page = source.page(&token(), &one).await.unwrap();
        assert_eq!(probes(&calls), [p2.as_str()]);
        assert_eq!(page.probes, 1);
        let mut none = request(Some(page.cursor));
        none.probe_limit = 0;
        let page = source.page(&token(), &none).await.unwrap();
        assert_eq!(probes(&calls), [p2.as_str()], "no call");
        assert_eq!(page.probes, 0);
        assert_eq!(
            Cursor::parse(Some(&page.cursor)).probed.as_deref(),
            Some(p2.as_str())
        );
    }

    #[tokio::test]
    async fn an_older_thread_slack_no_longer_has_is_forgotten() {
        let server = MockServer::start().await;
        users(&server).await;
        history(&server, Vec::new()).await;
        let calls = replies(&server, Vec::new()).await;
        let p = days_ago(60);
        let start = cursor_of(vec![(&p, thread(&days_ago(2), 1))]);
        let page = source(&server)
            .page(&token(), &request(Some(start)))
            .await
            .unwrap();
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(page.items.len(), 0);
        assert!(Cursor::parse(Some(&page.cursor)).threads.is_empty());
    }
}
