//! Observations, questions, guided checks and indicator answers ([contracts 3], [contracts 7];
//! [database 5], replacement per image; schema v1.4 `indicator_answer`).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Editor, Membership, Viewer},
    chat::SimilarCase,
    error::{DomainError, InvalidStateDetails},
    events::Stage,
    id::{ImageId, ObservationId, QuestionId, ScanId},
    ports::{
        AnalysisRepo, AnswerOutcome, ClaimAttempt, Conflict, DueQuestion, GuidedFinish,
        GuidedOutcome, Lease, NewMask, ObservationDiff, ObservationWrite, QuestionAdvance,
        QuestionStepClaim, QuestionStepOutput, StepClaim, StoreError,
    },
    records::{
        diagnosis::{EvidenceSource, IndicatorEvidence},
        observation::{KnowledgeRef, Observation, ObservationStage},
    },
    rules::{questions::QuestionStep, steps::PipelineStep},
    scan::AnswerRequest,
    url::Stored,
    wire::{Run, Timestamp},
};
use serde::Serialize;
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown},
    ids::{from_rid, indicator_answer_rid, rid},
    read::take_all,
    repo::{
        jobs::JobWriteRow,
        steps::{ClaimRow, claim_attempt},
    },
    rows::{EvidenceSourceRow, QuestionStepRow, ScanStatusRow, dt, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    use crate::fragments::{
        claim_check, claim_entry, evidence_write, job_enqueue, observation_fields,
        observation_write, question_step_check,
    };

    /// `measuring` after a segmentation result, under the question's step claim.
    pub const QUESTION_TO_MEASURING: &str = concat!(
        "BEGIN;\n",
        question_step_check!(),
        "UPDATE $question SET step = 'measuring', step_at = $now;\n",
        "COMMIT;"
    );

    /// Measured: store the photo's measurements and the indicators they settled, queue its
    /// `verify` job and move to `verifying`.
    pub const QUESTION_TO_VERIFYING: &str = concat!(
        "BEGIN;\n",
        question_step_check!(),
        "LET $scan = $q.scan;\n",
        observation_write!(),
        evidence_write!(),
        job_enqueue!(),
        "UPDATE $question SET step = 'verifying', step_at = $now;\n",
        "COMMIT;"
    );

    /// Measured and nothing left to ask: store them, clear the step (the question stays
    /// `answering` until `score` resolves it) and claim the scan's `score` step, all in one
    /// transaction. Returns the claim outcome last.
    pub const QUESTION_TO_SCORE: &str = concat!(
        "BEGIN;\n",
        question_step_check!(),
        "LET $scan = $q.scan;\n",
        observation_write!(),
        evidence_write!(),
        "UPDATE $question SET step = NONE, step_at = NONE;\n",
        claim_entry!(),
        "COMMIT;"
    );

    /// Photo answers stuck in `measuring` (index `question_stalled` ✓).
    pub const STALLED_QUESTION_STEPS: &str = "SELECT id, scan, step, step_at, run FROM question
WHERE step = 'measuring' AND step_at < $older_than AND status = 'answering';";

    /// The sweep's re-claim by the old stamp and run; `NONE` back means someone else has it.
    pub const RECLAIM_QUESTION_STEP: &str = "UPDATE ONLY $question SET step = $step, step_at = $now
WHERE status = 'answering' AND step = $seen_step AND step_at = $seen AND run = scan.run
  AND scan.plant.deleting_at = NONE AND scan.garden.deleting_at = NONE
RETURN AFTER;";

    /// `find_similar_cases` ([database 5] ✓: exact search within one garden, no HNSW index).
    /// Statements: 0 `LET $img`, 1 `IF` (not the garden's → `not_found`), 2 the search.
    pub const SIMILAR_CASES: &str = "LET $img = (SELECT scan, garden, embedding FROM ONLY $image);
IF $img = NONE OR $img.garden != $garden { THROW 'not_found' };
SELECT scan, scan.created_at AS created_at, scan.diagnosis.primary_problem AS primary_problem,
       scan.diagnosis.primary.name AS primary_name,
       vector::similarity::cosine(embedding, $img.embedding) AS similarity,
       (SELECT VALUE id FROM scan_image WHERE scan = $parent.scan AND kind = 'whole_plant' LIMIT 1)[0] AS thumbnail
FROM scan_image
WHERE garden = $garden AND scan != $img.scan AND embedding != NONE AND $img.embedding != NONE
  AND scan.deleting_at = NONE AND scan.plant.deleting_at = NONE
ORDER BY similarity DESC LIMIT $k;";

    /// A worker's `partial_result`, under its lease: the stage's observations, stored once
    /// (replacing per image, so a repeat changes nothing).
    pub const STORE_PARTIAL: &str = concat!(
        "BEGIN;\n",
        "LET $j = (SELECT status, lease_token, worker, lease_until, scan FROM ONLY $job);\n",
        "IF $j = NONE OR $j.status != 'leased' OR $j.lease_token != $lease_token OR $j.worker != $worker { THROW 'lease_lost' };\n",
        "LET $scan = $j.scan;\n",
        observation_write!(),
        "COMMIT;"
    );

    /// `api`'s own observations under its step claim (no hand-over: the step continues).
    pub const STORE_OBSERVATIONS: &str =
        concat!("BEGIN;\n", claim_check!(), observation_write!(), "COMMIT;");

    /// A job's scan, for the diff read before a partial result.
    pub const JOB_SCAN: &str = "SELECT VALUE scan FROM ONLY $job;";

    /// The observation ids a write would replace, read before it for the diff.
    pub const REPLACED_IDS: &str =
        "SELECT VALUE id FROM observation WHERE scan = $scan AND pipeline_stage = $stage
  AND (image IN $images OR ($imageless AND image = NONE));";

    /// The scan's current observations ([contracts 3], observations across runs), as contract
    /// shapes.
    pub const OBSERVATIONS: &str = concat!(
        "SELECT ",
        observation_fields!(),
        " FROM observation WHERE scan = $scan AND retracted_at = NONE ORDER BY created_at;"
    );

    /// What `answer_question` reads first: the question's run, scan and status, and the scan's
    /// status and garden.
    pub const QUESTION_STATE: &str = "SELECT run, scan, status, scan.status AS scan_status, scan.garden AS garden FROM ONLY $question;";

    /// `POST /scans/{id}/answers`: the user's evidence (rank 3, replacing a measured one),
    /// the question answered, and the `score` claim (or `rescore_pending`), in one transaction.
    pub const ANSWER_QUESTION: &str = concat!(
        "BEGIN;\n",
        "LET $qs = (SELECT status, run, scan, scan.garden AS garden FROM ONLY $question);\n",
        "IF $qs = NONE OR $qs.scan != $scan OR $qs.garden != $garden { THROW 'not_found' };\n",
        "IF $qs.status != 'open' { THROW 'invalid_state' };\n",
        "LET $run = $qs.run;\n",
        "LET $measured_images = [];\n",
        evidence_write!(),
        "UPDATE $question SET status = 'answered', answer = $answer, resolved_by = 'user', resolved_at = $now;\n",
        "UPDATE $scan SET open_question = NONE WHERE open_question = $question;\n",
        "LET $step = 'score';\n",
        claim_entry!(),
        "COMMIT;"
    );

    /// A photo answer that couldn't be analysed: `dropped` (and off the scan) when late,
    /// reopened otherwise.
    pub const DROP_PHOTO_ANSWER: &str = concat!(
        "BEGIN;\n",
        question_step_check!(),
        "LET $late = (SELECT VALUE status FROM ONLY $q.scan) IN ['diagnosed', 'planned'];\n",
        "UPDATE $question SET step = NONE, step_at = NONE,\n",
        "  status = IF $late { 'dropped' } ELSE { 'open' }, resolved_by = IF $late { 'dropped' } ELSE { NONE },\n",
        "  resolved_at = IF $late { $now } ELSE { NONE };\n",
        "IF $late { UPDATE $q.scan SET open_question = NONE WHERE open_question = $question; };\n",
        "COMMIT;"
    );

    /// Open questions past `continue_at` (index `question_sweep` ✓) not yet continued; a
    /// question whose photo is being analysed is `answering`, so it isn't `open`.
    pub const QUESTIONS_DUE: &str = "SELECT id, scan, run FROM question WHERE status = 'open' AND continue_at < $now AND continued_at = NONE;";

    /// The scheduler continues without an answer, and claims `score`.
    pub const CONTINUE_WITHOUT_ANSWER: &str = concat!(
        "BEGIN;\n",
        "UPDATE $question SET continued_at = $now WHERE status = 'open';\n",
        "LET $step = 'score';\n",
        claim_entry!(),
        "COMMIT;"
    );

    /// A `guided` job's result, under its lease: the texture observation, the check found from
    /// the job's payload image, the job `done`, and the `score` claim only on a `diagnosed` or
    /// `planned` scan ([contracts 7], guided checks and the diagnosis). The port passes no time,
    /// so this one transaction reads the database clock (`time::now()`), and the claim's stamp
    /// is returned with it.
    pub const FINISH_GUIDED_CHECK: &str = concat!(
        "BEGIN;\n",
        "LET $now = time::now();\n",
        "LET $j = (SELECT status, lease_token, worker, lease_until, scan, payload FROM ONLY $job);\n",
        "IF $j = NONE OR $j.status != 'leased' OR $j.lease_token != $lease_token OR $j.worker != $worker OR $j.lease_until < $now { THROW 'lease_lost' };\n",
        "IF $observation != NONE { CREATE $observation.id CONTENT $observation.row; };\n",
        "UPDATE guided_check SET status = $status, result = $observation.id\n",
        "  WHERE scan = $j.scan AND image = type::record('scan_image', $j.payload.image.image_id) AND status = 'analysing';\n",
        "UPDATE $job SET status = 'done', finished_at = $now;\n",
        "LET $scan = $j.scan;\n",
        "LET $step = 'score';\n",
        "LET $only_statuses = ['diagnosed', 'planned'];\n",
        claim_entry!(),
        "COMMIT;"
    );
}

