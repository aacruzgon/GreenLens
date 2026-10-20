//! `api`'s steps between jobs: the claim, the commit with its re-check and hand-over, and the
//! sweep's re-claim ([contracts 3], steps between jobs; [database 4], steps between jobs ✓).

use chrono::{DateTime, Utc};
use domain::{
    id::ScanId,
    ports::{ClaimAttempt, NewQuestion, StaleStep, StepClaim, StepOutput, StepRepo, StoreError},
    records::diagnosis::Diagnosis,
    rules::steps::PipelineStep,
    url::Stored,
    wire::{Run, Timestamp},
};
use surrealdb::{
    engine::any::Any,
    method::Query,
    types::{RecordId, SurrealValue},
};

use crate::{
    SurrealStore,
    error::{RowError, take_failed},
    ids::{from_rid, opt_rid, rid},
    repo::{
        analysis::{ObservationWriteRow, answer_rows, enum_str},
        jobs::JobWriteRow,
    },
    rows::{PipelineStepRow, ScanStatusRow, dt, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements. Each commit is one transaction: [`claim_check!`] first (a lost claim throws
/// `claim_lost` and nothing is written), the output, then [`handover!`], whose `RETURN` is the
/// last statement before `COMMIT`.
pub mod sql {
    use crate::fragments::{
        claim_check, claim_entry, evidence_write, handover, job_enqueue, observation_write,
        resolve_question,
    };

    /// An entry point's claim, alone in its transaction.
    pub const CLAIM_STEP: &str = concat!("BEGIN;\n", claim_entry!(), "COMMIT;");

    /// `retrieve`: the measurement observations, the measured answers, the candidate set and
    /// the weather snapshot, and the `vlm` job (checked against an existing one of the run).
    /// Hands over by releasing when a job was queued, else to `score`.
    pub const COMMIT_RETRIEVE: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        observation_write!(),
        evidence_write!(),
        "UPDATE $scan SET candidates = $candidates, weather = $weather;\n",
        job_enqueue!(),
        handover!(),
        "COMMIT;"
    );

    /// `score`: the diagnosis with a compare-and-set on `scan.diagnosis` (its `supersedes`, or
    /// `NONE` for the run's first), the status move, a resolved question, and the hand-over
    /// (`plan`, or release).
    pub const COMMIT_SCORE: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        "CREATE $diagnosis.id CONTENT $diagnosis.row;
LET $dwon = (UPDATE $scan SET diagnosis = $diagnosis.id WHERE diagnosis = $expected_diagnosis RETURN VALUE diagnosis);
IF array::len($dwon) = 0 { THROW 'claim_lost' };
IF $status != NONE { UPDATE $scan SET status = $status; };
",
        resolve_question!(),
        handover!(),
        "COMMIT;"
    );

    /// `score` that asks: the question, `awaiting_answer`, `questions_asked += 1`, release.
    pub const COMMIT_ASK: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        "CREATE $question.id CONTENT $question.row;
UPDATE $scan SET status = 'awaiting_answer', open_question = $question.id, questions_asked += 1;
",
        resolve_question!(),
        handover!(),
        "COMMIT;"
    );

    /// `score` that goes outside the candidate set: measured answers for the added
    /// candidates, the `verify` job (or none when the measurements settled everything), the
    /// candidates appended, then release (or `score` when no job was queued).
    pub const COMMIT_VERIFY: &str = concat!(
        "BEGIN;\n",
        claim_check!(),
        evidence_write!(),
        job_enqueue!(),
        "UPDATE $scan SET candidates = array::concat(candidates, $added_candidates);\n",
        handover!(),
        "COMMIT;"
    );

    /// Nothing to write: only the hand-over by the rescore rule.
    pub const COMMIT_RELEASE: &str = concat!("BEGIN;\n", claim_check!(), handover!(), "COMMIT;");

    /// Claims older than their limit (index `scan_stalled` ✓): 2 minutes for `retrieve` and
    /// `score`, 5 for `plan`, on scans that aren't `pending`, `failed` or being deleted.
    pub const STALE_STEPS: &str = "SELECT id, pipeline_step, pipeline_step_run, pipeline_step_at FROM scan
  WHERE pipeline_step = 'retrieve' AND pipeline_step_at < $short AND !(status IN ['pending', 'failed'])
    AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;
SELECT id, pipeline_step, pipeline_step_run, pipeline_step_at FROM scan
  WHERE pipeline_step = 'score' AND pipeline_step_at < $short AND !(status IN ['pending', 'failed'])
    AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;
SELECT id, pipeline_step, pipeline_step_run, pipeline_step_at FROM scan
  WHERE pipeline_step = 'plan' AND pipeline_step_at < $long AND !(status IN ['pending', 'failed'])
    AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;";

    /// The sweep's re-claim by the old stamp and run ([database 4] ✓); `NONE` back means
    /// someone else has it. Its new stamp makes a slow original's commit throw `claim_lost`.
    pub const RECLAIM_STEP: &str = "UPDATE ONLY $scan SET pipeline_step_at = $now
  WHERE pipeline_step = $step AND pipeline_step_at = $seen AND pipeline_step_run = run AND pipeline_step_run = $run
    AND plant.deleting_at = NONE AND garden.deleting_at = NONE
  RETURN AFTER;";
}

