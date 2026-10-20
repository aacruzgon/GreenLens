//! Farm connectors and their records ([connectors 4], [connectors 6]): the `connector` and
//! `farm_record` tables (roadmap task E1.6, docs/EON).
//!
//! - **Sync now** is a compare-and-set on `sync_requested_at` (EON decision 12): one
//!   conditional `UPDATE` accepts a request only when the field is unset or 5 minutes old, so
//!   the limit holds per connector and across restarts.
//! - **Upserts** keep a chunk's stored vector when the sync sends none, and refuse to do so
//!   when the stored `text_hash` differs, so a vector never belongs to other text.
//! - **The search** is exact cosine within one garden, like `memory`'s without its sharing
//!   filter (EON gap G7): no HNSW index.
//! - **Deletes** go 500 at a time ([database 4], deletion), records before their connector.
//!
//! [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
//! [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
//! [database 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#4-write-patterns

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use domain::{
    authz::{Membership, Owner, Viewer},
    connectors::{
        Connector, ConnectorKind, ConnectorSource, ConnectorStatus, FarmCitation, FarmRecordHit,
        SyncErrorCode,
    },
    error::{DomainError, InvalidStateDetails},
    id::{ConnectorId, FarmRecordId, GardenId, UserId},
    ports::{
        CompletedConnect, Conflict, ConnectorRow, Embedding, FarmRecordFilter, FarmRecordMatch,
        FarmRecordStore, NewConnector, NewFarmRecord, PendingConnect, Sealed, StoreError,
        SyncCursor, SyncOutcome, SyncRequest,
    },
    rules::limits::{DELETE_BATCH, MAX_CONNECTORS_PER_GARDEN, SYNC_NOW_INTERVAL},
    wire::Timestamp,
};
use surrealdb::types::{Bytes, RecordId, SurrealValue, Value};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown},
    ids::{farm_record_id_of, farm_record_rid, from_rid, rid},
    read::{decode, take_all, take_one},
    rows::{
        ConnectorEndpointRow, ConnectorKindRow, ConnectorStatusRow, FarmRecordKindRow,
        SyncErrorCodeRow, dt, to_json, ts,
    },
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// The projection of a `connector` into `Connector`, with its record count (index
    /// `farm_record_connector`).
    macro_rules! connector_fields {
        () => {
            "id, garden AS garden_id, kind, status, endpoint, account, sources, synced_at, created_at,
  count(SELECT VALUE id FROM farm_record WHERE connector = $parent.id) AS record_count"
        };
    }

    /// [`connector_fields!`] and the sync state a `ConnectorRow` adds.
    macro_rules! row_fields {
        () => {
            concat!(
                connector_fields!(),
                ", created_by, secret, oauth, cursor, next_sync_at, last_error, sync_requested_at"
            )
        };
    }

    /// Keeps the entries of the cursor object `$from` whose source id is in `$ids`, and the
    /// sync's own state (`#sync`, [`domain::ports::SYNC_STATE_KEY`]).
    macro_rules! kept_cursor {
        ($from:literal, $ids:literal) => {
            concat!(
                "object::from_entries(array::filter(object::entries(",
                $from,
                " ?? {}), |$e| $e[0] IN ",
                $ids,
                " OR $e[0] = '#sync'))"
            )
        };
    }

    /// `POST /gardens/{id}/connectors`: a `pending` connector, at most
    /// `MAX_CONNECTORS_PER_GARDEN` per garden besides those being deleted. The first
    /// statement bumps the garden's `connector_rev`, so two creates racing on one garden write
    /// the same key and one conflicts; its retry then counts the other's connector. (A count
    /// alone doesn't conflict under snapshot isolation: the creates write different keys.)
    pub const CREATE_CONNECTOR: &str = concat!(
        "BEGIN;
UPDATE $garden SET connector_rev += 1;
IF count(SELECT VALUE id FROM connector WHERE garden = $garden AND status != 'deleting') >= $max { THROW 'connector_limit' };
CREATE $connector CONTENT $row;
SELECT ", connector_fields!(), " FROM ONLY $connector;
COMMIT;"
    );
    /// Start-up seeding: creates the connector when its fixed id is free; otherwise repairs
    /// it, so every start may run it. A connector that exists in this garden and isn't being
    /// deleted gets the freshly sealed token (a new `CONNECTOR_KEY` makes the old one useless),
    /// `status` `active` and no `last_error`. One in another garden, or being deleted, or in a
    /// garden being deleted, is left as it is; the caller sees its garden in the result.
    pub const SEED_CONNECTOR: &str = concat!(
        "BEGIN;
LET $c = (SELECT garden, status FROM ONLY $connector);
LET $dying = (SELECT VALUE deleting_at FROM ONLY $garden);
IF $c = NONE { CREATE $connector CONTENT $row; }
ELSE IF $c.garden = $garden AND $c.status != 'deleting' AND $dying = NONE {
  UPDATE $connector SET status = $row.status, last_error = NONE,
    secret = IF $row.secret = NONE { secret } ELSE { $row.secret };
};
SELECT ",
        connector_fields!(),
        " FROM ONLY $connector;
COMMIT;"
    );
    /// `GET /gardens/{id}/connectors`: oldest first (index `connector_garden`), leaving out
    /// those being deleted.
    pub const CONNECTORS: &str = concat!(
        "SELECT ",
        connector_fields!(),
        " FROM connector WHERE garden = $garden AND status != 'deleting' ORDER BY created_at, id;"
    );
    /// One connector with its sync state.
    pub const CONNECTOR_ROW: &str = concat!("SELECT ", row_fields!(), " FROM ONLY $connector;");
    /// `POST /connectors/{id}/complete`, before the exchange: claims the pending state, a
    /// compare-and-set on `status = 'pending'`, the stored nonce, its expiry and no earlier
    /// claim. Two claims at once write the same record, so one conflicts, is retried, and then
    /// finds the claim and throws.
    pub const CLAIM_CONNECT: &str = "BEGIN;