/// A string literal-union value from its serde form (`ObservationStage::PlantCheck` →
/// `"plant_check"`), for the many enums whose row needs only the string.
pub fn enum_str<T: Serialize>(field: &'static str, value: &T) -> Result<String, RowError> {
    match to_json(field, value)? {
        serde_json::Value::String(s) => Ok(s),
        other => Err(RowError::BadField {
            field,
            problem: format!("expected a string, found {other}"),
        }),
    }
}

/// An `observation` row as written ([database 3]; measurement fields from contracts 7).
#[derive(Debug, Clone, SurrealValue)]
pub struct ObservationRow {
    /// Its scan.
    pub scan: RecordId,
    /// The run that produced it.
    pub run: i64,
    /// `plant_check` … `measurement`.
    pub stage: String,
    /// The pipeline stage; runs replace observations by it, per image.
    pub pipeline_stage: String,
    /// The image it came from; replacement is per image.
    pub image: Option<RecordId>,
    /// Display text.
    pub label: String,
    /// The knowledge-base record (`record<species | disease | organism>`).
    #[surreal(rename = "ref")]
    pub knowledge_ref: Option<RecordId>,
    /// Organisms only.
    pub role: Option<String>,
    /// `ObservationDetail`.
    pub detail: Option<serde_json::Value>,
    /// 0–1.
    pub confidence: f64,
    /// `Region<Stored>`, with `region_id` and `mask_id`.
    pub region: Option<serde_json::Value>,
    /// Who produced it.
    pub source: String,
    /// Measurement observations: the quantity, top-level for the growth series.
    pub quantity: Option<String>,
    /// Measurement observations: the value.
    pub measured_value: Option<f64>,
    /// The model or algorithm version.
    pub model_version: String,
    /// When it was made.
    pub created_at: DateTime<Utc>,
}