/// The result of [`claim_entry!`](crate::fragments).
#[derive(Debug, Clone, SurrealValue)]
pub struct ClaimRow {
    /// `claimed`, `busy` or `not_needed`.
    pub outcome: String,
    /// The run the claim belongs to, when claimed.
    pub run: Option<i64>,
    /// The claim's stamp as stored (equal to the bound `$now`).
    pub stamp: Option<DateTime<Utc>>,
    /// The scan's status when the claim was tried.
    pub status: Option<ScanStatusRow>,
}

/// The domain outcome of a claim taken at `now`.
pub fn claim_attempt(
    row: &ClaimRow,
    scan: ScanId,
    step: PipelineStep,
    now: Timestamp,
) -> Result<ClaimAttempt, RowError> {
    match (row.outcome.as_str(), row.run) {
        ("claimed", Some(run)) => Ok(ClaimAttempt::Claimed(StepClaim {
            scan,
            step,
            run: Run(u32_of("pipeline_step_run", run)?),
            stamp: row.stamp.map_or(now, ts),
        })),
        ("busy", _) => Ok(ClaimAttempt::Busy),
        ("not_needed", _) => Ok(ClaimAttempt::NotNeeded),
        (other, _) => Err(RowError::BadField {
            field: "outcome",
            problem: format!("unexpected claim outcome {other:?}"),
        }),
    }
}

/// The claim's binds for [`claim_check!`](crate::fragments): `$scan`, `$step`, `$stamp`,
/// `$run`. `'c` is the borrow of the client the query holds; the binds are owned values.
pub fn bind_claim<'c>(q: Query<'c, Any>, claim: &StepClaim) -> Query<'c, Any> {
    q.bind(("scan", rid(&claim.scan)))
        .bind(("step", PipelineStepRow::from(claim.step)))
        .bind(("stamp", dt(claim.stamp)))
        .bind(("run", i64::from(claim.run.0)))
}

/// A claimed scan as [`sql::STALE_STEPS`] and [`sql::RECLAIM_STEP`] read it.
#[derive(Debug, Clone, SurrealValue)]
pub struct ClaimedScanRow {
    /// The scan.
    pub id: RecordId,
    /// The step.
    pub pipeline_step: Option<PipelineStepRow>,
    /// The run of the claim.
    pub pipeline_step_run: Option<i64>,
    /// The stamp.
    pub pipeline_step_at: Option<DateTime<Utc>>,
}

impl TryFrom<ClaimedScanRow> for StaleStep {
    type Error = RowError;

    fn try_from(row: ClaimedScanRow) -> Result<Self, RowError> {
        let missing = |field| RowError::BadField {
            field,
            problem: "missing on a claimed scan".to_owned(),
        };
        Ok(Self {
            scan: from_rid(&row.id)?,
            step: row
                .pipeline_step
                .ok_or_else(|| missing("pipeline_step"))?
                .into(),
            run: Run(u32_of(
                "pipeline_step_run",
                row.pipeline_step_run
                    .ok_or_else(|| missing("pipeline_step_run"))?,
            )?),
            seen: ts(row
                .pipeline_step_at
                .ok_or_else(|| missing("pipeline_step_at"))?),
        })
    }
}

