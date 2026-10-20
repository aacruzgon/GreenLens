//! Scan events ([contracts 4]; [database 4], scan events: a gap-free `seq` ✓).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Membership, Viewer},
    events::{ScanEvent, ScanEventBody},
    id::ScanId,
    ports::{EventRepo, NewEvent, StoreError},
    rules::stream::StreamState,
    url::Stored,
    wire::{EventSeq, Run, V1},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{RowError, take_failed},
    ids::{from_rid, rid, seq_i64, seq_of},
    rows::{dt, from_json, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// The append ([database 4] ✓), with a guard for a missing scan. Statements: 0 `BEGIN`,
    /// 1 `LET`, 2 `IF`, 3 `CREATE` (the event), 4 `COMMIT`. `ts` is bound from the event, so
    /// the clock port controls it.
    pub const APPEND_EVENT: &str = "BEGIN;
LET $seq = (UPDATE ONLY $scan SET last_event_seq += 1 RETURN VALUE last_event_seq);
IF $seq = NONE { THROW 'not_found' };
CREATE type::record('scan_event', [$scan, $seq]) SET run = $run, type = $type, payload = $payload, ts = $ts;
COMMIT;";

    /// The index of the created event in [`APPEND_EVENT`].
    pub const APPEND_EVENT_RESULT: usize = 3;

    /// Replay after `Last-Event-ID` ([database 4] ✓): a key-range read, no index, scoped to the
    /// proof's garden and skipping a scan, plant or garden being deleted. Statements: 0 `IF`,
    /// 1 `SELECT`.
    pub const EVENTS_AFTER: &str =
        "IF array::len(SELECT VALUE id FROM $scan WHERE garden = $garden AND deleting_at = NONE
      AND plant.deleting_at = NONE AND garden.deleting_at = NONE) = 0 { THROW 'not_found' };
SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..];";

    /// A run's `run_started` and stage events, for a job's `complete` ([contracts 5], partial
    /// results): the scan's key range, filtered by run and type, in `seq` order. No garden
    /// scope: `api` reads it for a job it holds the lease of. Statements: 0 `SELECT`.
    pub const RUN_STAGE_EVENTS: &str = "SELECT * FROM scan_event:[$scan, 1]..=[$scan, ..]
WHERE run = $run AND type IN ['run_started', 'stage_started', 'stage_completed'];";

    /// What the closing rule needs ([contracts 4], when the stream closes).
    pub const STREAM_STATE: &str = "SELECT status, open_question.status AS question_status, pipeline_step != NONE AS step_claimed,
       array::len(SELECT VALUE id FROM guided_check WHERE scan = $parent.id AND status = 'analysing') AS checks_analysing
FROM ONLY $scan WHERE deleting_at = NONE;";
}

/// A `scan_event` row. The body is split as stored: `type` beside a `payload` object holding
/// the rest of [`ScanEventBody<Stored>`]'s fields (image and mask ids, never URLs).
#[derive(Debug, Clone, SurrealValue)]
pub struct EventRow {
    /// `scan_event:[scan, seq]`.
    pub id: RecordId,
    /// The run.
    pub run: i64,
    /// The event type.
    #[surreal(rename = "type")]
    pub kind: String,
    /// The body without `type`.
    pub payload: serde_json::Value,
    /// When it was stored.
    pub ts: DateTime<Utc>,
}

/// What [`sql::STREAM_STATE`] reads.
#[derive(Debug, Clone, SurrealValue)]
pub struct StreamStateRow {
    /// The scan's status.
    pub status: crate::rows::ScanStatusRow,
    /// The open question's status, if the scan has one.
    pub question_status: Option<String>,
    /// A step holds the claim (`pipeline_step != NONE`).
    pub step_claimed: bool,
    /// Guided checks still `analysing`.
    pub checks_analysing: i64,
}

/// The stored parts of a new event's body.
pub struct EventParts {
    /// `type`.
    pub kind: &'static str,
    /// The other fields.
    pub payload: serde_json::Value,
}

