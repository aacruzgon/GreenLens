//! The farm connectors' vendor port ([connectors 5], [connectors 6]): what the `connectors`
//! scheduler task and the owner routes ask Slack or Google Drive. `integrations` implements it
//! once per vendor (`SlackClient`, `DriveClient`), each built with a base URL so tests point it
//! at Era's simulators; `api` picks the implementation by the connector's `kind`.
//!
//! The adapter is handed a token already unsealed by `api` and never sees the database. It
//! reads raw items (a message with its thread, a document's text); chunking, embedding,
//! `text_hash` and the 10,000-record limit happen in the sync, outside it. The OAuth exchanges
//! are not part of this port: they run once per connect in `api`'s routes (E3).
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
//! [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync

use std::{fmt, sync::Arc, time::Duration};

use crate::{
    connectors::{ConnectorAccount, ConnectorKind, ConnectorSource, FarmRecordKind},
    ports::FarmSourceError,
    wire::Timestamp,
};

/// A vendor access token (Slack's bot token, Google's access token), unsealed for one call.
/// `Debug` never prints it, so a token can't reach a log through a derived `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct VendorToken(Arc<str>);

impl VendorToken {
    /// Wraps an unsealed token.
    pub fn new(token: &str) -> Self {
        Self(Arc::from(token))
    }

    /// The token, for the `Authorization` header only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VendorToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VendorToken(..)")
    }
}

/// Where a source's sync stands, opaque outside its adapter: Slack's newest `ts` and page
/// cursor, Drive's page token or `changes.getStartPageToken`. Stored per source in
/// `connector.cursor`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyncCursor(pub String);

/// One page to read from one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmPageRequest {
    /// The connected workspace or account (Slack permalinks are built from its URL).
    pub account: ConnectorAccount,
    /// The channel or folder.
    pub source: ConnectorSource,
    /// Where the last page stopped; `None` for a first sync.
    pub cursor: Option<SyncCursor>,
    /// The oldest item kept: 180 days back (`FARM_RECORD_KEEP`). A first sync reads from here,
    /// and nothing older is read.
    pub oldest: Timestamp,
    /// The daily re-read ([`FarmSource::reread_window`]): read everything from this instant
    /// again, not only what is new, so edits and deletions are seen. The sync sets it on every
    /// page of a re-read pass and `None` otherwise. The adapter starts a pass from it only when
    /// its cursor has no page in progress; a vendor without a re-read ignores it.
    pub reread_from: Option<Timestamp>,
    /// How many older items the adapter may check with an extra call each on this page, for
    /// changes no pass can see (Slack: replies to threads whose parent is older than the
    /// re-read window). The sync shares a fixed number among a connector's sources per run
    /// and sends 0 on a source's first pass and when its run is nearly over. A vendor whose
    /// change feed reports everything (Drive) ignores it.
    pub probe_limit: u32,
}

/// One item as the vendor holds it, before chunking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmItem {
    /// Slack `"<channel>/<ts>"` or a Drive file id.
    pub vendor_id: String,
    /// A message or a document.
    pub kind: FarmRecordKind,
    /// `"#field-reports"`, `"Spray log 2026"`.
    pub title: String,
    /// The author's display name, from the per-team `users.info` cache.
    pub author: Option<String>,
    /// The permalink or `webViewLink`.
    pub link: String,
    /// A message with its replies joined, or a document's text (cut at 1 MB). Empty for a file
    /// listed by name only.
    pub text: String,
    /// The message's time, or the document's `modifiedTime`.
    pub at: Timestamp,
}

/// What one page returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmPage {
    /// New or changed items.
    pub items: Vec<FarmItem>,
    /// Vendor ids that are gone (deleted, trashed, moved out of the folder), whose records the
    /// sync deletes.
    pub removed: Vec<String>,
    /// Vendor ids still at the vendor and unchanged since the adapter last returned them, so
    /// not sent again (Slack: a thread whose parent and `latest_reply` are as before, whose
    /// replies aren't re-read). A re-read counts them as seen, so their records stay.
    pub kept: Vec<String>,
    /// Every vendor id the source holds at the vendor, when the adapter listed the whole source
    /// on this page (Drive's first sync, or a resync after its change feed lost its place). The
    /// sync then deletes the source's records whose vendor id isn't here, after storing the
    /// page; `None` on every other page.
    pub listed: Option<Vec<String>>,
    /// The cursor to save after this page.
    pub cursor: SyncCursor,
    /// `true` when another page follows; the sync reads it now or on its next run.
    pub more: bool,
    /// How many of [`FarmPageRequest::probe_limit`]'s checks this page made, failed ones
    /// included.
    pub probes: u32,
}

/// A vendor behind the farm connectors (Slack or Google Drive).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait FarmSource: Send + Sync {
    /// Which vendor this adapter speaks to.
    fn kind(&self) -> ConnectorKind;

    /// The trailing window the sync reads again once a day to catch edits and deletions that
    /// reading only what is new can't see: Slack's 7 days ([connectors 6]). `None` for a vendor
    /// whose change feed reports them (Drive's `changes.list`): the sync then never deletes by
    /// absence.
    ///
    /// [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
    fn reread_window(&self) -> Option<Duration>;

    /// `GET /connectors/{id}/available-sources`: the channels the bot is a member of
    /// (`is_member`), or the folders the account can open.
    async fn available_sources(
        &self,
        token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError>;

    /// One page of one source, from `request.cursor`. Honours `Retry-After`; a `429` that
    /// outlasts the pacer is [`FarmSourceError::RateLimited`], so the sync saves its cursor.
    async fn read_page(
        &self,
        token: &VendorToken,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError>;

    /// Revokes the token on disconnect (`auth.revoke`, Google's `revoke`). A failure is logged
    /// by the caller and the records are deleted anyway.
    async fn revoke(&self, token: &VendorToken) -> Result<(), FarmSourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_never_prints() {
        let token = VendorToken::new("xoxb-123");
        assert_eq!(format!("{token:?}"), "VendorToken(..)");
        assert_eq!(token.expose(), "xoxb-123");
    }
}