impl TryFrom<&Observation<Stored>> for ObservationRow {
    type Error = RowError;

    fn try_from(o: &Observation<Stored>) -> Result<Self, RowError> {
        let measurement = o.detail.as_ref().and_then(|d| d.measurement.as_ref());
        Ok(Self {
            scan: rid(&o.scan_id),
            run: i64::from(o.run.0),
            stage: enum_str("stage", &o.stage)?,
            pipeline_stage: enum_str("pipeline_stage", &o.pipeline_stage)?,
            image: o.region.as_ref().map(|r| rid(&r.image_id)),
            label: o.label.clone(),
            knowledge_ref: o.knowledge_ref.as_ref().map(|r| match r {
                KnowledgeRef::Species(id) => rid(id),
                KnowledgeRef::Disease(id) => rid(id),
                KnowledgeRef::Organism(id) => rid(id),
            }),
            role: o.role.as_ref().map(|r| enum_str("role", r)).transpose()?,
            detail: o
                .detail
                .as_ref()
                .map(|d| to_json("detail", d))
                .transpose()?,
            confidence: o.confidence.confidence().get(),
            region: o
                .region
                .as_ref()
                .map(|r| to_json("region", r))
                .transpose()?,
            source: enum_str("source", &o.source)?,
            quantity: match (o.stage, measurement) {
                (ObservationStage::Measurement, Some(m)) => {
                    Some(enum_str("quantity", &m.quantity)?)
                }
                _ => None,
            },
            measured_value: measurement.map(|m| m.value),
            model_version: o.model_version.clone(),
            created_at: dt(o.created_at),
        })
    }
}

