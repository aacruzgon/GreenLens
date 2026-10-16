//! `api`'s steps between jobs ([contracts 3], steps between jobs; [database 4], steps between
//! jobs).

use crate::{
    events::SelectedCandidate,
    id::{QuestionId, ScanId},
    ports::{ClaimAttempt, NewJob, ObservationWrite, PlanWrite, StepClaim, StoreError},
    records::{
        diagnosis::{Diagnosis, IndicatorEvidence},
        weather::WeatherSummary,
    },
    rules::steps::PipelineStep,
    scan::{Question, ScanStatus},
    url::Stored,
    wire::{Run, Timestamp},
};

/// A question the `score` step asks.
#[derive(Debug, Clone, PartialEq)]
pub struct NewQuestion {
    /// The question as the app sees it (`status: open`).
    pub question: Question,
    /// The indicator an `indicator` question asks (`question.indicator`, server-side only):
    /// the answer is stored as that indicator's `user` evidence.
    pub indicator: Option<crate::id::IndicatorId>,
}

/// A step's output, committed in one transaction that first re-checks the claim.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutput {
    /// `retrieve`: measurements, the indicators they settled, and the `vlm` job (or none when
    /// the run goes straight to scoring). Hands over by releasing the claim when a job is
    /// queued, else to `score`.
    Retrieve {
        /// The measurement stage's observations.
        observations: Option<ObservationWrite>,
        /// Measured answers.
        evidence: Vec<IndicatorEvidence>,
        /// The run's candidate set, stored for `score` (`scan.candidates`).
        candidates: Vec<SelectedCandidate>,
        /// The weather snapshot (`scan.weather`), when it could be fetched.
        weather: Option<WeatherSummary>,
        /// The `vlm` (or `verify`) job; checked against an existing job of the run first.
        job: Option<NewJob>,
    },
    /// `score`: the diagnosis (a compare-and-set on `scan.diagnosis` like plans). Hands over to
    /// `plan` when `needs_plan`, else releases.
    Score {
        /// The new diagnosis.
        diagnosis: Diagnosis<Stored>,
        /// The status it moves to, when it moves (`diagnosed`); late work moves nothing.
        status: Option<ScanStatus>,
        /// The question this score resolves: a photo answer whose `verify` job finished (an
        /// indicator answer is resolved by `AnalysisRepo::answer_question` itself).
        resolved_question: Option<QuestionId>,
        /// The step's own hand-over: release, or `plan`.
        needs_plan: bool,
    },
    /// `score`, ambiguous and the run may still ask: stores the question, moves the scan to
    /// `awaiting_answer`, increments `questions_asked` and releases the claim. No diagnosis is
    /// stored: the run's diagnosis comes after the answer or the 10-minute continuation.
    Ask {
        /// The question (`status: open`).
        question: NewQuestion,
        /// A photo answer resolved by the same score, as in `Score`.
        resolved_question: Option<QuestionId>,
    },
    /// `score`, when the VLM found an organism or damage pattern outside the candidate set: the
    /// candidates it adds to the run's set, the indicators a measurement settled for them, and
    /// the `verify` job for the rest (checked against an existing job of the run first).
    /// Releases the claim when a job is queued; with no job (measurements settled everything)
    /// hands over to `score`.
    Verify {
        /// The `verify` job (`reason: outside_candidate_set`), unless nothing is left to ask.
        job: Option<NewJob>,
        /// Measured answers for the added candidates.
        evidence: Vec<IndicatorEvidence>,
        /// Candidates added to the run's set (source `organism` or `damage`).
        added_candidates: Vec<SelectedCandidate>,
    },
    /// `plan`: the plan, written with its compare-and-set, then the claim is released.
    Plan(Box<PlanWrite>),
    /// Nothing to write (a re-run that finds its work already done, or a `score` that finds
    /// the run's VLM stage still in flight): releases the claim, or hands over to `score` when
    /// `rescore_pending` is set.
    Release,
}

/// A claim older than its step's limit, found by the sweep (index `scan_stalled`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleStep {
    /// The scan.
    pub scan: ScanId,
    /// The step.
    pub step: PipelineStep,
    /// The run of the claim.
    pub run: Run,
    /// The stamp the sweep saw.
    pub seen: Timestamp,
}

/// The step claim on `scan` ([database 4], steps between jobs).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait StepRepo: Send + Sync {
    /// An entry point's claim: a compare-and-set on `pipeline_step = NONE` recording the
    /// scan's current run (claiming `score` clears `rescore_pending`); when another step holds
    /// it, sets `rescore_pending` and returns `Busy`.
    async fn claim_step(
        &self,
        scan: ScanId,
        step: PipelineStep,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError>;

    /// Commits a step's output under its claim and hands over: the next claim, or `None` when
    /// released. `Conflict::ClaimLost` when the claim, stamp or run no longer holds or the plant
    /// or garden is being deleted; nothing is written then.
    async fn commit_step(
        &self,
        claim: StepClaim,
        output: StepOutput,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError>;

    /// Claims older than their limit on scans that aren't `pending`, `failed` or being deleted.
    async fn stale_steps(&self, now: Timestamp) -> Result<Vec<StaleStep>, StoreError>;

    /// The sweep's re-claim, a compare-and-set on the old stamp and run; `None` when someone
    /// else has it.
    async fn reclaim_step(
        &self,
        stale: StaleStep,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError>;
}