LET $c = (SELECT status, oauth FROM ONLY $connector WHERE garden = $garden);
IF $c = NONE { THROW 'not_found' };
IF $c.status != 'pending' OR $c.oauth.nonce != $nonce OR $c.oauth.expires_at <= $now OR $c.oauth.claimed_at != NONE { THROW 'connect_lost' };
UPDATE $connector SET oauth.claimed_at = $now;
COMMIT;";
    /// Clears this nonce's claim after a failed exchange; a no-op for anything else.
    pub const RELEASE_CONNECT: &str = "UPDATE $connector SET oauth.claimed_at = NONE WHERE garden = $garden AND status = 'pending' AND oauth.nonce = $nonce;";
    /// `POST /connectors/{id}/complete`, after the exchange: a compare-and-set on
    /// `status = 'pending'` and the claimed nonce, so a state works once. The expiry was checked
    /// by the claim.
    pub const COMPLETE_CONNECT: &str = concat!(
        "BEGIN;
LET $c = (SELECT status, oauth FROM ONLY $connector WHERE garden = $garden);
IF $c = NONE { THROW 'not_found' };
IF $c.status != 'pending' OR $c.oauth.nonce != $nonce OR $c.oauth.claimed_at = NONE { THROW 'connect_lost' };
UPDATE $connector SET status = 'active', oauth = NONE, account = $account, secret = $secret,
  next_sync_at = $now, last_error = NONE;
SELECT ", connector_fields!(), " FROM ONLY $connector;
COMMIT;"
    );
    /// `PUT /connectors/{id}/sources`: replaces the sources, keeps only the chosen sources'
    /// cursors, clears a failure that waited for new sources and makes the sync due now.
    pub const SET_SOURCES: &str = concat!(
        "BEGIN;
LET $c = (SELECT status, cursor FROM ONLY $connector WHERE garden = $garden);
IF $c = NONE OR $c.status = 'deleting' { THROW 'not_found' };
IF $c.status = 'pending' { THROW 'invalid_state' };
UPDATE $connector SET sources = $sources, cursor = ",
        kept_cursor!("$c.cursor", "$sources.id"),
        ",
  next_sync_at = $now,
  last_error = IF last_error IN ['too_large', 'source_missing'] { NONE } ELSE { last_error };
SELECT ",
        connector_fields!(),
        " FROM ONLY $connector;
COMMIT;"
    );
    /// `POST /connectors/{id}/sync` (EON decision 12): a connector stopped until new sources
    /// (`too_large`, `source_missing`) is `invalid_state`, because the due query skips it. The
    /// conditional `UPDATE` is the compare-and-set. It returns the connector's id when it was accepted and nothing within
    /// 5 minutes of the last accepted request; the last statement returns that request's time.
    pub const REQUEST_SYNC: &str = "BEGIN;
LET $c = (SELECT status, sync_requested_at, last_error FROM ONLY $connector WHERE garden = $garden);
IF $c = NONE OR $c.status = 'deleting' { THROW 'not_found' };
IF $c.status != 'active' OR $c.last_error IN ['too_large', 'source_missing'] { THROW 'invalid_state' };
UPDATE $connector SET sync_requested_at = $now, next_sync_at = $now
  WHERE sync_requested_at = NONE OR sync_requested_at <= $cutoff RETURN VALUE id;
RETURN $c.sync_requested_at;
COMMIT;";
    /// The `connectors` task (index `connector_due`): `active` connectors whose
    /// `next_sync_at` has passed, earliest first, except those stopped until the owner
    /// changes the sources (`too_large`, `source_missing`, [connectors 6]).
    pub const DUE_CONNECTORS: &str = concat!(
        "SELECT ",
        row_fields!(),
        " FROM connector
WHERE status = 'active' AND next_sync_at <= $now AND !(last_error IN ['too_large', 'source_missing'])
ORDER BY next_sync_at, id LIMIT $limit;"
    );
    /// A sync run's end: the cursors of sources still chosen, the times and the failure. A
    /// connector being deleted is left alone, and a missing one is never created. When the
    /// connector's `next_sync_at` moved past `$due_at` (the value the run started from) while
    /// the run was going, a **Sync now** or a new set of sources landed: that time stays, a
    /// stopping failure of the old sources isn't stored over the owner's change, and
    /// `synced_at` isn't set, since the run may not have read the sources now chosen.
    pub const FINISH_SYNC: &str = concat!(
        "BEGIN;
LET $moved = (SELECT VALUE next_sync_at FROM ONLY $connector) > $due_at;
UPDATE $connector SET cursor = ",
        kept_cursor!("$cursor", "sources.id"),
        ",
  synced_at = IF $moved { synced_at } ELSE { $synced_at ?? synced_at },
  next_sync_at = IF $moved { next_sync_at } ELSE { $next },
  last_error = IF $moved AND $last_error IN ['too_large', 'source_missing'] { last_error } ELSE { $last_error }
WHERE status != 'deleting';
COMMIT;"
    );
    /// The status and failure; a connector being deleted stays so.
    pub const SET_STATUS: &str = "UPDATE $connector SET status = $status, last_error = $last_error WHERE status != 'deleting';";
    /// `DELETE /connectors/{id}`: `status: deleting`, so the sync and the list skip it.
    pub const START_DELETION: &str = concat!(
        "BEGIN;
IF (SELECT VALUE id FROM ONLY $connector WHERE garden = $garden) = NONE { THROW 'not_found' };
UPDATE $connector SET status = 'deleting';
SELECT ",
        row_fields!(),
        " FROM ONLY $connector;
COMMIT;"
    );
    /// One batch of a deleting connector's records ([database 4], deletion: 500 a batch).
    pub const DELETE_CONNECTOR_RECORDS: &str = "BEGIN;