/// An observation with its record id, as [`observation_write!`] creates it.
#[derive(Debug, Clone, SurrealValue)]
pub struct ObservationEntry {
    /// `observation:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: ObservationRow,
}

/// A `mask` row (`rle` added in schema v1.4, kept for measurement).
#[derive(Debug, Clone, SurrealValue)]
pub struct MaskRow {
    /// Its scan.
    pub scan: RecordId,
    /// The image.
    pub image: RecordId,
    /// The region it outlines.
    pub region_id: String,
    /// Its storage key.
    pub storage_key: String,
    /// The worker's `RleMask`.
    pub rle: serde_json::Value,
}

/// A mask with its record id.
#[derive(Debug, Clone, SurrealValue)]
pub struct MaskEntry {
    /// `mask:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: MaskRow,
}

/// `$obs` of [`observation_write!`].
#[derive(Debug, Clone, SurrealValue)]
pub struct ObservationWriteRow {
    /// The pipeline stage whose observations are replaced.
    pub stage: String,
    /// The images covered.
    pub images: Vec<RecordId>,
    /// Also replace the stage's observations tied to no image.
    pub replace_imageless: bool,
    /// The new observations.
    pub rows: Vec<ObservationEntry>,
    /// Masks to record with them.
    pub masks: Vec<MaskEntry>,
}

impl ObservationWriteRow {
    /// The row of a domain write for `scan`.
    pub fn new(scan: ScanId, write: &ObservationWrite) -> Result<Self, RowError> {
        Ok(Self {
            stage: enum_str("stage", &write.stage)?,
            images: write.images.iter().map(rid).collect(),
            replace_imageless: write.replace_imageless,
            rows: write
                .observations
                .iter()
                .map(|o| {
                    Ok(ObservationEntry {
                        id: rid(&o.id),
                        row: ObservationRow::try_from(o)?,
                    })
                })
                .collect::<Result<_, RowError>>()?,
            masks: write
                .masks
                .iter()
                .map(|m| mask_entry(scan, m))
                .collect::<Result<_, RowError>>()?,
        })
    }

    /// The images whose measured answers this write replaces: the covered images of a
    /// measurement write ([contracts 7]: re-measuring one photo replaces that photo's
    /// measurements only).
    pub fn measured_images(write: Option<&ObservationWrite>) -> Vec<RecordId> {
        match write {
            Some(w) if w.stage == Stage::Measurement => w.images.iter().map(rid).collect(),
            _ => Vec::new(),
        }
    }
}

fn mask_entry(scan: ScanId, m: &NewMask) -> Result<MaskEntry, RowError> {
    Ok(MaskEntry {
        id: rid(&m.id),
        row: MaskRow {
            scan: rid(&scan),
            image: rid(&m.image),
            region_id: m.region_id.to_string(),
            storage_key: m.storage_key.as_str().to_owned(),
            rle: to_json("rle", &m.rle)?,
        },
    })
}

/// An `indicator_answer` row (schema v1.4).
#[derive(Debug, Clone, SurrealValue)]
pub struct AnswerRow {
    /// The scan (repeated from the key, with a reference).
    pub scan: RecordId,
    /// The run.
    pub run: i64,
    /// `user`, `measurement` or `vlm`.
    pub source: EvidenceSourceRow,
    /// The `IndicatorEvidence`.
    pub evidence: serde_json::Value,
    /// Measured answers: the photo measured.
    pub image: Option<RecordId>,
    /// User answers: the question answered.
    pub question: Option<RecordId>,
    /// When.
    pub answered_at: DateTime<Utc>,
}

/// One entry of `$answers` in [`evidence_write!`]: the key, the new answer's rank (user 3,
/// measurement 2, vlm 1, the same rule as the stored `rank` field), and the row.
#[derive(Debug, Clone, SurrealValue)]
pub struct AnswerWriteRow {
    /// `indicator_answer:[scan, run, indicator_id]`.
    pub id: RecordId,
    /// The new answer's rank.
    pub rank: i64,
    /// The row.
    pub row: AnswerRow,
}