/// A `diagnosis` row ([database 3]); the contract shapes are `FLEXIBLE` JSON.
/// `organisms[].preys_on` is never stored (computed from the graph when read, database 3).
#[derive(Debug, Clone, SurrealValue)]
pub struct DiagnosisRow {
    /// Its scan.
    pub scan: RecordId,
    /// The run.
    pub run: i64,
    /// The diagnosis it replaces.
    pub supersedes: Option<RecordId>,
    /// `confirmed`, `uncertain` or `healthy`.
    pub status: String,
    /// Uncertain, and nothing points to any problem.
    pub low_evidence: bool,
    /// For the timeline and similar cases.
    pub primary_problem: Option<RecordId>,
    /// `CandidateScore`.
    pub primary: Option<serde_json::Value>,
    /// Ranked candidates.
    pub candidates: Vec<serde_json::Value>,
    /// Organisms, without `preys_on`.
    pub organisms: Vec<serde_json::Value>,
    /// The soil observations used.
    pub soil: Vec<RecordId>,
    /// Risks.
    pub risks: Vec<serde_json::Value>,
    /// `EvidenceBasis`.
    pub evidence_basis: serde_json::Value,
    /// Citations.
    pub citations: Vec<serde_json::Value>,
    /// Model name → version.
    pub model_versions: serde_json::Value,
    /// When.
    pub created_at: DateTime<Utc>,
}

/// A diagnosis with its record id.
#[derive(Debug, Clone, SurrealValue)]
pub struct DiagnosisWriteRow {
    /// `diagnosis:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: DiagnosisRow,
}

fn json_vec<T: serde::Serialize>(
    field: &'static str,
    items: &[T],
) -> Result<Vec<serde_json::Value>, RowError> {
    items.iter().map(|i| to_json(field, i)).collect()
}

impl TryFrom<&Diagnosis<Stored>> for DiagnosisWriteRow {
    type Error = RowError;

    fn try_from(d: &Diagnosis<Stored>) -> Result<Self, RowError> {
        let mut organisms = json_vec("organisms", &d.organisms)?;
        for organism in &mut organisms {
            if let Some(object) = organism.as_object_mut() {
                object.remove("preys_on");
            }
        }
        Ok(Self {
            id: rid(&d.id),
            row: DiagnosisRow {
                scan: rid(&d.scan_id),
                run: i64::from(d.run.0),
                supersedes: opt_rid(d.supersedes.as_ref()),
                status: enum_str("status", &d.status)?,
                low_evidence: d.low_evidence,
                primary_problem: d.primary.as_ref().map(|p| rid(&p.problem_id)),
                primary: d
                    .primary
                    .as_ref()
                    .map(|p| to_json("primary", p))
                    .transpose()?,
                candidates: json_vec("candidates", &d.candidates)?,
                organisms,
                soil: d.soil.iter().map(|o| rid(&o.id)).collect(),
                risks: json_vec("risks", &d.risks)?,
                evidence_basis: to_json("evidence_basis", &d.evidence_basis)?,
                citations: json_vec("citations", &d.citations)?,
                model_versions: to_json("model_versions", &d.model_versions)?,
                created_at: dt(d.created_at),
            },
        })
    }
}

/// A `question` row as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct QuestionRow {
    /// Its scan.
    pub scan: RecordId,
    /// Its run.
    pub run: i64,
    /// `indicator` or `photo_request`.
    pub kind: String,
    /// The question.
    pub text: String,
    /// Indicator questions' options.
    pub options: Option<serde_json::Value>,
    /// `photo_request`: which shot.
    pub photo_kind: Option<String>,
    /// `photo_request`: the hint.
    pub photo_hint: Option<String>,
    /// The problems it separates.
    pub candidates: Vec<RecordId>,
    /// The indicator it asks about (server-side only).
    pub indicator: Option<String>,
    /// When it was asked.
    pub asked_at: DateTime<Utc>,
    /// When the scheduler continues without it.
    pub continue_at: DateTime<Utc>,
}

