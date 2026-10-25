//! The job broker's storage ([contracts 5]; [database 4], job claim with `slots` ✓).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use domain::{
    garden::ops::{QueueCounts, Status, WorkerStatus},
    id::{ImageId, JobId, WorkerId},
    jobs::{Capability, Heartbeat, HeartbeatResponse, Job, JobKind, PerceptionStage},
    ports::{
        ClaimAttempt, ClaimedJob, CompletionOutcome, Conflict, ContentType, JobCompletion, JobRepo,
        Lease, LeasedJob, LiveWorker, NewJob, ObjectKey, ObjectRef, RequeuedJob, StoreError,
    },
    rules::jobs::{ClaimPolicy, FREE_REQUEUES, MAX_ATTEMPTS, WORKER_STALE_AFTER},
    wire::{Run, Timestamp, V1},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown},
    ids::{from_opt_rid, from_rid, opt_rid, rid, worker_rid},
    repo::{
        analysis::{ObservationWriteRow, observation_diff},
        steps::{ClaimRow, claim_attempt},
    },
    rows::{JobKindRow, JobStatusRow, PipelineStepRow, dt, from_json, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements. Every `time::now()` of [database 4] is the bound `$now`, so the `Clock`
/// port (and a test's `FixedClock`) controls lease times.
pub mod sql {
    use crate::fragments::{claim_entry, evidence_write, observation_write};

    /// The shared head of the two `done` transactions.
    macro_rules! complete_done_head {
        () => {
            concat!(
                "BEGIN;
LET $j = (SELECT status, lease_token, worker, lease_until, kind, scan, run, question FROM ONLY $job);
IF $j = NONE OR $j.status != 'leased' OR $j.lease_token != $lease_token OR $j.worker != $worker OR $j.lease_until < $now { THROW 'lease_lost' };
LET $scan = $j.scan;
LET $run = $j.run;
LET $measured_images = [];
FOR $obs IN $writes {
",
                observation_write!(),
                "};\n",
                evidence_write!(),
                "UPDATE $job SET status = 'done', result = $result, finished_at = $now;
IF $j.question != NONE AND $j.kind = 'perception' {
  UPDATE $j.question SET step = 'measuring', step_at = $now WHERE status = 'answering' AND step = 'segmenting';
};
IF $j.question != NONE AND $j.kind = 'verify' {
  UPDATE $j.question SET step = NONE, step_at = NONE WHERE status = 'answering';
};
"
            )
        };
    }

    /// Queues a job on its own (a guided check's; run jobs are queued in step commits and run
    /// starts).
    pub const ENQUEUE_JOB: &str = "CREATE $job.id CONTENT $job.row;";

    /// The claim ([database 4] ✓): counts the worker's leased jobs against `slots`, then
    /// leases the oldest eligible queued job of its kinds, re-checking `status = 'queued'`. The
    /// fallback's filter holds a photo answer's segmentation job while a perception worker is
    /// alive. Added to the documented statement: `max_runtime` is set at claim from
    /// `$runtimes`, the claimer's runtime per kind (`JobKind::max_runtime`: doubled on the
    /// fallback, [contracts 5]), so it doubles once from the kind's base however often the job
    /// is requeued and claimed again, and a GPU worker that claims it after the fallback gets
    /// the base runtime back; and a run's first job moves the scan from `queued` to
    /// `analysing` in the same transaction. Statements: 0 `BEGIN`, 1 `LET $held`,
    /// 2 `LET $perception_alive`, 3 `LET $claimed`, 4 `LET $job`, 5 `LET $started`,
    /// 6 the result, 7 `COMMIT`.
    pub const CLAIM_JOB: &str = "BEGIN;
LET $held = (SELECT count() AS n FROM job WHERE worker = $worker AND status = 'leased' GROUP ALL)[0].n ?? 0;
LET $perception_alive = ((SELECT count() AS n FROM worker WHERE caps CONTAINS 'perception' AND last_heartbeat > $now - 30s GROUP ALL)[0].n ?? 0) > 0;
LET $claimed = IF $held < $slots {
  (UPDATE (SELECT VALUE id FROM job
          WHERE status = 'queued' AND kind IN $kinds AND queued_at <= $eligible_before
            AND (!$hold_question OR question = NONE OR !$perception_alive)
          ORDER BY queued_at LIMIT 1)
  SET status = 'leased', worker = $worker, lease_token = rand::ulid(),
      claimed_at = $now, lease_until = $now + 2m,
      max_runtime = $runtimes[kind]
  WHERE status = 'queued'
  RETURN AFTER)
} ELSE { [] };
LET $job = $claimed[0];
LET $started = IF $job != NONE AND $job.run != NONE {
  array::len(UPDATE $job.scan SET status = 'analysing' WHERE status = 'queued' AND run = $job.run RETURN VALUE id) > 0
} ELSE { false };
RETURN { job: $job, started_analysing: $started };
COMMIT;";
    /// The index of the result in [`CLAIM_JOB`].
    pub const CLAIM_JOB_RESULT: usize = 6;

    /// A heartbeat: upserts `worker:[worker_id]` with its `versions` verbatim, extends each
    /// listed lease whose token matches (never past `claimed_at + max_runtime`), and returns
    /// the listed jobs the worker must stop: cancelled, requeued or leased elsewhere.
    pub const HEARTBEAT: &str = "BEGIN;
UPSERT $worker SET caps = $caps, slots = $slots, gpu = $gpu, versions = $versions, last_heartbeat = $now;
FOR $h IN $held {
  UPDATE $h.job SET lease_until = IF $now + 2m < claimed_at + max_runtime { $now + 2m } ELSE { claimed_at + max_runtime }
    WHERE status = 'leased' AND lease_token = $h.token AND worker = $worker_id;
};
RETURN array::filter($held, |$h| array::len(SELECT VALUE id FROM $h.job
    WHERE status = 'leased' AND lease_token = $h.token AND worker = $worker_id) = 0).map(|$h| $h.job);
COMMIT;";
    /// The index of the result in [`HEARTBEAT`].
    pub const HEARTBEAT_RESULT: usize = 3;

    /// Requeues expired leases (index `job_lease` ✓) and the jobs of workers silent for 30
    /// seconds, resetting `queued_at`, invalidating the token and counting attempts
    /// ([contracts 5]: every requeue after the 2nd counts); a job out of attempts fails for
    /// good instead, and a `guided` job out of attempts marks its check `failed` in the same
    /// transaction (contracts 5). Statements: 0 `BEGIN`, 1–3 `LET`, 4 `FOR`, 5 the requeued jobs, 6 `COMMIT`.
    pub const REQUEUE_STALE: &str = "BEGIN;
LET $silent = (SELECT VALUE record::id(id)[0] FROM worker WHERE last_heartbeat < $now - 30s);
LET $expired = (SELECT VALUE id FROM job WHERE status = 'leased' AND lease_until < $now);
LET $orphaned = (SELECT VALUE id FROM job WHERE status = 'leased' AND worker IN $silent);
FOR $j IN array::union($expired, $orphaned) {
  LET $r = (SELECT requeues, attempts FROM ONLY $j);
  LET $att = IF $r.requeues >= $free_requeues { $r.attempts + 1 } ELSE { $r.attempts };
  UPDATE $j SET requeues = $r.requeues + 1, attempts = $att,
    status = IF $att >= $max_attempts { 'failed' } ELSE { 'queued' },
    finished_at = IF $att >= $max_attempts { $now } ELSE { NONE },
    worker = NONE, lease_token = NONE, lease_until = NONE, claimed_at = NONE, queued_at = $now;
  IF $att >= $max_attempts AND $j.kind = 'guided' {
    UPDATE guided_check SET status = 'failed'
      WHERE scan = $j.scan AND status = 'analysing' AND image = type::record('scan_image', $j.payload.image.image_id);
  };
};
SELECT id, kind, scan, run, question, status, payload.stages AS stages FROM job WHERE id IN array::union($expired, $orphaned);
COMMIT;";
    /// The index of the result in [`REQUEUE_STALE`].
    pub const REQUEUE_STALE_RESULT: usize = 5;

    /// Whether a perception worker heartbeated in the last 30 seconds ([database 4] ✓).
    pub const PERCEPTION_ALIVE: &str = "RETURN ((SELECT count() AS n FROM worker WHERE caps CONTAINS 'perception' AND last_heartbeat > $now - 30s GROUP ALL)[0].n ?? 0) > 0;";

    /// The workers with a heartbeat in the last 30 seconds, with their `versions`.
    pub const LIVE_WORKERS: &str =
        "SELECT caps, versions FROM worker WHERE last_heartbeat > $now - 30s;";

    /// A lease check for `events` and `complete`: the token and worker, and either a live lease
    /// or a job this same lease already finished (`done` or `failed`), so a repeated `complete`
    /// reaches `complete_job`, which answers `AlreadyCompleted` (contracts 5: `204`). A requeue
    /// clears the token, so a requeued job never matches.
    pub const CHECK_LEASE: &str =
        "SELECT id, kind, scan, run, question, status, payload.stages AS stages FROM $job
WHERE lease_token = $lease_token AND worker = $worker
  AND ((status = 'leased' AND lease_until >= $now) OR status IN ['done', 'failed']);";

    /// `GET /internal/jobs/{job_id}/images/{image_id}`: the image only if the leased job's
    /// payload names it.
    pub const JOB_IMAGE: &str = "LET $j = (SELECT scan, payload FROM ONLY $job WHERE status = 'leased' AND lease_token = $lease_token AND worker = $worker AND lease_until >= $now);
IF $j = NONE { THROW 'lease_lost' };
RETURN $j.payload;
SELECT VALUE storage_key FROM $image WHERE scan = $j.scan;";

    /// What `complete_job` reads first: a repeat by the same lease after `done` or `failed`
    /// answers `AlreadyCompleted`; anything else but a live lease is `lease_lost`.
    pub const LEASE_STATE: &str = "SELECT status, lease_token, worker, lease_until, kind, scan, run, question, payload.stages AS stages FROM ONLY $job;";

    /// `complete` with `status: done`, under the lease: each stage's observations (replacing
    /// per image, [database 5]), the VLM's indicator answers (ranked, schema v1.4), the job
    /// `done`, a question photo's segmentation moving its question to `measuring` and a `verify`
    /// result clearing the question's step (it stays `answering` until `score` resolves it),
    /// then the step claim the result hands to ([contracts 3], claiming).
    pub const COMPLETE_DONE_CLAIM: &str = concat!(
        complete_done_head!(),
        "LET $step = $claim_step;\n",
        claim_entry!(),
        "COMMIT;"
    );

    /// As [`COMPLETE_DONE_CLAIM`], for a result that claims no step (a guided job, a question
    /// photo's jobs).
    pub const COMPLETE_DONE: &str = concat!(
        complete_done_head!(),
        "RETURN { outcome: 'not_needed', run: NONE };\n",
        "COMMIT;"
    );

    /// `complete` with `status: failed`: a retryable failure with attempts left is requeued
    /// (token cleared, `queued_at` reset); otherwise the job fails for good, and a `guided`
    /// job's check is marked `failed` in the same transaction (contracts 5). Returns whether it
    /// failed for good.
    pub const COMPLETE_FAILED: &str = "BEGIN;
LET $j = (SELECT status, lease_token, worker, lease_until, attempts, kind, scan, payload FROM ONLY $job);
IF $j = NONE OR $j.status != 'leased' OR $j.lease_token != $lease_token OR $j.worker != $worker OR $j.lease_until < $now { THROW 'lease_lost' };
LET $att = IF $retryable { $j.attempts + 1 } ELSE { $j.attempts };
LET $final = !$retryable OR $att >= $max_attempts;
UPDATE $job SET attempts = $att, error = $error,
  status = IF $final { 'failed' } ELSE { 'queued' },
  finished_at = IF $final { $now } ELSE { NONE },
  worker = IF $final { worker } ELSE { NONE },
  lease_token = IF $final { lease_token } ELSE { NONE },
  lease_until = NONE,
  claimed_at = IF $final { claimed_at } ELSE { NONE },
  queued_at = IF $final { queued_at } ELSE { $now };
IF $final AND $j.kind = 'guided' {
  UPDATE guided_check SET status = 'failed'
    WHERE scan = $j.scan AND status = 'analysing' AND image = type::record('scan_image', $j.payload.image.image_id);
};
RETURN $final;
COMMIT;";

    /// `GET /status`: workers and queue counts.
    pub const STATUS: &str = "SELECT record::id(id)[0] AS worker_id, caps, last_heartbeat, gpu.name AS gpu FROM worker ORDER BY worker_id;
RETURN count(SELECT VALUE id FROM job WHERE status = 'queued');
RETURN count(SELECT VALUE id FROM job WHERE status = 'leased');";

    /// Cleanup: finished jobs older than the cutoff, one batch of 500 ([database 4], cleanup).
    pub const DELETE_FINISHED_JOBS: &str = "DELETE (SELECT VALUE id FROM job WHERE status IN ['done', 'failed', 'cancelled'] AND finished_at < $cutoff LIMIT 500) RETURN VALUE $before.id;";
}

/// A `job` row as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewJobRow {
    /// `perception`, `vlm`, `verify` or `guided`.
    pub kind: JobKindRow,
    /// Its scan.
    pub scan: RecordId,
    /// Its run; `NONE` for guided jobs.
    pub run: Option<i64>,
    /// A photo answer's job: top-level for the fallback's claim.
    pub question: Option<RecordId>,
    /// The payload of the `JobSpec`.
    pub payload: serde_json::Value,
    /// The kind's `max_runtime` on a GPU worker.
    pub max_runtime: surrealdb::types::Duration,
    /// When it was queued.
    pub queued_at: DateTime<Utc>,
}

/// A job with its record id, as [`job_enqueue!`](crate::fragments) and
/// [`sql::ENQUEUE_JOB`] create it.
#[derive(Debug, Clone, SurrealValue)]
pub struct JobWriteRow {
    /// `job:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: NewJobRow,
}

impl JobWriteRow {
    /// The rows of a domain job.
    pub fn new(job: &NewJob) -> Result<Self, RowError> {
        let mut spec = to_json("payload", &job.spec)?;
        let payload = spec
            .get_mut("payload")
            .map(serde_json::Value::take)
            .unwrap_or_default();
        Ok(Self {
            id: rid(&job.id),
            row: NewJobRow {
                kind: job.spec.kind().into(),
                scan: rid(&job.scan),
                run: job.run.map(|r| i64::from(r.0)),
                question: opt_rid(job.question.as_ref()),
                payload,
                max_runtime: job.max_runtime.into(),
                queued_at: dt(job.queued_at),
            },
        })
    }
}

/// A leased `job` row, as the claim returns it.
#[derive(Debug, Clone, SurrealValue)]
pub struct LeasedJobRow {
    /// `job:<ulid>`.
    pub id: RecordId,
    /// Its kind.
    pub kind: JobKindRow,
    /// Its scan.
    pub scan: RecordId,
    /// Its run.
    pub run: Option<i64>,
    /// The payload.
    pub payload: serde_json::Value,
    /// The new token (`rand::ulid()`).
    pub lease_token: String,
    /// When the lease ends.
    pub lease_until: DateTime<Utc>,
    /// The runtime limit, doubled on the fallback.
    pub max_runtime: surrealdb::types::Duration,
    /// Failed attempts counted so far.
    pub attempts: i64,
}

impl TryFrom<LeasedJobRow> for Job {
    type Error = RowError;

    fn try_from(row: LeasedJobRow) -> Result<Self, RowError> {
        let kind = JobKind::from(row.kind);
        let spec = from_json(
            "payload",
            serde_json::json!({ "kind": kind.as_str(), "payload": row.payload }),
        )?;
        Ok(Self {
            v: V1,
            job_id: from_rid(&row.id)?,
            scan_id: from_rid(&row.scan)?,
            run: row.run.map(|r| u32_of("run", r).map(Run)).transpose()?,
            attempt: u32_of("attempts", row.attempts)? + 1,
            lease_token: row
                .lease_token
                .parse()
                .map_err(|e: domain::id::IdParseError| RowError::BadField {
                    field: "lease_token",
                    problem: e.to_string(),
                })?,
            lease_until: ts(row.lease_until),
            max_runtime_s: u32::try_from(row.max_runtime.into_inner().as_secs())
                .unwrap_or(u32::MAX),
            spec,
        })
    }
}

/// The result of [`sql::CLAIM_JOB`].
#[derive(Debug, Clone, SurrealValue)]
pub struct ClaimResultRow {
    /// The leased job, if any.
    pub job: Option<LeasedJobRow>,
    /// The scan moved from `queued` to `analysing`.
    pub started_analysing: bool,
}

/// What [`sql::LEASE_STATE`] reads.
#[derive(Debug, Clone, SurrealValue)]
pub struct LeaseStateRow {
    /// The job's status.
    pub status: JobStatusRow,
    /// Its token; `NONE` once requeued.
    pub lease_token: Option<String>,
    /// Its worker.
    pub worker: Option<String>,
    /// When the lease ends.
    pub lease_until: Option<DateTime<Utc>>,
    /// Its kind.
    pub kind: JobKindRow,
    /// Its scan.
    pub scan: RecordId,
    /// Its run.
    pub run: Option<i64>,
    /// Its question.
    pub question: Option<RecordId>,
    /// `payload.stages`: a perception job's stages, `NONE` for other kinds.
    pub stages: Option<serde_json::Value>,
}

/// One worker of [`sql::STATUS`].
#[derive(Debug, Clone, SurrealValue)]
pub struct WorkerStatusRow {
    /// `worker:[id]`'s key.
    pub worker_id: String,
    /// Its capabilities.
    pub caps: Vec<String>,
    /// Its last heartbeat.
    pub last_heartbeat: DateTime<Utc>,
    /// Its GPU's name.
    pub gpu: Option<String>,
}

/// Whether a job payload names `image` in any `image_id` (perception, VLM, verify and guided
/// payloads all list their images as `JobImage`s).
fn names_image(payload: &serde_json::Value, image: &str) -> bool {
    match payload {
        serde_json::Value::Object(fields) => fields
            .iter()
            .any(|(k, v)| (k == "image_id" && v.as_str() == Some(image)) || names_image(v, image)),
        serde_json::Value::Array(items) => items.iter().any(|v| names_image(v, image)),
        _ => false,
    }
}

/// A job's identity, for requeues and lease checks.
#[derive(Debug, Clone, SurrealValue)]
pub struct JobHeadRow {
    /// `job:<ulid>`.
    pub id: RecordId,
    /// Its kind.
    pub kind: JobKindRow,
    /// Its scan.
    pub scan: RecordId,
    /// Its run.
    pub run: Option<i64>,
    /// Its question.
    pub question: Option<RecordId>,
    /// Its status after the statement.
    pub status: JobStatusRow,
    /// `payload.stages`: a perception job's stages, `NONE` for other kinds.
    pub stages: Option<serde_json::Value>,
}

impl TryFrom<&JobHeadRow> for LeasedJob {
    type Error = RowError;

    fn try_from(row: &JobHeadRow) -> Result<Self, RowError> {
        Ok(Self {
            job: from_rid(&row.id)?,
            kind: row.kind.into(),
            scan: from_rid(&row.scan)?,
            run: row.run.map(|r| u32_of("run", r).map(Run)).transpose()?,
            question: from_opt_rid(row.question.as_ref())?,
            segmentation: runs_segmentation(row.kind, row.stages.as_ref())?,
        })
    }
}

/// Whether a job runs segmentation: a perception job whose `payload.stages` include it.
fn runs_segmentation(
    kind: JobKindRow,
    stages: Option<&serde_json::Value>,
) -> Result<bool, RowError> {
    match (JobKind::from(kind), stages) {
        (JobKind::Perception, Some(stages)) => {
            let stages: Vec<PerceptionStage> = from_json("payload.stages", stages.clone())?;
            Ok(stages.contains(&PerceptionStage::Segmentation))
        }
        _ => Ok(false),
    }
}

/// One `{ job, token }` of a heartbeat's `current_jobs`.
#[derive(Debug, Clone, SurrealValue)]
pub struct HeldJobRow {
    /// The job.
    pub job: RecordId,
    /// Its lease token.
    pub token: String,
}

/// A live worker's capabilities and versions.
#[derive(Debug, Clone, SurrealValue)]
pub struct LiveWorkerRow {
    /// `perception`, `vlm`, `fallback`.
    pub caps: Vec<String>,
    /// Model name → version, verbatim.
    pub versions: serde_json::Value,
}

#[async_trait::async_trait]
#[allow(
    clippy::too_many_lines,
    reason = "complete_job is one arm per outcome, each a bind list"
)]
impl JobRepo for SurrealStore {
    async fn enqueue_job(&self, job: NewJob) -> Result<JobId, StoreError> {
        const WHAT: &str = "enqueue job";
        let row = JobWriteRow::new(&job).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::ENQUEUE_JOB, |q| q.bind(("job", row.clone())))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        // After the commit: a waiting claim long poll wakes and finds the job.
        self.jobs_queued.notify_waiters();
        Ok(job.id)
    }

    async fn claim_job(
        &self,
        worker: &WorkerId,
        slots: u32,
        policy: ClaimPolicy,
        now: Timestamp,
    ) -> Result<Option<ClaimedJob>, StoreError> {
        const WHAT: &str = "claim job";
        let kinds: Vec<JobKindRow> = policy.kinds.iter().copied().map(Into::into).collect();
        let runtimes: BTreeMap<String, surrealdb::types::Duration> = JobKind::ALL
            .iter()
            .map(|k| {
                (
                    k.as_str().to_owned(),
                    k.max_runtime(policy.on_fallback).into(),
                )
            })
            .collect();
        let mut results = run_tx(&self.db, sql::CLAIM_JOB, |q| {
            q.bind(("worker", worker.as_str()))
                .bind(("slots", i64::from(slots)))
                .bind(("kinds", kinds.clone()))
                .bind(("eligible_before", dt(policy.eligible_before)))
                .bind(("hold_question", policy.hold_question_segmentation))
                .bind(("runtimes", runtimes.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<ClaimResultRow> = results
            .take(sql::CLAIM_JOB_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        let Some(ClaimResultRow {
            job: Some(job),
            started_analysing,
        }) = row
        else {
            return Ok(None);
        };
        Ok(Some(ClaimedJob {
            job: Job::try_from(job).map_err(|e| e.into_store(WHAT))?,
            started_analysing,
        }))
    }

    async fn heartbeat(
        &self,
        heartbeat: Heartbeat,
        now: Timestamp,
    ) -> Result<HeartbeatResponse, StoreError> {
        const WHAT: &str = "heartbeat";
        let caps: Vec<String> = heartbeat
            .caps
            .iter()
            .map(|c| crate::repo::analysis::enum_str("caps", c))
            .collect::<Result<_, _>>()
            .map_err(|e| e.into_store(WHAT))?;
        let gpu = heartbeat
            .gpu
            .as_ref()
            .map(|g| to_json("gpu", g))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let versions = to_json("versions", &heartbeat.versions).map_err(|e| e.into_store(WHAT))?;
        let held: Vec<HeldJobRow> = heartbeat
            .current_jobs
            .iter()
            .map(|h| HeldJobRow {
                job: rid(&h.job_id),
                token: h.lease_token.to_string(),
            })
            .collect();
        let mut results = run_tx(&self.db, sql::HEARTBEAT, |q| {
            q.bind(("worker", worker_rid(&heartbeat.worker_id)))
                .bind(("worker_id", heartbeat.worker_id.as_str()))
                .bind(("caps", caps.clone()))
                .bind(("slots", i64::from(heartbeat.slots)))
                .bind(("gpu", gpu.clone()))
                .bind(("versions", versions.clone()))
                .bind(("held", held.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let cancel: Vec<RecordId> = results
            .take(sql::HEARTBEAT_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        Ok(HeartbeatResponse {
            cancel_job_ids: cancel
                .iter()
                .map(from_rid)
                .collect::<Result<_, _>>()
                .map_err(|e: RowError| e.into_store(WHAT))?,
        })
    }

    async fn check_lease(&self, lease: &Lease, now: Timestamp) -> Result<LeasedJob, StoreError> {
        const WHAT: &str = "check lease";
        let mut results = run_tx(&self.db, sql::CHECK_LEASE, |q| {
            q.bind(("job", rid(&lease.job)))
                .bind(("lease_token", lease.token.as_str()))
                .bind(("worker", lease.worker.as_str()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<JobHeadRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.first()
            .ok_or(StoreError::Conflict(domain::ports::Conflict::LeaseLost {
                job_id: lease.job,
            }))?
            .try_into()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn live_lease(
        &self,
        lease: &Lease,
        now: Timestamp,
    ) -> Result<Option<LeasedJob>, StoreError> {
        const WHAT: &str = "live lease";
        let mut results = run_tx(&self.db, sql::CHECK_LEASE, |q| {
            q.bind(("job", rid(&lease.job)))
                .bind(("lease_token", lease.token.as_str()))
                .bind(("worker", lease.worker.as_str()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<JobHeadRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        match rows.first() {
            // A late event after `complete` by the same lease: dropped (`204`).
            Some(row) if row.status != JobStatusRow::Leased => Ok(None),
            Some(row) => LeasedJob::try_from(row)
                .map(Some)
                .map_err(|e| e.into_store(WHAT)),
            None => Err(StoreError::Conflict(Conflict::LeaseLost {
                job_id: lease.job,
            })),
        }
    }

    async fn job_image(
        &self,
        lease: &Lease,
        image: ImageId,
        now: Timestamp,
    ) -> Result<Option<ObjectRef>, StoreError> {
        const WHAT: &str = "job image";
        let result = run_tx(&self.db, sql::JOB_IMAGE, |q| {
            q.bind(("job", rid(&lease.job)))
                .bind(("lease_token", lease.token.as_str()))
                .bind(("worker", lease.worker.as_str()))
                .bind(("image", rid(&image)))
                .bind(("now", dt(now)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            Err(f) if f.thrown() == Some(thrown::LEASE_LOST) => {
                return Err(StoreError::Conflict(Conflict::LeaseLost {
                    job_id: lease.job,
                }));
            }
            Err(f) => return Err(f.into_store(WHAT)),
        };
        let payload: Option<serde_json::Value> =
            results.take(2).map_err(|e| take_failed(WHAT, e))?;
        // Only an image the leased job's payload names is served (contracts 5, `JobImage.url`).
        if !payload.is_some_and(|p| names_image(&p, &image.to_string())) {
            return Ok(None);
        }
        let keys: Vec<String> = results.take(3).map_err(|e| take_failed(WHAT, e))?;
        Ok(keys
            .first()
            .and_then(|k| ObjectKey::from_stored(k))
            .map(|key| ObjectRef {
                key,
                content_type: ContentType::Webp,
            }))
    }

    async fn complete_job(
        &self,
        lease: &Lease,
        completion: JobCompletion,
        now: Timestamp,
    ) -> Result<CompletionOutcome, StoreError> {
        const WHAT: &str = "complete job";
        let lease_lost = || StoreError::Conflict(Conflict::LeaseLost { job_id: lease.job });
        let mut state = run_tx(&self.db, sql::LEASE_STATE, |q| {
            q.bind(("job", rid(&lease.job)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let state: Option<LeaseStateRow> = state.take(0).map_err(|e| take_failed(WHAT, e))?;
        let state = state.ok_or_else(lease_lost)?;
        let same_lease = state.lease_token.as_deref() == Some(lease.token.as_str())
            && state.worker.as_deref() == Some(lease.worker.as_str());
        match state.status {
            // A retry after a network timeout: 204, nothing changes (contracts 5).
            JobStatusRow::Done | JobStatusRow::Failed if same_lease => {
                return Ok(CompletionOutcome::AlreadyCompleted);
            }
            JobStatusRow::Leased if same_lease => {}
            _ => return Err(lease_lost()),
        }
        let head = LeasedJob::try_from(&JobHeadRow {
            id: rid(&lease.job),
            kind: state.kind,
            scan: state.scan.clone(),
            run: state.run,
            question: state.question.clone(),
            status: state.status,
            stages: state.stages.clone(),
        })
        .map_err(|e| e.into_store(WHAT))?;
        let map_lost = |f: DbFailure| {
            if f.thrown() == Some(thrown::LEASE_LOST) {
                lease_lost()
            } else {
                f.into_store(WHAT)
            }
        };
        match completion {
            JobCompletion::Done {
                result,
                observations,
                evidence,
                claim,
            } => {
                let mut before = Vec::new();
                for write in &observations {
                    before.extend(self.replaced_observations(head.scan, write).await?);
                }
                let writes = observations
                    .iter()
                    .map(|w| ObservationWriteRow::new(head.scan, w))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.into_store(WHAT))?;
                let run = head.run.unwrap_or(Run::NONE);
                let answers =
                    crate::repo::analysis::answer_rows(head.scan, run, &evidence, None, now)
                        .map_err(|e| e.into_store(WHAT))?;
                let statement = if claim.is_some() {
                    sql::COMPLETE_DONE_CLAIM
                } else {
                    sql::COMPLETE_DONE
                };
                let mut results = run_tx(&self.db, statement, |q| {
                    q.bind(("job", rid(&lease.job)))
                        .bind(("lease_token", lease.token.as_str()))
                        .bind(("worker", lease.worker.as_str()))
                        .bind(("writes", writes.clone()))
                        .bind(("answers", answers.clone()))
                        .bind(("result", result.clone()))
                        .bind(("claim_step", claim.map(PipelineStepRow::from)))
                        .bind(("now", dt(now)))
                })
                .await
                .map_err(map_lost)?;
                let last = results.num_statements().saturating_sub(2);
                let claim_row: Option<ClaimRow> =
                    results.take(last).map_err(|e| take_failed(WHAT, e))?;
                let claim_row = claim_row.ok_or_else(lease_lost)?;
                let claim = match claim {
                    Some(step) => claim_attempt(&claim_row, head.scan, step, now)
                        .map_err(|e| e.into_store(WHAT))?,
                    None => ClaimAttempt::NotNeeded,
                };
                Ok(CompletionOutcome::Stored {
                    claim,
                    diff: observation_diff(before, &observations),
                })
            }
            JobCompletion::Failed { error } => {
                let error_json = to_json("error", &error).map_err(|e| e.into_store(WHAT))?;
                let mut results = run_tx(&self.db, sql::COMPLETE_FAILED, |q| {
                    q.bind(("job", rid(&lease.job)))
                        .bind(("lease_token", lease.token.as_str()))
                        .bind(("worker", lease.worker.as_str()))
                        .bind(("retryable", error.retryable))
                        .bind(("error", error_json.clone()))
                        .bind(("max_attempts", i64::from(MAX_ATTEMPTS)))
                        .bind(("now", dt(now)))
                })
                .await
                .map_err(map_lost)?;
                let last = results.num_statements().saturating_sub(2);
                let failed_for_good: Option<bool> =
                    results.take(last).map_err(|e| take_failed(WHAT, e))?;
                if failed_for_good.unwrap_or(true) {
                    Ok(CompletionOutcome::FailedForGood { job: head })
                } else {
                    self.jobs_queued.notify_waiters();
                    Ok(CompletionOutcome::Requeued)
                }
            }
        }
    }

    async fn requeue_stale(&self, now: Timestamp) -> Result<Vec<RequeuedJob>, StoreError> {
        const WHAT: &str = "requeue stale jobs";
        let mut results = run_tx(&self.db, sql::REQUEUE_STALE, |q| {
            q.bind(("now", dt(now)))
                .bind(("free_requeues", i64::from(FREE_REQUEUES)))
                .bind(("max_attempts", i64::from(MAX_ATTEMPTS)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<JobHeadRow> = results
            .take(sql::REQUEUE_STALE_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        rows.iter()
            .map(|row| {
                Ok(RequeuedJob {
                    job: LeasedJob::try_from(row)?,
                    exhausted: row.status == JobStatusRow::Failed,
                })
            })
            .collect::<Result<Vec<_>, RowError>>()
            .map_err(|e| e.into_store(WHAT))
            .inspect(|requeued| {
                // Requeued jobs are claimable again: wake the claim long polls (the store's
                // `Notify` is the only wake-up besides the 1-second fallback poll).
                if requeued.iter().any(|r| !r.exhausted) {
                    self.jobs_queued.notify_waiters();
                }
            })
    }

    async fn live_workers(&self, now: Timestamp) -> Result<Vec<LiveWorker>, StoreError> {
        const WHAT: &str = "live workers";
        let mut results = run_tx(&self.db, sql::LIVE_WORKERS, |q| q.bind(("now", dt(now))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<LiveWorkerRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok(LiveWorker {
                    caps: row
                        .caps
                        .into_iter()
                        .map(|c| from_json::<Capability>("caps", serde_json::Value::String(c)))
                        .collect::<Result<_, _>>()?,
                    versions: from_json::<BTreeMap<String, String>>("versions", row.versions)?,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn perception_alive(&self, now: Timestamp) -> Result<bool, StoreError> {
        const WHAT: &str = "perception alive";
        let mut results = run_tx(&self.db, sql::PERCEPTION_ALIVE, |q| {
            q.bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let alive: Option<bool> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(alive.unwrap_or(false))
    }

    async fn status(&self, now: Timestamp) -> Result<Status, StoreError> {
        const WHAT: &str = "status";
        let mut results = run_tx(&self.db, sql::STATUS, |q| q)
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let workers: Vec<WorkerStatusRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        let queued: Option<i64> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        let leased: Option<i64> = results.take(2).map_err(|e| take_failed(WHAT, e))?;
        let stale = now.minus(WORKER_STALE_AFTER);
        let workers = workers
            .into_iter()
            .map(|w| {
                Ok(WorkerStatus {
                    worker_id: w.worker_id.parse().map_err(|_| RowError::BadField {
                        field: "worker_id",
                        problem: w.worker_id.clone(),
                    })?,
                    caps: w
                        .caps
                        .into_iter()
                        .map(|c| from_json::<Capability>("caps", serde_json::Value::String(c)))
                        .collect::<Result<_, _>>()?,
                    healthy: ts(w.last_heartbeat) > stale,
                    last_heartbeat: ts(w.last_heartbeat),
                    gpu: w.gpu,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))?;
        Ok(Status {
            workers,
            queue: QueueCounts {
                queued: u32::try_from(queued.unwrap_or(0)).unwrap_or(0),
                leased: u32::try_from(leased.unwrap_or(0)).unwrap_or(0),
            },
        })
    }

    async fn delete_finished_jobs(&self, before: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "delete finished jobs";
        let mut results = run_tx(&self.db, sql::DELETE_FINISHED_JOBS, |q| {
            q.bind(("cutoff", dt(before)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
    }
}