/// The rank of an evidence source (contracts 7: user, then measurement, then vlm).
pub const fn rank(source: EvidenceSource) -> i64 {
    match source {
        EvidenceSource::User => 3,
        EvidenceSource::Measurement => 2,
        EvidenceSource::Vlm => 1,
    }
}

/// `$answers` for one run's evidence.
pub fn answer_rows(
    scan: ScanId,
    run: Run,
    evidence: &[IndicatorEvidence],
    question: Option<QuestionId>,
    now: Timestamp,
) -> Result<Vec<AnswerWriteRow>, RowError> {
    evidence
        .iter()
        .map(|e| {
            Ok(AnswerWriteRow {
                id: indicator_answer_rid(scan, run, &e.indicator_id),
                rank: rank(e.source),
                row: AnswerRow {
                    scan: rid(&scan),
                    run: i64::from(run.0),
                    source: e.source.into(),
                    evidence: to_json("evidence", e)?,
                    image: e.measured.as_ref().map(|m| rid(&m.image_id)),
                    question: question
                        .filter(|_| e.source == EvidenceSource::User)
                        .map(|q| rid(&q)),
                    answered_at: dt(now),
                },
            })
        })
        .collect()
}

/// A question step as [`sql::STALLED_QUESTION_STEPS`] and [`sql::RECLAIM_QUESTION_STEP`] read
/// it.
#[derive(Debug, Clone, SurrealValue)]
pub struct QuestionClaimRow {
    /// The question.
    pub id: RecordId,
    /// Its scan.
    pub scan: RecordId,
    /// The step.
    pub step: Option<QuestionStepRow>,
    /// The stamp.
    pub step_at: Option<DateTime<Utc>>,
    /// Its run.
    pub run: i64,
}

impl TryFrom<QuestionClaimRow> for QuestionStepClaim {
    type Error = RowError;

    fn try_from(row: QuestionClaimRow) -> Result<Self, RowError> {
        let missing = |field| RowError::BadField {
            field,
            problem: "missing on a claimed step".to_owned(),
        };
        Ok(Self {
            question: from_rid(&row.id)?,
            scan: from_rid(&row.scan)?,
            step: row.step.ok_or_else(|| missing("step"))?.into(),
            run: Run(u32_of("run", row.run)?),
            stamp: ts(row.step_at.ok_or_else(|| missing("step_at"))?),
        })
    }
}

/// What [`sql::QUESTION_STATE`] reads.
#[derive(Debug, Clone, SurrealValue)]
pub struct QuestionStateRow {
    /// The question's run.
    pub run: i64,
    /// Its scan.
    pub scan: RecordId,
    /// The scan's status.
    pub scan_status: ScanStatusRow,
    /// The scan's garden.
    pub garden: RecordId,
}

/// One row of [`sql::QUESTIONS_DUE`].
#[derive(Debug, Clone, SurrealValue)]
pub struct DueRow {
    /// The question.
    pub id: RecordId,
    /// Its scan.
    pub scan: RecordId,
    /// Its run.
    pub run: i64,
}

/// A lost lease as `LeaseLost`, anything else by the default mapping.
fn lease_failure(f: DbFailure, lease: &Lease, what: &'static str) -> StoreError {
    if f.thrown() == Some(crate::error::thrown::LEASE_LOST) {
        StoreError::Conflict(Conflict::LeaseLost { job_id: lease.job })
    } else {
        f.into_store(what)
    }
}

/// What a write changed against what it replaced ([contracts 5], partial results): new
/// observation ids are `added`, replaced ids the write didn't keep are `retracted`.
pub fn observation_diff(
    before: Vec<ObservationId>,
    writes: &[ObservationWrite],
) -> ObservationDiff {
    let new: std::collections::HashSet<ObservationId> = writes
        .iter()
        .flat_map(|w| w.observations.iter().map(|o| o.id))
        .collect();
    let old: std::collections::HashSet<ObservationId> = before.iter().copied().collect();
    ObservationDiff {
        added: writes
            .iter()
            .flat_map(|w| w.observations.iter())
            .filter(|o| !old.contains(&o.id))
            .cloned()
            .collect(),
        retracted: before.into_iter().filter(|id| !new.contains(id)).collect(),
    }
}