LET $st = (SELECT VALUE status FROM ONLY $connector);
IF $st != NONE AND $st != 'deleting' { THROW 'invalid_state' };
DELETE (SELECT VALUE id FROM farm_record WHERE connector = $connector LIMIT 500) RETURN VALUE $before.id;
COMMIT;";
    /// The connector itself, once its records are gone.
    pub const DELETE_CONNECTOR: &str =
        "DELETE $connector WHERE status = 'deleting' RETURN VALUE $before.id;";
    /// The connectors a member created in a garden.
    pub const CREATED_BY: &str =
        "SELECT VALUE id FROM connector WHERE garden = $garden AND created_by = $user ORDER BY id;";
    /// Other live `vendor` connectors holding a token of this Slack workspace. No index: the
    /// table holds at most two connectors a garden.
    pub const SLACK_ACCOUNT_CONNECTORS: &str = "RETURN count(SELECT VALUE id FROM connector WHERE kind = 'slack' AND endpoint = 'vendor' AND status IN ['active', 'reauth_needed'] AND account.team_id = $account AND id != $except);";
    /// Other live `vendor` connectors holding a grant of this Google account.
    pub const GOOGLE_ACCOUNT_CONNECTORS: &str = "RETURN count(SELECT VALUE id FROM connector WHERE kind = 'google_drive' AND endpoint = 'vendor' AND status IN ['active', 'reauth_needed'] AND account.email = $account AND id != $except);";
    /// A refreshed token: a compare-and-set on the sealed value the caller read. One record by
    /// its id, so the statement touches at most one row.
    pub const REPLACE_SECRET: &str = "UPDATE $connector SET secret = $new WHERE secret = $read AND status IN ['active', 'reauth_needed'] RETURN VALUE id;";
    /// A garden's connectors marked `deleting` (index `connector_garden`), for a cascade or a
    /// demotion to revoke and delete.
    pub const DELETING_OF_GARDEN: &str = concat!(
        "SELECT ",
        row_fields!(),
        " FROM connector WHERE garden = $garden AND status = 'deleting' ORDER BY id;"
    );
    /// Every connector marked `deleting` outside a garden being deleted, to resume after a
    /// restart (index `connector_due`, whose first field is `status`). A garden being deleted
    /// finishes its own connectors in its resumed cascade.
    pub const DELETING_TO_RESUME: &str = concat!(
        "SELECT ",
        row_fields!(),
        " FROM connector WHERE status = 'deleting' AND garden.deleting_at = NONE ORDER BY id;"
    );
    /// The connectors marked `deleting` that a user created, in any garden (a garden being
    /// deleted included), for the account deletion that must clear `created_by` references.
    pub const DELETING_OF_USER: &str = concat!(
        "SELECT ",
        row_fields!(),
        " FROM connector WHERE status = 'deleting' AND created_by = $user ORDER BY id;"
    );
    /// The cleanup: one batch of `pending` connectors started before the cutoff, leaving those
    /// claimed since `$claimed_cutoff`, whose exchange may still be running. A pending
    /// connector holds no records.
    pub const DELETE_EXPIRED_PENDING: &str = "DELETE (SELECT VALUE id FROM connector WHERE status = 'pending' AND created_at < $cutoff AND (oauth.claimed_at = NONE OR oauth.claimed_at < $claimed_cutoff) LIMIT 500) RETURN VALUE $before.id;";

    /// The stored hash of every chunk of these items (index `farm_record_connector`).
    pub const RECORD_HASHES: &str = "SELECT id, text_hash FROM farm_record WHERE connector = $connector AND vendor_id IN $vendor_ids;";
    /// Chunks by `(connector, vendor_id, n)`. A chunk sent without a vector keeps the stored
    /// one, and only when its `text_hash` is unchanged (`missing_vector` otherwise); the
    /// connector must be syncing (`active` or `reauth_needed`), so no record is written for a
    /// connector being deleted. A record whose `source` is no longer among the connector's
    /// `sources` is skipped in the same transaction: `PUT .../sources` can drop a source while
    /// a sync that read it is still running, and its records must not come back. The last
    /// statement returns how many were written. The check reads `sources` from the
    /// transaction's snapshot: a `PUT .../sources` that commits while it runs isn't seen, so
    /// that page's records of the dropped source can still land (a known narrow window,
    /// [connectors 6]).
    ///
    /// [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
    pub const UPSERT_RECORDS: &str = "BEGIN;
