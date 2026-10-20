//! Farm connectors and their synced records ([connectors 4], [connectors 6]): the `connector`
//! and `farm_record` tables. Owner routes take a `Membership<Owner>` from the connector's
//! garden (`GardenResource::Connector`); the `connectors` scheduler task, the cleanup and the
//! deletion runner call the internal methods, which take no proof.
//!
//! [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
//! [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync

use std::{collections::BTreeMap, fmt};

use crate::{
    authz::{Membership, Owner, Viewer},
    connectors::{
        Connector, ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource,
        ConnectorStatus, FarmRecordHit, FarmRecordKind, SyncErrorCode,
    },
    id::{ConnectorId, DeviceHash, FarmRecordId, GardenId, UserId},
    ports::{Embedding, StoreError, SyncCursor},
    wire::Timestamp,
};

/// A value sealed with AES-256-GCM under `CONNECTOR_KEY`, the connector's id as associated
/// data (`connector.secret`, the PKCE verifier in `oauth`). `Debug` never prints the bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed(pub bytes::Bytes);

impl fmt::Debug for Sealed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sealed({} bytes)", self.0.len())
    }
}

/// A pending connect's `oauth` field: kept until the exchange clears it, so a state works once.
#[derive(Clone, PartialEq, Eq)]
pub struct PendingConnect {
    /// The nonce the signed `state` carries.
    pub nonce: String,
    /// `SHA-256(device_verifier)` from the app that started the connect.
    pub device_hash: DeviceHash,
    /// Google's PKCE verifier, sealed; `None` for Slack.
    pub verifier: Option<Sealed>,
    /// 10 minutes after the start.
    pub expires_at: Timestamp,
    /// When a `complete` claimed the state ([`FarmRecordStore::claim_connect`]) to exchange the
    /// code; `None` until then. A claimed state is spent: the callback and every other
    /// `complete` refuse it.
    pub claimed_at: Option<Timestamp>,
}

impl fmt::Debug for PendingConnect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingConnect")
            .field("nonce", &"[redacted]")
            .field("device_hash", &self.device_hash)
            .field("verifier", &self.verifier)
            .field("expires_at", &self.expires_at)
            .field("claimed_at", &self.claimed_at)
            .finish()
    }
}

/// A connector to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConnector {
    /// Its id.
    pub id: ConnectorId,
    /// Slack or Google Drive.
    pub kind: ConnectorKind,
    /// `pending` for an owner's connect; `active` for the seeded demo connectors.
    pub status: ConnectorStatus,
    /// `vendor` for every route; `era` only from start-up seeding.
    pub endpoint: ConnectorEndpoint,
    /// The owner who consented.
    pub created_by: UserId,
    /// Known at once only for a seeded connector.
    pub account: Option<ConnectorAccount>,
    /// The sealed tokens; `None` until the exchange.
    pub secret: Option<Sealed>,
    /// A pending connect's state; `None` for a seeded connector.
    pub oauth: Option<PendingConnect>,
    /// The chosen sources; empty for an owner's connect.
    pub sources: Vec<ConnectorSource>,
    /// Now.
    pub created_at: Timestamp,
}

/// A connector with everything the sync and the routes need beside its wire shape. Never sent:
/// it holds the sealed tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorRow {
    /// The wire shape, with its record count.
    pub connector: Connector,
    /// The owner who consented; the connector goes when they stop being an owner.
    pub created_by: UserId,
    /// The sealed tokens.
    pub secret: Option<Sealed>,
    /// The pending connect's state.
    pub oauth: Option<PendingConnect>,
    /// Each source's sync cursor, by source id.
    pub cursors: BTreeMap<String, SyncCursor>,
    /// When the sync is due.
    pub next_sync_at: Timestamp,
    /// The last sync's failure, a fixed code.
    pub last_error: Option<SyncErrorCode>,
    /// The last accepted **Sync now** (decision 12): one per connector in 5 minutes.
    pub sync_requested_at: Option<Timestamp>,
}

/// What a successful exchange stores: `status: active`, `oauth` cleared, `next_sync_at` now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedConnect {
    /// The nonce the state carried: the write is a compare-and-set on the stored one, so a
    /// state works once.
    pub nonce: String,
    /// The connected workspace or account.
    pub account: ConnectorAccount,
    /// The sealed tokens.
    pub secret: Sealed,
    /// Now: also the first `next_sync_at`.
    pub now: Timestamp,
}