/// A question with its record id.
#[derive(Debug, Clone, SurrealValue)]
pub struct QuestionWriteRow {
    /// `question:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: QuestionRow,
}

impl QuestionWriteRow {
    /// The row of a new question of `scan`.
    pub fn new(scan: ScanId, new: &NewQuestion) -> Result<Self, RowError> {
        let q = &new.question;
        Ok(Self {
            id: rid(&q.id),
            row: QuestionRow {
                scan: rid(&scan),
                run: i64::from(q.run.0),
                kind: enum_str("kind", &q.kind)?,
                text: q.text.clone(),
                options: q
                    .options
                    .as_ref()
                    .map(|o| to_json("options", o))
                    .transpose()?,
                photo_kind: q
                    .photo_kind
                    .as_ref()
                    .map(|k| enum_str("photo_kind", k))
                    .transpose()?,
                photo_hint: q.photo_hint.clone(),
                candidates: q.candidates.iter().map(rid).collect(),
                indicator: new.indicator.as_ref().map(ToString::to_string),
                asked_at: dt(q.asked_at),
                continue_at: dt(q.continue_at),
            },
        })
    }
}

impl SurrealStore {
    /// Sends one commit transaction and returns the next step it handed over to.
    async fn send_commit<'a, F>(
        &'a self,
        sql: &'static str,
        claim: &StepClaim,
        handover: Option<PipelineStep>,
        now: Timestamp,
        extra: F,
    ) -> Result<Option<StepClaim>, StoreError>
    where
        F: Fn(Query<'a, Any>) -> Query<'a, Any> + Send + Sync,
    {
        const WHAT: &str = "commit step";
        let mut results = run_tx(&self.db, sql, |q| {
            extra(bind_claim(q, claim))
                .bind(("handover", handover.map(PipelineStepRow::from)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let next: Option<PipelineStepRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        Ok(next.map(|step| StepClaim {
            scan: claim.scan,
            step: step.into(),
            run: claim.run,
            stamp: now,
        }))
    }
}

#[async_trait::async_trait]
impl StepRepo for SurrealStore {
    async fn claim_step(
        &self,
        scan: ScanId,
        step: PipelineStep,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError> {
        const WHAT: &str = "claim step";
        let mut results = run_tx(&self.db, sql::CLAIM_STEP, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("step", PipelineStepRow::from(step)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<ClaimRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        claim_attempt(&row.ok_or(StoreError::NotFound)?, scan, step, now)
            .map_err(|e| e.into_store(WHAT))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per StepOutput variant, each a bind list"
    )]
    async fn commit_step(
        &self,
        claim: StepClaim,
        output: StepOutput,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError> {
        const WHAT: &str = "commit step";
        let row_err = |e: RowError| e.into_store(WHAT);
        let next = match output {
            StepOutput::Retrieve {
                observations,
                evidence,
                candidates,
                weather,
                job,
            } => {
                let obs = observations
                    .as_ref()
                    .map(|w| ObservationWriteRow::new(claim.scan, w))
                    .transpose()
                    .map_err(row_err)?;
                let measured = ObservationWriteRow::measured_images(observations.as_ref());
                let answers =
                    answer_rows(claim.scan, claim.run, &evidence, None, now).map_err(row_err)?;
                let candidates = to_json("candidates", &candidates).map_err(row_err)?;
                let weather = weather
                    .as_ref()
                    .map(|w| to_json("weather", w))
                    .transpose()
                    .map_err(row_err)?;
                let queued = job.is_some();
                let job = job
                    .as_ref()
                    .map(JobWriteRow::new)
                    .transpose()
                    .map_err(row_err)?;
                let handover = (!queued).then_some(PipelineStep::Score);
                let next = self
                    .send_commit(sql::COMMIT_RETRIEVE, &claim, handover, now, |q| {
                        q.bind(("obs", obs.clone()))
                            .bind(("measured_images", measured.clone()))
                            .bind(("answers", answers.clone()))
                            .bind(("candidates", candidates.clone()))
                            .bind(("weather", weather.clone()))
                            .bind(("job", job.clone()))
                    })
                    .await?;
                if queued {
                    self.jobs_queued.notify_waiters();
                }
                next
            }
            StepOutput::Score {
                diagnosis,
                status,
                resolved_question,
                needs_plan,
            } => {
                let row = DiagnosisWriteRow::try_from(&diagnosis).map_err(row_err)?;
                let expected = opt_rid(diagnosis.supersedes.as_ref());
                let status = status.map(ScanStatusRow::from);
                let resolved = opt_rid(resolved_question.as_ref());
                let handover = needs_plan.then_some(PipelineStep::Plan);
                self.send_commit(sql::COMMIT_SCORE, &claim, handover, now, |q| {
                    q.bind(("diagnosis", row.clone()))
                        .bind(("expected_diagnosis", expected.clone()))
                        .bind(("status", status))
                        .bind(("resolved", resolved.clone()))
                })
                .await?
            }
            StepOutput::Ask {
                question,
                resolved_question,
            } => {
                let row = QuestionWriteRow::new(claim.scan, &question).map_err(row_err)?;
                let resolved = opt_rid(resolved_question.as_ref());
                self.send_commit(sql::COMMIT_ASK, &claim, None, now, |q| {
                    q.bind(("question", row.clone()))
                        .bind(("resolved", resolved.clone()))
                })
                .await?
            }
            StepOutput::Verify {
                job,
                evidence,
                added_candidates,
            } => {
                let answers =
                    answer_rows(claim.scan, claim.run, &evidence, None, now).map_err(row_err)?;
                let added = to_json("candidates", &added_candidates).map_err(row_err)?;
                let queued = job.is_some();
                let job = job
                    .as_ref()
                    .map(JobWriteRow::new)
                    .transpose()
                    .map_err(row_err)?;
                let handover = (!queued).then_some(PipelineStep::Score);
                let next = self
                    .send_commit(sql::COMMIT_VERIFY, &claim, handover, now, |q| {
                        q.bind(("measured_images", Vec::<RecordId>::new()))
                            .bind(("answers", answers.clone()))
                            .bind(("job", job.clone()))
                            .bind(("added_candidates", added.clone()))
                    })
                    .await?;
                if queued {
                    self.jobs_queued.notify_waiters();
                }
                next
            }
            StepOutput::Plan(write) => {
                self.commit_plan(&claim, &write, now)
                    .await?
                    .map(|step| StepClaim {
                        scan: claim.scan,
                        step: step.into(),
                        run: claim.run,
                        stamp: now,
                    })
            }
            StepOutput::Release => {
                self.send_commit(sql::COMMIT_RELEASE, &claim, None, now, |q| q)
                    .await?
            }
        };
        Ok(next)
    }

    async fn stale_steps(&self, now: Timestamp) -> Result<Vec<StaleStep>, StoreError> {
        const WHAT: &str = "stale steps";
        let short = now.minus(PipelineStep::Score.stale_after());
        let long = now.minus(PipelineStep::Plan.stale_after());
        let mut results = run_tx(&self.db, sql::STALE_STEPS, |q| {
            q.bind(("short", dt(short))).bind(("long", dt(long)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let mut stale = Vec::new();
        for index in 0..3 {
            let rows: Vec<ClaimedScanRow> =
                results.take(index).map_err(|e| take_failed(WHAT, e))?;
            for row in rows {
                stale.push(StaleStep::try_from(row).map_err(|e| e.into_store(WHAT))?);
            }
        }
        Ok(stale)
    }

    async fn reclaim_step(
        &self,
        stale: StaleStep,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError> {
        const WHAT: &str = "reclaim step";
        let mut results = run_tx(&self.db, sql::RECLAIM_STEP, |q| {
            q.bind(("scan", rid(&stale.scan)))
                .bind(("step", PipelineStepRow::from(stale.step)))
                .bind(("seen", dt(stale.seen)))
                .bind(("run", i64::from(stale.run.0)))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<ClaimedScanRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(row.map(|_| StepClaim {
            scan: stale.scan,
            step: stale.step,
            run: stale.run,
            stamp: now,
        }))
    }
}