LET $c = (SELECT garden, status, sources FROM ONLY $connector);
IF $c = NONE OR !($c.status IN ['active', 'reauth_needed']) { THROW 'not_found' };
FOR $r IN $records {
  IF $r.source IN $c.sources.id {
    IF $r.embedding = NONE {
      IF (SELECT VALUE text_hash FROM ONLY $r.id) != $r.text_hash { THROW 'missing_vector' };
      UPDATE $r.id SET kind = $r.kind, source = $r.source, title = $r.title, author = $r.author, link = $r.link,
        text = $r.text, at = $r.at;
    } ELSE {
      UPSERT $r.id CONTENT { garden: $c.garden, connector: $connector, kind: $r.kind,
        vendor_id: $r.vendor_id, source: $r.source, title: $r.title, author: $r.author, link: $r.link, text: $r.text,
        text_hash: $r.text_hash, embedding: $r.embedding, embedding_model: $r.embedding_model,
        at: $r.at };
    };
  };
};
RETURN count($records.filter(|$r| $r.source IN $c.sources.id));
COMMIT;";
    /// Every chunk of these items.
    pub const DELETE_RECORDS: &str = "DELETE farm_record WHERE connector = $connector AND vendor_id IN $vendor_ids RETURN VALUE $before.id;";
    /// A document's chunks numbered `$from` and up: a key range on
    /// `farm_record:[connector, vendor_id, n]`.
    pub const DELETE_CHUNKS_FROM: &str = "DELETE farm_record:[$connector, $vendor_id, $from]..=[$connector, $vendor_id, ..] RETURN VALUE $before.id;";
    /// The vendor ids a source holds from `$since` on, each once (index
    /// `farm_record_source`).
    pub const SOURCE_VENDOR_IDS_SINCE: &str = "RETURN array::distinct(SELECT VALUE vendor_id FROM farm_record WHERE connector = $connector AND source = $source AND at >= $since);";
    /// One batch of a dropped source's records, by the source stored on each record (index
    /// `farm_record_source`), so another source's records, a new folder's among them, stay.
    pub const DELETE_SOURCE_RECORDS: &str = "DELETE (SELECT VALUE id FROM farm_record WHERE connector = $connector AND source = $source LIMIT 500) RETURN VALUE $before.id;";
    /// A garden's records (index `farm_record_garden`), for the 10,000 limit.
    pub const GARDEN_RECORD_COUNT: &str =
        "RETURN count(SELECT VALUE id FROM farm_record WHERE garden = $garden);";
    /// `search_farm_records` (EON gap G7): `memory`'s exact cosine search without its sharing
    /// filter, within one garden, leaving out a connector being deleted.
    pub const SEARCH: &str = "SELECT id, connector.kind AS system, title, author, link, text, at,
  vector::similarity::cosine(embedding, $query) AS similarity FROM farm_record
WHERE garden = $garden AND ($kind = NONE OR kind = $kind) AND ($since = NONE OR at >= $since)
  AND connector.status != 'deleting'
ORDER BY similarity DESC LIMIT $k;";
    /// The cleanup: one batch of Slack messages older than the cutoff (index
    /// `farm_record_kind_at`). Documents stay while they are under the Drive folder.
    pub const DELETE_RECORDS_BEFORE: &str = "DELETE (SELECT VALUE id FROM farm_record WHERE kind = 'message' AND at < $cutoff LIMIT 500) RETURN VALUE $before.id;";
}

/// `connector.sources[*]`.
#[derive(Debug, Clone, SurrealValue)]
pub struct SourceRow {
    /// The channel or folder id.
    pub id: String,
    /// Its name.
    pub name: String,
}

/// `connector.oauth`: a pending connect's state. No `Debug`: it holds the nonce and the sealed
/// verifier.
#[derive(Clone, SurrealValue)]
pub struct OAuthRow {
    /// The nonce the signed state carries.
    pub nonce: String,
    /// `SHA-256(device_verifier)`.
    pub device_hash: String,
    /// Google's sealed PKCE verifier.
    pub verifier: Option<Bytes>,
    /// 10 minutes after the start.
    pub expires_at: DateTime<Utc>,
    /// When a `complete` claimed the state.
    pub claimed_at: Option<DateTime<Utc>>,
}

/// A `connector` as created. No `Debug`: it holds the sealed tokens.
#[derive(Clone, SurrealValue)]
pub struct ConnectorWriteRow {
    /// Its garden.
    pub garden: RecordId,
    /// Slack or Google Drive.
    pub kind: ConnectorKindRow,
    /// `pending`, or `active` for a seeded connector.
    pub status: ConnectorStatusRow,
    /// The owner who consented.
    pub created_by: RecordId,
    /// `ConnectorAccount`, when known.
    pub account: Option<serde_json::Value>,
    /// The chosen sources.
    pub sources: Vec<SourceRow>,
    /// The sealed tokens.
    pub secret: Option<Bytes>,
    /// The pending connect's state.
    pub oauth: Option<OAuthRow>,
    /// `vendor` or `era`.
    pub endpoint: ConnectorEndpointRow,
    /// Due at once (a seeded connector syncs on the first run).
    pub next_sync_at: DateTime<Utc>,
    /// Now.
    pub created_at: DateTime<Utc>,
}

/// What a `ConnectorRow` adds to `Connector`, read from [`sql::CONNECTOR_ROW`]'s projection.
#[derive(Clone, SurrealValue)]
pub struct ConnectorStateRow {
    /// The owner who consented.
    pub created_by: RecordId,
    /// The sealed tokens.
    pub secret: Option<Bytes>,
    /// The pending connect's state.
    pub oauth: Option<OAuthRow>,
    /// Each source's cursor, by source id.
    pub cursor: Option<BTreeMap<String, String>>,
    /// When the sync is due.
    pub next_sync_at: DateTime<Utc>,
    /// The last failure.
    pub last_error: Option<SyncErrorCodeRow>,
    /// The last accepted **Sync now**.
    pub sync_requested_at: Option<DateTime<Utc>>,
}