/// What a sync run stores when it stops, finished or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    /// The connector's `next_sync_at` as [`due_connectors`](FarmRecordStore::due_connectors)
    /// returned it. If the stored value is later when the run ends, a **Sync now** or a new set
    /// of sources landed during the run: the store keeps that time, and doesn't store a
    /// stopping failure (`too_large`, `source_missing`) over it.
    pub due_at: Timestamp,
    /// Each source's cursor after the run.
    pub cursors: BTreeMap<String, SyncCursor>,
    /// Set when every source was read to its end.
    pub synced_at: Option<Timestamp>,
    /// 15 minutes after a finished sync or an outage; now when pages are left.
    pub next_sync_at: Timestamp,
    /// The failure that stopped it, if any.
    pub last_error: Option<SyncErrorCode>,
}

/// What [`FarmRecordStore::request_sync`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRequest {
    /// Accepted: the connector is due now.
    Accepted,
    /// Within [`SYNC_NOW_INTERVAL`](crate::rules::limits::SYNC_NOW_INTERVAL) of the last
    /// accepted request.
    TooSoon {
        /// When the next request will be accepted (the last one plus the interval).
        allowed_at: Timestamp,
    },
}

/// A chunk's vector, sent only when its `text_hash` changed.
#[derive(Debug, Clone, PartialEq)]
pub struct FarmVector {
    /// The vector, embedded in document mode with the pinned text embedder.
    pub embedding: Embedding,
    /// The embedding model's name.
    pub model: String,
}

/// One chunk to upsert by `(connector, vendor_id, n)`.
#[derive(Debug, Clone, PartialEq)]
pub struct NewFarmRecord {
    /// `farm_record:[connector, vendor_id, n]`.
    pub id: FarmRecordId,
    /// A message or a document chunk.
    pub kind: FarmRecordKind,
    /// The connector source it came from: the Slack channel id or the Drive folder id.
    /// Dropping the source deletes by it.
    pub source: String,
    /// `"#field-reports"`, `"Spray log 2026"`.
    pub title: String,
    /// The author's display name.
    pub author: Option<String>,
    /// The permalink or `webViewLink`.
    pub link: String,
    /// At most 2,000 characters.
    pub text: String,
    /// The hash of `text`: an unchanged hash keeps the stored vector.
    pub text_hash: String,
    /// `None` when `text_hash` is unchanged, so the stored vector stays; required for a new
    /// record.
    pub vector: Option<FarmVector>,
    /// The message's time, or the document's `modifiedTime`.
    pub at: Timestamp,
}

/// `search_farm_records`' filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FarmRecordFilter {
    /// Messages or documents only.
    pub kind: Option<FarmRecordKind>,
    /// Records at or after this instant (the tool's `since` date at the garden's midnight).
    pub since: Option<Timestamp>,
}

/// A search hit with its similarity to the query.
#[derive(Debug, Clone, PartialEq)]
pub struct FarmRecordMatch {
    /// The text and its citation.
    pub hit: FarmRecordHit,
    /// Cosine similarity.
    pub similarity: f32,
}