impl SurrealStore {
    /// The observation ids `write` will replace, read before it ([`sql::REPLACED_IDS`]).
    pub(crate) async fn replaced_observations(
        &self,
        scan: ScanId,
        write: &ObservationWrite,
    ) -> Result<Vec<ObservationId>, StoreError> {
        const WHAT: &str = "replaced observations";
        let stage = enum_str("stage", &write.stage).map_err(|e| e.into_store(WHAT))?;
        let images: Vec<RecordId> = write.images.iter().map(rid).collect();
        let mut r = run_tx(&self.db, sql::REPLACED_IDS, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("stage", stage.clone()))
                .bind(("images", images.clone()))
                .bind(("imageless", write.replace_imageless))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        ids.iter()
            .map(from_rid)
            .collect::<Result<_, _>>()
            .map_err(|e: RowError| e.into_store(WHAT))
    }
}

/// One row of [`sql::SIMILAR_CASES`].
#[derive(Debug, Clone, SurrealValue)]
pub struct SimilarRow {
    /// The other scan.
    pub scan: RecordId,
    /// Its creation time.
    pub created_at: DateTime<Utc>,
    /// Its diagnosis's primary problem.
    pub primary_problem: Option<RecordId>,
    /// That problem's name.
    pub primary_name: Option<String>,
    /// Cosine similarity.
    pub similarity: f64,
    /// Its `whole_plant` shot.
    pub thumbnail: Option<RecordId>,
}