/// One chunk as [`sql::UPSERT_RECORDS`] reads it; `embedding` is NONE when the stored vector
/// stays.
#[derive(Debug, Clone, SurrealValue)]
pub struct FarmRecordWriteRow {
    /// `farm_record:[connector, vendor_id, n]`.
    pub id: RecordId,
    /// `message` or `document`.
    pub kind: FarmRecordKindRow,
    /// The vendor's id.
    pub vendor_id: String,
    /// The Slack channel id or the Drive folder id.
    pub source: String,
    /// The channel or document title.
    pub title: String,
    /// The author's display name.
    pub author: Option<String>,
    /// The permalink or `webViewLink`.
    pub link: String,
    /// One chunk.
    pub text: String,
    /// The hash of `text`.
    pub text_hash: String,
    /// The new vector, or NONE to keep the stored one.
    pub embedding: Option<Vec<f32>>,
    /// The embedding model, with the vector.
    pub embedding_model: Option<String>,
    /// The message's time or the document's `modifiedTime`.
    pub at: DateTime<Utc>,
}

/// What [`sql::RECORD_HASHES`] reads.
#[derive(Debug, Clone, SurrealValue)]
pub struct RecordHashRow {
    /// The record.
    pub id: RecordId,
    /// Its stored hash.
    pub text_hash: String,
}

/// A search hit as [`sql::SEARCH`] reads it.
#[derive(Debug, Clone, SurrealValue)]
pub struct FarmHitRow {
    /// The record.
    pub id: RecordId,
    /// Its connector's kind.
    pub system: ConnectorKindRow,
    /// The channel or document title.
    pub title: String,
    /// The author's display name.
    pub author: Option<String>,
    /// The link.
    pub link: String,
    /// The chunk.
    pub text: String,
    /// When.
    pub at: DateTime<Utc>,
    /// The cosine similarity to the query.
    pub similarity: Option<f64>,
}

impl ConnectorWriteRow {
    fn new(garden: GardenId, c: &NewConnector) -> Result<Self, RowError> {
        Ok(Self {
            garden: rid(&garden),
            kind: c.kind.into(),
            status: c.status.into(),
            created_by: rid(&c.created_by),
            account: c
                .account
                .as_ref()
                .map(|a| to_json("account", a))
                .transpose()?,
            sources: sources_row(&c.sources),
            secret: c.secret.as_ref().map(sealed_bytes),
            oauth: c.oauth.as_ref().map(OAuthRow::from),
            endpoint: c.endpoint.into(),
            next_sync_at: dt(c.created_at),
            created_at: dt(c.created_at),
        })
    }
}

impl From<&PendingConnect> for OAuthRow {
    fn from(p: &PendingConnect) -> Self {
        Self {
            nonce: p.nonce.clone(),
            device_hash: p.device_hash.to_string(),
            verifier: p.verifier.as_ref().map(sealed_bytes),
            expires_at: dt(p.expires_at),
            claimed_at: p.claimed_at.map(dt),
        }
    }
}

impl TryFrom<OAuthRow> for PendingConnect {
    type Error = RowError;

    fn try_from(row: OAuthRow) -> Result<Self, RowError> {
        Ok(Self {
            nonce: row.nonce,
            device_hash: row.device_hash.parse().map_err(|_| RowError::BadField {
                field: "oauth.device_hash",
                problem: "not a device hash".to_owned(),
            })?,
            verifier: row.verifier.map(|b| Sealed(b.into())),
            expires_at: ts(row.expires_at),
            claimed_at: row.claimed_at.map(ts),
        })
    }
}

impl FarmRecordWriteRow {
    fn new(r: &NewFarmRecord) -> Self {
        Self {
            id: farm_record_rid(&r.id),
            kind: r.kind.into(),
            vendor_id: r.id.vendor_id.to_string(),
            source: r.source.clone(),
            title: r.title.clone(),
            author: r.author.clone(),
            link: r.link.clone(),
            text: r.text.clone(),
            text_hash: r.text_hash.clone(),
            embedding: r.vector.as_ref().map(|v| v.embedding.0.clone()),
            embedding_model: r.vector.as_ref().map(|v| v.model.clone()),
            at: dt(r.at),
        }
    }
}

fn sealed_bytes(s: &Sealed) -> Bytes {
    Bytes::from(s.0.clone())
}

fn sources_row(sources: &[ConnectorSource]) -> Vec<SourceRow> {
    sources
        .iter()
        .map(|s| SourceRow {
            id: s.id.clone(),
            name: s.name.clone(),
        })
        .collect()
}