/// The `connector` and `farm_record` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait FarmRecordStore: Send + Sync {
    // ---- connectors ----

    /// `POST /gardens/{id}/connectors`: a `pending` connector with its `oauth` state. A garden
    /// holds at most 2 connectors (`Rejected`).
    async fn create_connector(
        &self,
        owner: Membership<Owner>,
        connector: NewConnector,
    ) -> Result<Connector, StoreError>;

    /// Start-up seeding of the demo garden's `era` connectors: idempotent by the fixed id.
    async fn seed_connector(
        &self,
        garden: GardenId,
        connector: NewConnector,
    ) -> Result<Connector, StoreError>;

    /// `GET /gardens/{id}/connectors`: the garden's connectors with their record counts,
    /// oldest first, leaving out those being deleted.
    async fn connectors(&self, member: Membership<Viewer>) -> Result<Vec<Connector>, StoreError>;

    /// One connector with its sealed tokens and sync state; `None` when it doesn't exist.
    async fn connector_row(
        &self,
        connector: ConnectorId,
    ) -> Result<Option<ConnectorRow>, StoreError>;

    /// `POST /connectors/{id}/complete`, before the code exchange: claims the pending state, a
    /// compare-and-set on `status = 'pending'`, the stored nonce, its `expires_at` being after
    /// `now` and no earlier claim, which sets `oauth.claimed_at`. Single use: of two
    /// concurrent `complete`s, one claims and the other gets `Conflict(AlreadyExists)`, so the
    /// code is spent at most once. `Conflict(AlreadyExists)` too when the state was used,
    /// replaced or expired; `NotFound` when the connector is gone.
    async fn claim_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        nonce: String,
        now: Timestamp,
    ) -> Result<(), StoreError>;

    /// Undoes [`claim_connect`](Self::claim_connect) when the exchange failed before the vendor
    /// gave tokens (an outage, a refused code), so a retry of the same `complete` can claim
    /// again. Only the claim of `nonce` on a connector still `pending` is cleared; anything
    /// else is left as it is.
    async fn release_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        nonce: String,
    ) -> Result<(), StoreError>;

    /// `POST /connectors/{id}/complete`, after the exchange: a compare-and-set on
    /// `status = 'pending'` and the stored nonce having been claimed; `Conflict(AlreadyExists)`
    /// when that no longer holds, and `NotFound` when the connector is gone (deleted, or
    /// removed by the pending cleanup while the exchange ran). The caller then revokes the
    /// tokens it was given, which nothing stores.
    async fn complete_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        done: CompletedConnect,
    ) -> Result<Connector, StoreError>;

    /// `PUT /connectors/{id}/sources`: replaces the sources, drops the cursors of sources no
    /// longer chosen and makes the sync due now.
    async fn set_sources(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        sources: Vec<ConnectorSource>,
        now: Timestamp,
    ) -> Result<Connector, StoreError>;

    /// `POST /connectors/{id}/sync` (decision 12): a compare-and-set on `sync_requested_at`,
    /// accepted when it is unset or [`SYNC_NOW_INTERVAL`] old (`next_sync_at` becomes now);
    /// [`SyncRequest::TooSoon`] within it, with the instant the next is allowed, so the route
    /// can answer `429 rate_limited` with `retry_after_s`. Only an `active` connector syncs; any
    /// other status, and a connector stopped until new sources (`too_large`, `source_missing`),
    /// is `Rejected(InvalidState)`.
    ///
    /// [`SYNC_NOW_INTERVAL`]: crate::rules::limits::SYNC_NOW_INTERVAL
    async fn request_sync(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        now: Timestamp,
    ) -> Result<SyncRequest, StoreError>;

    /// The `connectors` task: connectors whose `next_sync_at` has passed, `active` only
    /// (never `pending`, `reauth_needed` or `deleting`), on the `connector_due` index, earliest
    /// first. A connector stopped with `too_large` or `source_missing` waits until the owner
    /// changes its sources ([`set_sources`](Self::set_sources) clears the failure), and
    /// [`request_sync`](Self::request_sync) refuses it with `invalid_state`.
    async fn due_connectors(
        &self,
        now: Timestamp,
        limit: u32,
    ) -> Result<Vec<ConnectorRow>, StoreError>;

    /// Stores a sync run's cursors, times and failure.
    async fn finish_sync(
        &self,
        connector: ConnectorId,
        outcome: SyncOutcome,
    ) -> Result<(), StoreError>;

    /// Sets `status` and `last_error` (a refused token: `reauth_needed` with `auth_refused`).
    async fn set_status(
        &self,
        connector: ConnectorId,
        status: ConnectorStatus,
        last_error: Option<SyncErrorCode>,
    ) -> Result<(), StoreError>;

    /// `DELETE /connectors/{id}`: `status: deleting`, so the sync and the list skip it.
    /// Returns the row, whose tokens the caller revokes before deleting the records.
    async fn start_connector_deletion(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
    ) -> Result<ConnectorRow, StoreError>;

    /// One batch of a connector's deletion: its farm records, 500 at a time, then the connector
    /// itself. The caller repeats it until 0.
    async fn delete_connector_batch(&self, connector: ConnectorId) -> Result<u32, StoreError>;

    /// The connectors a member created in a garden: deleted when that member stops being an
    /// owner (demoted, removed, leaving or deleting the account). The store marks those
    /// `deleting` itself, in the transaction that changes the membership
    /// (`GardenRepo::change_role`, `GardenRepo::remove_member`, `DeletionRepo::transfer_ownership`;
    /// a garden's in `DeletionRepo::start_garden_deletion`).
    async fn connectors_created_by(
        &self,
        garden: GardenId,
        user: UserId,
    ) -> Result<Vec<ConnectorId>, StoreError>;

    /// How many other live connectors, in any garden, hold a token of this account: `vendor`
    /// connectors of `kind`, `active` or `reauth_needed`, not `except`, whose account is
    /// `account` (Slack's team id, or the Google account's email). A deletion revokes the token
    /// only when there are none ([connectors 5]): Slack gives one bot token per app and
    /// workspace, and revoking Google's refresh token ends the whole grant of that account to
    /// this OAuth client, so either would stop every connector of the account. `era`
    /// connectors hold the simulator's tokens and never count.
    ///
    /// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
    async fn shared_account_connectors(
        &self,
        kind: ConnectorKind,
        account: String,
        except: ConnectorId,
    ) -> Result<u32, StoreError>;

    /// Replaces a connector's sealed tokens with `new` (a refreshed Google access token) only
    /// when the stored value is still `read`, the one the caller unsealed, and the connector is
    /// `active` or `reauth_needed`: a compare-and-set, so of two refreshes at once one is
    /// stored and the other keeps its token for its own call. `true` when it was stored.
    /// Used by the source picker and the sync (E4).
    async fn replace_secret(
        &self,
        connector: ConnectorId,
        read: Sealed,
        new: Sealed,
    ) -> Result<bool, StoreError>;

    /// Connectors marked `deleting` whose deletion hasn't finished, with their sealed tokens,
    /// oldest id first: with `Some(garden)`, that garden's (a cascade or a demotion finishing
    /// them); with `None`, those of every garden not itself being deleted (`api`'s
    /// `deletions.resume()` after a restart; a garden being deleted finishes its own connectors
    /// in its cascade). The marker is what makes a deletion resumable: the status is set in the
    /// transaction that removed the owner, and the row keeps its token until `api` revoked it
    /// and deleted the records (EON E3.6).
    async fn deleting_connectors(
        &self,
        garden: Option<GardenId>,
    ) -> Result<Vec<ConnectorRow>, StoreError>;

    /// The cleanup: deletes `pending` connectors created before `before` (10 minutes ago),
    /// except those a `complete` claimed at or after `claimed_before` (`CONNECT_CLAIM_HOLD`
    /// ago), whose exchange may still be running; up to 500 a call.
    async fn delete_expired_pending(
        &self,
        before: Timestamp,
        claimed_before: Timestamp,
    ) -> Result<u32, StoreError>;

    // ---- farm records ----

    /// The stored `text_hash` of every chunk of these items, so the sync embeds only changed
    /// text.
    async fn record_hashes(
        &self,
        connector: ConnectorId,
        vendor_ids: Vec<String>,
    ) -> Result<Vec<(FarmRecordId, String)>, StoreError>;

    /// Upserts chunks by `(connector, vendor_id, n)`, keeping the stored vector when
    /// `vector` is `None`. Returns how many were written. `NotFound` when the connector is
    /// missing or being deleted, so no record outlives its connector's deletion.
    async fn upsert_records(
        &self,
        connector: ConnectorId,
        records: Vec<NewFarmRecord>,
    ) -> Result<u32, StoreError>;

    /// Deletes every chunk of these items (gone at the vendor). Returns how many went.
    async fn delete_records(
        &self,
        connector: ConnectorId,
        vendor_ids: Vec<String>,
    ) -> Result<u32, StoreError>;

    /// A document re-chunked into fewer chunks: deletes its chunks numbered `from` and up,
    /// leaving the others (and their vectors) for [`upsert_records`](Self::upsert_records).
    async fn delete_chunks_from(
        &self,
        connector: ConnectorId,
        vendor_id: String,
        from: u32,
    ) -> Result<u32, StoreError>;

    /// The vendor ids a source holds from `since` on (by `at`), for the daily 7-day re-read to
    /// find what is gone at the vendor ([connectors 6]). The source is the id stored on each
    /// record (`NewFarmRecord::source`).
    async fn source_vendor_ids_since(
        &self,
        connector: ConnectorId,
        source: String,
        since: Timestamp,
    ) -> Result<Vec<String>, StoreError>;

    /// One batch of 500 of a source's records, after the owner dropped it: those of the
    /// connector whose stored `source` is this channel or folder id. Another source's records,
    /// a new Drive folder's among them, stay.
    async fn delete_source_records(
        &self,
        connector: ConnectorId,
        source: String,
    ) -> Result<u32, StoreError>;

    /// A garden's record count, for the 10,000 limit.
    async fn garden_record_count(&self, garden: GardenId) -> Result<u32, StoreError>;

    /// `search_farm_records`: the `k` closest records of the proof's garden, searched exactly
    /// (no HNSW index, gap G7), each with its citation.
    async fn search_records(
        &self,
        member: Membership<Viewer>,
        query: &Embedding,
        filter: FarmRecordFilter,
        k: usize,
    ) -> Result<Vec<FarmRecordMatch>, StoreError>;

    /// The cleanup: one batch of 500 Slack messages older than the cutoff (180 days), on the
    /// `farm_record_kind_at` index. Documents are not deleted by age: Drive's sync removes a
    /// file when `changes.list` says it is gone.
    async fn delete_records_before(&self, cutoff: Timestamp) -> Result<u32, StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_bytes_never_print() {
        let sealed = Sealed(bytes::Bytes::from_static(b"ciphertext"));
        assert_eq!(format!("{sealed:?}"), "Sealed(10 bytes)");
    }

    #[test]
    fn the_pending_connects_nonce_never_prints() {
        let pending = PendingConnect {
            nonce: "the-secret-nonce".into(),
            device_hash: "h".repeat(43).parse().unwrap(),
            verifier: None,
            expires_at: Timestamp::from(chrono::DateTime::UNIX_EPOCH),
            claimed_at: None,
        };
        assert!(!format!("{pending:?}").contains("the-secret-nonce"));
    }
}