impl SurrealStore {
    /// Sends one of the photo-answer step transactions with the binds they all share.
    async fn question_tx(
        &self,
        sql: &'static str,
        claim: &QuestionStepClaim,
        write: Option<&ObservationWrite>,
        evidence: &[IndicatorEvidence],
        job: Option<JobWriteRow>,
        now: Timestamp,
    ) -> Result<surrealdb::IndexedResults, StoreError> {
        const WHAT: &str = "advance question step";
        let obs = write
            .map(|w| ObservationWriteRow::new(claim.scan, w))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let answers = answer_rows(claim.scan, claim.run, evidence, None, now)
            .map_err(|e| e.into_store(WHAT))?;
        let measured = ObservationWriteRow::measured_images(write);
        run_tx(&self.db, sql, |q| {
            q.bind(("question", rid(&claim.question)))
                .bind(("step", QuestionStepRow::from(claim.step)))
                .bind(("stamp", dt(claim.stamp)))
                .bind(("run", i64::from(claim.run.0)))
                .bind(("obs", obs.clone()))
                .bind(("answers", answers.clone()))
                .bind(("measured_images", measured.clone()))
                .bind(("job", job.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))
    }
}

#[async_trait::async_trait]
impl AnalysisRepo for SurrealStore {
    /// Runs [`sql::STORE_PARTIAL`]. The port passes no time, so the lease is checked by token,
    /// worker and status; a requeue clears the token, so an expired lease that was requeued
    /// fails, and `complete` checks expiry.
    async fn store_partial(
        &self,
        lease: &Lease,
        write: ObservationWrite,
    ) -> Result<ObservationDiff, StoreError> {
        const WHAT: &str = "store partial result";
        let mut r = run_tx(&self.db, sql::JOB_SCAN, |q| {
            q.bind(("job", rid(&lease.job)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let scan: Option<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let scan: ScanId = from_rid(&scan.ok_or(StoreError::Conflict(Conflict::LeaseLost {
            job_id: lease.job,
        }))?)
        .map_err(|e| e.into_store(WHAT))?;
        let before = self.replaced_observations(scan, &write).await?;
        let obs = ObservationWriteRow::new(scan, &write).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::STORE_PARTIAL, |q| {
            q.bind(("job", rid(&lease.job)))
                .bind(("lease_token", lease.token.as_str()))
                .bind(("worker", lease.worker.as_str()))
                .bind(("obs", obs.clone()))
        })
        .await
        .map_err(|f| lease_failure(f, lease, WHAT))?;
        Ok(observation_diff(before, std::slice::from_ref(&write)))
    }

    async fn store_observations(
        &self,
        claim: &StepClaim,
        write: ObservationWrite,
    ) -> Result<ObservationDiff, StoreError> {
        const WHAT: &str = "store observations";
        let before = self.replaced_observations(claim.scan, &write).await?;
        let obs = ObservationWriteRow::new(claim.scan, &write).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::STORE_OBSERVATIONS, |q| {
            crate::repo::steps::bind_claim(q, claim).bind(("obs", obs.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        Ok(observation_diff(before, std::slice::from_ref(&write)))
    }

    async fn observations(&self, scan: ScanId) -> Result<Vec<Observation<Stored>>, StoreError> {
        const WHAT: &str = "observations";
        let mut r = run_tx(&self.db, sql::OBSERVATIONS, |q| {
            q.bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_all(&mut r, 0, WHAT)
    }

    async fn answer_question(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        answer: AnswerRequest,
        evidence: Vec<IndicatorEvidence>,
        now: Timestamp,
    ) -> Result<AnswerOutcome, StoreError> {
        const WHAT: &str = "answer question";
        let mut r = run_tx(&self.db, sql::QUESTION_STATE, |q| {
            q.bind(("question", rid(&answer.question_id)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let state: Option<QuestionStateRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let state = state.ok_or(StoreError::NotFound)?;
        if state.scan != rid(&scan) || state.garden != rid(&editor.garden()) {
            return Err(StoreError::NotFound);
        }
        let run = Run(u32_of("run", state.run).map_err(|e| e.into_store(WHAT))?);
        let answers = answer_rows(scan, run, &evidence, Some(answer.question_id), now)
            .map_err(|e| e.into_store(WHAT))?;
        let option = enum_str("answer", &answer.option_id).map_err(|e| e.into_store(WHAT))?;
        let result = run_tx(&self.db, sql::ANSWER_QUESTION, |q| {
            q.bind(("question", rid(&answer.question_id)))
                .bind(("scan", rid(&scan)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("answers", answers.clone()))
                .bind(("answer", option.clone()))
                .bind(("now", dt(now)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            Err(f) if f.thrown() == Some(thrown::INVALID_STATE) => {
                return Err(StoreError::Rejected(DomainError::InvalidState(
                    InvalidStateDetails::default(),
                )));
            }
            Err(f) => return Err(f.into_store(WHAT)),
        };
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ClaimRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(AnswerOutcome {
            late: matches!(
                state.scan_status,
                ScanStatusRow::Diagnosed | ScanStatusRow::Planned
            ),
            claim: claim_attempt(&row, scan, PipelineStep::Score, now)
                .map_err(|e| e.into_store(WHAT))?,
        })
    }

    async fn advance_question_step(
        &self,
        claim: &QuestionStepClaim,
        output: QuestionStepOutput,
        now: Timestamp,
    ) -> Result<QuestionAdvance, StoreError> {
        const WHAT: &str = "advance question step";
        match output {
            QuestionStepOutput::Measure => {
                self.question_tx(sql::QUESTION_TO_MEASURING, claim, None, &[], None, now)
                    .await?;
                Ok(QuestionAdvance::Next(QuestionStepClaim {
                    step: QuestionStep::Measuring,
                    stamp: now,
                    ..*claim
                }))
            }
            QuestionStepOutput::Verify {
                observations,
                evidence,
                job,
            } => {
                let job = JobWriteRow::new(&job).map_err(|e| e.into_store(WHAT))?;
                self.question_tx(
                    sql::QUESTION_TO_VERIFYING,
                    claim,
                    Some(&observations),
                    &evidence,
                    Some(job),
                    now,
                )
                .await?;
                self.jobs_queued.notify_waiters();
                Ok(QuestionAdvance::Next(QuestionStepClaim {
                    step: QuestionStep::Verifying,
                    stamp: now,
                    ..*claim
                }))
            }
            QuestionStepOutput::Score {
                observations,
                evidence,
            } => {
                let mut results = self
                    .question_tx(
                        sql::QUESTION_TO_SCORE,
                        claim,
                        Some(&observations),
                        &evidence,
                        None,
                        now,
                    )
                    .await?;
                let last = results.num_statements().saturating_sub(2);
                let row: Option<ClaimRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
                let row = row.ok_or(StoreError::NotFound)?;
                Ok(QuestionAdvance::Scored(
                    claim_attempt(&row, claim.scan, PipelineStep::Score, now)
                        .map_err(|e| e.into_store(WHAT))?,
                ))
            }
        }
    }

    async fn drop_photo_answer(
        &self,
        claim: &QuestionStepClaim,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        self.question_tx(sql::DROP_PHOTO_ANSWER, claim, None, &[], None, now)
            .await
            .map(drop)
    }

    async fn stalled_question_steps(
        &self,
        older_than: Timestamp,
    ) -> Result<Vec<QuestionStepClaim>, StoreError> {
        const WHAT: &str = "stalled question steps";
        let mut results = run_tx(&self.db, sql::STALLED_QUESTION_STEPS, |q| {
            q.bind(("older_than", dt(older_than)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<QuestionClaimRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| QuestionStepClaim::try_from(row).map_err(|e| e.into_store(WHAT)))
            .collect()
    }

    async fn reclaim_question_step(
        &self,
        stale: &QuestionStepClaim,
        step: QuestionStep,
        now: Timestamp,
    ) -> Result<Option<QuestionStepClaim>, StoreError> {
        const WHAT: &str = "reclaim question step";
        let mut results = run_tx(&self.db, sql::RECLAIM_QUESTION_STEP, |q| {
            q.bind(("question", rid(&stale.question)))
                .bind(("step", QuestionStepRow::from(step)))
                .bind(("seen_step", QuestionStepRow::from(stale.step)))
                .bind(("seen", dt(stale.stamp)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<QuestionClaimRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        row.map(QuestionStepClaim::try_from)
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn questions_due(&self, now: Timestamp) -> Result<Vec<DueQuestion>, StoreError> {
        const WHAT: &str = "questions due";
        let mut r = run_tx(&self.db, sql::QUESTIONS_DUE, |q| q.bind(("now", dt(now))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<DueRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok(DueQuestion {
                    question: from_rid(&row.id)?,
                    scan: from_rid(&row.scan)?,
                    run: Run(u32_of("run", row.run)?),
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn continue_without_answer(
        &self,
        due: DueQuestion,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError> {
        const WHAT: &str = "continue without answer";
        let mut results = run_tx(&self.db, sql::CONTINUE_WITHOUT_ANSWER, |q| {
            q.bind(("question", rid(&due.question)))
                .bind(("scan", rid(&due.scan)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ClaimRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        claim_attempt(
            &row.ok_or(StoreError::NotFound)?,
            due.scan,
            PipelineStep::Score,
            now,
        )
        .map_err(|e| e.into_store(WHAT))
    }

    async fn similar_cases(
        &self,
        member: Membership<Viewer>,
        image: ImageId,
        k: u32,
    ) -> Result<Vec<SimilarCase>, StoreError> {
        const WHAT: &str = "similar cases";
        let mut results = run_tx(&self.db, sql::SIMILAR_CASES, |q| {
            q.bind(("image", rid(&image)))
                .bind(("garden", rid(&member.garden())))
                .bind(("k", i64::from(k.min(5))))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<SimilarRow> = results.take(2).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                let primary = match (row.primary_problem, row.primary_name) {
                    (Some(problem), Some(name)) => Some(domain::chat::ProblemRef {
                        problem_id: from_rid(&problem)?,
                        name,
                    }),
                    _ => None,
                };
                Ok(SimilarCase {
                    scan_id: from_rid(&row.scan)?,
                    created_at: ts(row.created_at),
                    primary,
                    similarity: row.similarity,
                    thumbnail_image_id: row.thumbnail.as_ref().map(from_rid).transpose()?,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn finish_guided_check(
        &self,
        lease: &Lease,
        outcome: GuidedOutcome,
    ) -> Result<GuidedFinish, StoreError> {
        const WHAT: &str = "finish guided check";
        let observation = outcome
            .observation
            .as_ref()
            .map(|o| {
                Ok::<_, RowError>(ObservationEntry {
                    id: rid(&o.id),
                    row: ObservationRow::try_from(o)?,
                })
            })
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let status = enum_str("status", &outcome.status).map_err(|e| e.into_store(WHAT))?;
        let mut state = run_tx(&self.db, sql::JOB_SCAN, |q| {
            q.bind(("job", rid(&lease.job)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let scan: Option<RecordId> = state.take(0).map_err(|e| take_failed(WHAT, e))?;
        let scan: ScanId = from_rid(&scan.ok_or(StoreError::Conflict(Conflict::LeaseLost {
            job_id: lease.job,
        }))?)
        .map_err(|e| e.into_store(WHAT))?;
        let mut results = run_tx(&self.db, sql::FINISH_GUIDED_CHECK, |q| {
            q.bind(("job", rid(&lease.job)))
                .bind(("lease_token", lease.token.as_str()))
                .bind(("worker", lease.worker.as_str()))
                .bind(("observation", observation.clone()))
                .bind(("status", status.clone()))
        })
        .await
        .map_err(|f| lease_failure(f, lease, WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ClaimRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(GuidedFinish {
            scan_status: row.status.map(Into::into).ok_or(StoreError::NotFound)?,
            // A claim's stamp comes back with the row (the database clock); the fallback is unused.
            claim: claim_attempt(
                &row,
                scan,
                PipelineStep::Score,
                Timestamp::from(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH),
            )
            .map_err(|e| e.into_store(WHAT))?,
        })
    }
}