/// A `ConnectorRow` from [`sql::CONNECTOR_ROW`]'s projection: the sync state through
/// `SurrealValue`, the wire shape through its serde type (the sealed fields removed first, so
/// they never become JSON).
fn connector_row_of(value: Value) -> Result<ConnectorRow, RowError> {
    let state = ConnectorStateRow::from_value(value.clone()).map_err(|e| RowError::BadField {
        field: "connector",
        problem: e.to_string(),
    })?;
    let Value::Object(mut fields) = value else {
        return Err(RowError::BadField {
            field: "connector",
            problem: "not an object".to_owned(),
        });
    };
    for sealed in ["secret", "oauth", "cursor", "created_by"] {
        fields.remove(sealed);
    }
    let connector: Connector = decode("connector", Value::Object(fields))?;
    Ok(ConnectorRow {
        connector,
        created_by: from_rid(&state.created_by)?,
        secret: state.secret.map(|b| Sealed(b.into())),
        oauth: state.oauth.map(PendingConnect::try_from).transpose()?,
        cursors: state
            .cursor
            .unwrap_or_default()
            .into_iter()
            .map(|(source, cursor)| (source, SyncCursor(cursor)))
            .collect(),
        next_sync_at: ts(state.next_sync_at),
        last_error: state.last_error.map(SyncErrorCode::from),
        sync_requested_at: state.sync_requested_at.map(ts),
    })
}

