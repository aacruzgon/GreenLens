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
//! - `threads`: per thread parent, its `latest_reply` and a short hash of its text when its
//!   replies were last read. A parent seen again with both unchanged isn't sent again (its id is
//!   [`FarmPage::kept`]) and costs no `conversations.replies` call.
//!
//! The daily re-read ([`FarmPageRequest::reread_from`]) starts a pass from 7 days back. Slack's
//! history never reports a deletion, so the sync deletes what that pass didn't see. A new reply
//! to a thread shows in its parent's `latest_reply`, which a pass reads when the parent is in
//! its window: a later pass when the parent is new, the daily re-read for the last 7 days.
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
    wire::Timestamp,
};

use super::{SlackClient, SlackMessage, permalink};
use crate::farm::FarmClientError;

/// The trailing window read again once a day ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const REREAD_WINDOW: Duration = Duration::from_hours(7 * 24);

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

        let more = history.next_cursor.is_some();
        match history.next_cursor {
            Some(next) => {
                cursor.pass = Some(Pass {
                    oldest,
                    page: next,
                    top: page_top,
                });
            }
            None => {
                cursor.newest = [cursor.newest.take(), page_top]
                    .into_iter()
                    .flatten()
                    .max_by_key(|t| ts_key(t));
            }
        }
        // Threads whose parent left the 180 days are forgotten with their records.
        let floor_key = ts_key(&floor);
        cursor.threads.retain(|ts, _| ts_key(ts) >= floor_key);
        Ok(FarmPage {
            items,
            removed: Vec::new(),
            kept: kept_ids,
            cursor: cursor.into_sync_cursor(),
            more,
        })
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