impl TryFrom<&ScanEventBody<Stored>> for EventParts {
    type Error = RowError;

    fn try_from(body: &ScanEventBody<Stored>) -> Result<Self, RowError> {
        let mut payload = to_json("payload", body)?;
        if let Some(object) = payload.as_object_mut() {
            object.remove("type");
        }
        Ok(Self {
            kind: body.event_type().as_str(),
            payload,
        })
    }
}

impl TryFrom<EventRow> for ScanEvent<Stored> {
    type Error = RowError;

    fn try_from(row: EventRow) -> Result<Self, RowError> {
        let seq = seq_of(&row.id)?;
        let scan = match &row.id.key {
            surrealdb::types::RecordIdKey::Array(parts) => match parts.first() {
                Some(surrealdb::types::Value::RecordId(scan)) => from_rid::<ScanId>(scan)?,
                _ => return Err(bad_key(&row.id)),
            },
            _ => return Err(bad_key(&row.id)),
        };
        let mut payload = row.payload;
        let Some(object) = payload.as_object_mut() else {
            return Err(RowError::BadField {
                field: "payload",
                problem: "not an object".to_owned(),
            });
        };
        object.insert("type".to_owned(), serde_json::Value::String(row.kind));
        Ok(ScanEvent {
            v: V1,
            seq,
            run: Run(u32_of("run", row.run)?),
            scan_id: scan,
            ts: ts(row.ts),
            body: from_json("payload", payload)?,
        })
    }
}

fn bad_key(id: &RecordId) -> RowError {
    RowError::BadId {
        id: format!("{id:?}"),
        expected: "scan_event",
    }
}

#[async_trait::async_trait]
impl EventRepo for SurrealStore {
    async fn append_event(
        &self,
        scan: ScanId,
        event: NewEvent,
    ) -> Result<ScanEvent<Stored>, StoreError> {
        const WHAT: &str = "append event";
        let parts = EventParts::try_from(&event.body).map_err(|e| e.into_store(WHAT))?;
        let mut results = run_tx(&self.db, sql::APPEND_EVENT, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("run", i64::from(event.run.0)))
                .bind(("type", parts.kind))
                .bind(("payload", parts.payload.clone()))
                .bind(("ts", dt(event.ts)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<EventRow> = results
            .take(sql::APPEND_EVENT_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        row.ok_or(StoreError::NotFound)?
            .try_into()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn events_after(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
        after: EventSeq,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError> {
        const WHAT: &str = "replay events";
        let mut results = run_tx(&self.db, sql::EVENTS_AFTER, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("garden", rid(&member.garden())))
                .bind(("after", seq_i64(after)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<EventRow> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| ScanEvent::try_from(row).map_err(|e| e.into_store(WHAT)))
            .collect()
    }

    async fn run_stage_events(
        &self,
        scan: ScanId,
        run: Run,
    ) -> Result<Vec<ScanEventBody<Stored>>, StoreError> {
        const WHAT: &str = "read a run's stage events";
        let mut results = run_tx(&self.db, sql::RUN_STAGE_EVENTS, |q| {
            q.bind(("scan", rid(&scan))).bind(("run", i64::from(run.0)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<EventRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                ScanEvent::try_from(row)
                    .map(|e| e.body)
                    .map_err(|e| e.into_store(WHAT))
            })
            .collect()
    }

    async fn stream_state(&self, scan: ScanId) -> Result<StreamState, StoreError> {
        const WHAT: &str = "stream state";
        let mut results = run_tx(&self.db, sql::STREAM_STATE, |q| {
            q.bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<StreamStateRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(StreamState {
            status: row.status.into(),
            question_unresolved: matches!(
                row.question_status.as_deref(),
                Some("open" | "answering")
            ),
            guided_check_analysing: row.checks_analysing > 0,
            step_claimed: row.step_claimed,
        })
    }
}