/// Every `ConnectorRow` of statement `index`.
fn take_rows(
    r: &mut surrealdb::IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<Vec<ConnectorRow>, StoreError> {
    let value: Value = r.take(index).map_err(|e| take_failed(what, e))?;
    let items = match value {
        Value::None | Value::Null => Vec::new(),
        Value::Array(items) => items.into_iter().collect(),
        one => vec![one],
    };
    items
        .into_iter()
        .map(|v| connector_row_of(v).map_err(|e| e.into_store(what)))
        .collect()
}

/// How many ids a batched `DELETE … RETURN VALUE $before.id` returned.
fn deleted(
    r: &mut surrealdb::IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<u32, StoreError> {
    let ids: Vec<RecordId> = r.take(index).map_err(|e| take_failed(what, e))?;
    Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
}

/// The `THROW` codes of this module, then the default mapping.
fn farm_failure(f: DbFailure, what: &'static str) -> StoreError {
    match f.thrown() {
        Some(thrown::INVALID_STATE | "connector_limit") => {
            StoreError::Rejected(DomainError::InvalidState(InvalidStateDetails::default()))
        }
        Some("connect_lost") => StoreError::Conflict(Conflict::AlreadyExists),
        _ => f.into_store(what),
    }
}

#[async_trait::async_trait]
impl FarmRecordStore for SurrealStore {
    async fn create_connector(
        &self,
        owner: Membership<Owner>,
        connector: NewConnector,
    ) -> Result<Connector, StoreError> {
        const WHAT: &str = "create connector";
        let row =
            ConnectorWriteRow::new(owner.garden(), &connector).map_err(|e| e.into_store(WHAT))?;
        let mut r = run_tx(&self.db, sql::CREATE_CONNECTOR, |q| {
            q.bind(("connector", rid(&connector.id)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("max", i64::from(MAX_CONNECTORS_PER_GARDEN)))
                .bind(("row", row.clone()))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn seed_connector(
        &self,
        garden: GardenId,
        connector: NewConnector,
    ) -> Result<Connector, StoreError> {
        const WHAT: &str = "seed connector";
        let row = ConnectorWriteRow::new(garden, &connector).map_err(|e| e.into_store(WHAT))?;
        let mut r = run_tx(&self.db, sql::SEED_CONNECTOR, |q| {
            q.bind(("connector", rid(&connector.id)))
                .bind(("garden", rid(&garden)))
                .bind(("row", row.clone()))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn connectors(&self, member: Membership<Viewer>) -> Result<Vec<Connector>, StoreError> {
        const WHAT: &str = "list connectors";
        let mut r = run_tx(&self.db, sql::CONNECTORS, |q| {
            q.bind(("garden", rid(&member.garden())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_all(&mut r, 0, WHAT)
    }

    async fn connector_row(
        &self,
        connector: ConnectorId,
    ) -> Result<Option<ConnectorRow>, StoreError> {
        const WHAT: &str = "read connector";
        let mut r = run_tx(&self.db, sql::CONNECTOR_ROW, |q| {
            q.bind(("connector", rid(&connector)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        Ok(take_rows(&mut r, 0, WHAT)?.into_iter().next())
    }

    async fn complete_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        done: CompletedConnect,
    ) -> Result<Connector, StoreError> {
        const WHAT: &str = "complete connect";
        let account = to_json("account", &done.account).map_err(|e| e.into_store(WHAT))?;
        let mut r = run_tx(&self.db, sql::COMPLETE_CONNECT, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("nonce", done.nonce.clone()))
                .bind(("account", account.clone()))
                .bind(("secret", sealed_bytes(&done.secret)))
                .bind(("now", dt(done.now)))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn claim_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        nonce: String,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "claim connect";
        run_tx(&self.db, sql::CLAIM_CONNECT, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("nonce", nonce.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        Ok(())
    }

    async fn release_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        nonce: String,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "release connect";
        run_tx(&self.db, sql::RELEASE_CONNECT, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("nonce", nonce.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        Ok(())
    }

    async fn set_sources(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        sources: Vec<ConnectorSource>,
        now: Timestamp,
    ) -> Result<Connector, StoreError> {
        const WHAT: &str = "set connector sources";
        let sources = sources_row(&sources);
        let mut r = run_tx(&self.db, sql::SET_SOURCES, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("sources", sources.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn request_sync(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        now: Timestamp,
    ) -> Result<SyncRequest, StoreError> {
        const WHAT: &str = "request sync";
        let mut r = run_tx(&self.db, sql::REQUEST_SYNC, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("garden", rid(&owner.garden())))
                .bind(("now", dt(now)))
                .bind(("cutoff", dt(now.minus(SYNC_NOW_INTERVAL))))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        // BEGIN, LET, IF, IF, UPDATE (the compare-and-set), RETURN, COMMIT (counted from the end).
        let n = r.num_statements();
        let accepted: Vec<RecordId> = r
            .take(n.saturating_sub(3))
            .map_err(|e| take_failed(WHAT, e))?;
        if !accepted.is_empty() {
            return Ok(SyncRequest::Accepted);
        }
        let last: Option<DateTime<Utc>> = r
            .take(n.saturating_sub(2))
            .map_err(|e| take_failed(WHAT, e))?;
        let last = last.map_or(now, ts);
        Ok(SyncRequest::TooSoon {
            allowed_at: last.plus(SYNC_NOW_INTERVAL),
        })
    }

    async fn due_connectors(
        &self,
        now: Timestamp,
        limit: u32,
    ) -> Result<Vec<ConnectorRow>, StoreError> {
        const WHAT: &str = "due connectors";
        let mut r = run_tx(&self.db, sql::DUE_CONNECTORS, |q| {
            q.bind(("now", dt(now))).bind(("limit", i64::from(limit)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_rows(&mut r, 0, WHAT)
    }

    async fn finish_sync(
        &self,
        connector: ConnectorId,
        outcome: SyncOutcome,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "finish sync";
        let cursor: BTreeMap<String, String> = outcome
            .cursors
            .iter()
            .map(|(source, cursor)| (source.clone(), cursor.0.clone()))
            .collect();
        run_tx(&self.db, sql::FINISH_SYNC, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("cursor", cursor.clone()))
                .bind(("synced_at", outcome.synced_at.map(dt)))
                .bind(("due_at", dt(outcome.due_at)))
                .bind(("next", dt(outcome.next_sync_at)))
                .bind(("last_error", outcome.last_error.map(SyncErrorCodeRow::from)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn set_status(
        &self,
        connector: ConnectorId,
        status: ConnectorStatus,
        last_error: Option<SyncErrorCode>,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "set connector status";
        run_tx(&self.db, sql::SET_STATUS, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("status", ConnectorStatusRow::from(status)))
                .bind(("last_error", last_error.map(SyncErrorCodeRow::from)))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn start_connector_deletion(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
    ) -> Result<ConnectorRow, StoreError> {
        const WHAT: &str = "start connector deletion";
        let mut r = run_tx(&self.db, sql::START_DELETION, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("garden", rid(&owner.garden())))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_rows(&mut r, last, WHAT)?
            .into_iter()
            .next()
            .ok_or(StoreError::NotFound)
    }

    async fn delete_connector_batch(&self, connector: ConnectorId) -> Result<u32, StoreError> {
        const WHAT: &str = "delete connector batch";
        let mut r = run_tx(&self.db, sql::DELETE_CONNECTOR_RECORDS, |q| {
            q.bind(("connector", rid(&connector)))
        })
        .await
        .map_err(|f| farm_failure(f, WHAT))?;
        // BEGIN, LET, IF, DELETE, COMMIT.
        let index = r.num_statements().saturating_sub(2);
        let records = deleted(&mut r, index, WHAT)?;
        if records > 0 {
            return Ok(records);
        }
        let mut r = run_tx(&self.db, sql::DELETE_CONNECTOR, |q| {
            q.bind(("connector", rid(&connector)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        deleted(&mut r, 0, WHAT)
    }

    async fn connectors_created_by(
        &self,
        garden: GardenId,
        user: UserId,
    ) -> Result<Vec<ConnectorId>, StoreError> {
        const WHAT: &str = "connectors created by";
        let mut r = run_tx(&self.db, sql::CREATED_BY, |q| {
            q.bind(("garden", rid(&garden))).bind(("user", rid(&user)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        ids.iter()
            .map(from_rid)
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn shared_account_connectors(
        &self,
        kind: ConnectorKind,
        account: String,
        except: ConnectorId,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "shared account connectors";
        let statement = match kind {
            ConnectorKind::Slack => sql::SLACK_ACCOUNT_CONNECTORS,
            ConnectorKind::GoogleDrive => sql::GOOGLE_ACCOUNT_CONNECTORS,
        };
        let mut r = run_tx(&self.db, statement, |q| {
            q.bind(("account", account.clone()))
                .bind(("except", rid(&except)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let count: Option<i64> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(count.unwrap_or(0)).unwrap_or(u32::MAX))
    }

    async fn replace_secret(
        &self,
        connector: ConnectorId,
        read: Sealed,
        new: Sealed,
    ) -> Result<bool, StoreError> {
        const WHAT: &str = "replace connector secret";
        let mut r = run_tx(&self.db, sql::REPLACE_SECRET, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("read", sealed_bytes(&read)))
                .bind(("new", sealed_bytes(&new)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(!ids.is_empty())
    }

    async fn deleting_connectors(
        &self,
        garden: Option<GardenId>,
    ) -> Result<Vec<ConnectorRow>, StoreError> {
        const WHAT: &str = "deleting connectors";
        let statement = if garden.is_some() {
            sql::DELETING_OF_GARDEN
        } else {
            sql::DELETING_TO_RESUME
        };
        let mut r = run_tx(&self.db, statement, |q| {
            q.bind(("garden", garden.as_ref().map(rid)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_rows(&mut r, 0, WHAT)
    }

    async fn deleting_connectors_of_user(
        &self,
        user: UserId,
    ) -> Result<Vec<ConnectorRow>, StoreError> {
        const WHAT: &str = "deleting connectors of a user";
        let mut r = run_tx(&self.db, sql::DELETING_OF_USER, |q| {
            q.bind(("user", rid(&user)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_rows(&mut r, 0, WHAT)
    }

    async fn delete_expired_pending(
        &self,
        before: Timestamp,
        claimed_before: Timestamp,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "delete expired pending connectors";
        let mut r = run_tx(&self.db, sql::DELETE_EXPIRED_PENDING, |q| {
            q.bind(("cutoff", dt(before)))
                .bind(("claimed_cutoff", dt(claimed_before)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        deleted(&mut r, 0, WHAT)
    }

    async fn record_hashes(
        &self,
        connector: ConnectorId,
        vendor_ids: Vec<String>,
    ) -> Result<Vec<(FarmRecordId, String)>, StoreError> {
        const WHAT: &str = "farm record hashes";
        let mut r = run_tx(&self.db, sql::RECORD_HASHES, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("vendor_ids", vendor_ids.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<RecordHashRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| Ok((farm_record_id_of(&row.id)?, row.text_hash)))
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn upsert_records(
        &self,
        connector: ConnectorId,
        records: Vec<NewFarmRecord>,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "upsert farm records";
        if records.is_empty() {
            return Ok(0);
        }
        if let Some(other) = records.iter().find(|r| r.id.connector != connector) {
            return Err(RowError::BadField {
                field: "farm_record.id",
                problem: format!("{} is not a record of connector {connector}", other.id),
            }
            .into_store(WHAT));
        }
        let rows: Vec<FarmRecordWriteRow> = records.iter().map(FarmRecordWriteRow::new).collect();
        let mut r = run_tx(&self.db, sql::UPSERT_RECORDS, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("records", rows.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        let written: Option<i64> = r.take(last).map_err(|e| take_failed(WHAT, e))?;
        Ok(written.map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)))
    }

    async fn delete_records(
        &self,
        connector: ConnectorId,
        vendor_ids: Vec<String>,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "delete farm records";
        if vendor_ids.is_empty() {
            return Ok(0);
        }
        let mut r = run_tx(&self.db, sql::DELETE_RECORDS, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("vendor_ids", vendor_ids.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        deleted(&mut r, 0, WHAT)
    }

    async fn delete_chunks_from(
        &self,
        connector: ConnectorId,
        vendor_id: String,
        from: u32,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "delete farm record chunks";
        let mut r = run_tx(&self.db, sql::DELETE_CHUNKS_FROM, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("vendor_id", vendor_id.clone()))
                .bind(("from", i64::from(from)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        deleted(&mut r, 0, WHAT)
    }

    async fn source_vendor_ids_since(
        &self,
        connector: ConnectorId,
        source: String,
        since: Timestamp,
    ) -> Result<Vec<String>, StoreError> {
        const WHAT: &str = "source vendor ids";
        let mut r = run_tx(&self.db, sql::SOURCE_VENDOR_IDS_SINCE, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("source", source.clone()))
                .bind(("since", dt(since)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        r.take(0).map_err(|e| take_failed(WHAT, e))
    }

    async fn delete_source_records(
        &self,
        connector: ConnectorId,
        source: String,
    ) -> Result<u32, StoreError> {
        const WHAT: &str = "delete a source's farm records";
        let mut r = run_tx(&self.db, sql::DELETE_SOURCE_RECORDS, |q| {
            q.bind(("connector", rid(&connector)))
                .bind(("source", source.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        deleted(&mut r, 0, WHAT)
    }

    async fn garden_record_count(&self, garden: GardenId) -> Result<u32, StoreError> {
        const WHAT: &str = "garden farm record count";
        let mut r = run_tx(&self.db, sql::GARDEN_RECORD_COUNT, |q| {
            q.bind(("garden", rid(&garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let count: Option<i64> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(count.unwrap_or(0)).unwrap_or(u32::MAX))
    }

    async fn search_records(
        &self,
        member: Membership<Viewer>,
        query: &Embedding,
        filter: FarmRecordFilter,
        k: usize,
    ) -> Result<Vec<FarmRecordMatch>, StoreError> {
        const WHAT: &str = "search farm records";
        let mut r = run_tx(&self.db, sql::SEARCH, |q| {
            q.bind(("garden", rid(&member.garden())))
                .bind(("query", query.0.clone()))
                .bind(("kind", filter.kind.map(FarmRecordKindRow::from)))
                .bind(("since", filter.since.map(dt)))
                .bind(("k", i64::try_from(k).unwrap_or(i64::MAX)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<FarmHitRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a cosine similarity is in -1..=1"
                )]
                let similarity = row.similarity.unwrap_or(0.0) as f32;
                Ok(FarmRecordMatch {
                    hit: FarmRecordHit {
                        text: row.text,
                        citation: FarmCitation {
                            record_id: farm_record_id_of(&row.id)?,
                            system: ConnectorKind::from(row.system),
                            title: row.title,
                            author: row.author,
                            at: ts(row.at),
                            link: row.link,
                        },
                    },
                    similarity,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn delete_records_before(&self, cutoff: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "delete old farm records";
        let mut r = run_tx(&self.db, sql::DELETE_RECORDS_BEFORE, |q| {
            q.bind(("cutoff", dt(cutoff)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        deleted(&mut r, 0, WHAT)
    }
}

/// The batch size every batched delete here uses, as `DELETE_BATCH` fixes it.
const _: () = assert!(DELETE_BATCH == 500);
