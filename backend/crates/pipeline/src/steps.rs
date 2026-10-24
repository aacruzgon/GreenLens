//! The step machine ([contracts 3], steps between jobs, and recovery; [database 4], steps
//! between jobs).
//!
//! ```text
//!  perception result ──claim──▶ retrieve ──commit: vlm job queued──▶ (release)
//!  vlm / run verify result ─┐
//!  answer, continuation,    ├─claim──▶ score ──commit──▶ release (question asked, verify job, nothing)
//!  late answer, guided ─────┘                 └─────────▶ plan ──commit: plan stored──▶ release
//!  any commit with rescore_pending ──────────▶ score
//! ```
//!
//! Each step takes its claim, re-reads the scan (`ScanRepo::snapshot`), does its work and
//! commits one `StepOutput`. The commit re-checks the claim, the run and the stamp; if anything
//! changed it fails with `Conflict::ClaimLost` and the step stops ([`PipelineError::lost`]).

use domain::{
    events::{ScanEventBody, Skipped, Stage},
    ports::{ScanSnapshot, StepClaim, StepOutput},
    rules::steps::PipelineStep,
    scan::{ScanFailure, ScanFailureCode, ScanStatus},
    url::Stored,
    wire::Run,
};

use crate::{FollowUp, Pipeline, PipelineError};

pub(crate) mod plan;
mod question;
mod retrieve;
pub(crate) mod score;

impl Pipeline {
    /// Runs a follow-up: the scan's steps from a claim, following each hand-over, or a photo
    /// answer's `measuring` step. A lost claim ends quietly: another owner has the work.
    pub async fn follow_up(&self, follow_up: FollowUp) -> Result<(), PipelineError> {
        let result = match follow_up {
            FollowUp::Step(claim) => self.run_steps(claim).await,
            FollowUp::QuestionStep(claim) => match self.measuring(claim).await {
                Ok(Some(next)) => self.run_steps(next).await,
                Ok(None) => Ok(()),
                Err(e) => Err(e),
            },
        };
        match result {
            Err(e) if e.lost() => {
                tracing::info!("step lost its claim; another owner continues");
                Ok(())
            }
            other => other,
        }
    }

    async fn run_steps(&self, mut claim: StepClaim) -> Result<(), PipelineError> {
        loop {
            let next = match claim.step {
                PipelineStep::Retrieve => self.retrieve(claim).await?,
                PipelineStep::Score => self.score(claim).await?,
                PipelineStep::Plan => self.plan(claim).await?,
            };
            match next {
                Some(c) => claim = c,
                None => return Ok(()),
            }
        }
    }

    /// Commits a step's output; the next claim, or `None` when released.
    async fn commit(
        &self,
        claim: StepClaim,
        output: StepOutput,
    ) -> Result<Option<StepClaim>, PipelineError> {
        Ok(self
            .deps
            .store
            .commit_step(claim, output, self.now())
            .await?)
    }

    /// Reads the scan for a step; `None` when the claim's run is no longer the scan's (its
    /// commit would be refused anyway).
    async fn snapshot_for(&self, claim: StepClaim) -> Result<Option<ScanSnapshot>, PipelineError> {
        let snap = self.deps.store.snapshot(claim.scan).await?;
        Ok((snap.scan.run == claim.run).then_some(snap))
    }

    /// Fails the scan's run ([contracts 3]): releases the claim and supersedes the question in
    /// the store's transaction, then `question_resolved` (superseded) and `failed`, in one
    /// batch.
    async fn fail_run(
        &self,
        snap: &ScanSnapshot,
        code: ScanFailureCode,
        message: &str,
    ) -> Result<(), PipelineError> {
        let failure = ScanFailure::new(code, message);
        let failed = self
            .deps
            .store
            .fail_scan(snap.scan.id, snap.scan.run, failure.clone(), self.now())
            .await?;
        let mut events = Vec::new();
        if let Some(q) = failed.superseded_question {
            events.push(ScanEventBody::QuestionResolved {
                question_id: q,
                by: domain::events::ResolvedBy::Superseded,
            });
        }
        events.push(ScanEventBody::Failed { failure });
        self.emit(snap.scan.id, snap.scan.run, events).await
    }
}

/// `stage_started`.
pub(crate) const fn started(stage: Stage) -> ScanEventBody<Stored> {
    ScanEventBody::StageStarted { stage }
}

/// `stage_completed`, with how long it took and why it was skipped.
pub(crate) fn completed(
    stage: Stage,
    since: std::time::Instant,
    skipped: Option<&str>,
) -> ScanEventBody<Stored> {
    ScanEventBody::StageCompleted {
        stage,
        duration_ms: u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX),
        skipped: skipped.map(|reason| Skipped {
            reason: reason.to_owned(),
        }),
    }
}

/// `status_changed`, when the status moves.
pub(crate) fn status_change(from: ScanStatus, to: ScanStatus) -> Option<ScanEventBody<Stored>> {
    (from != to).then_some(ScanEventBody::StatusChanged { from, to })
}

/// The run of a snapshot, for events.
pub(crate) const fn run_of(snap: &ScanSnapshot) -> Run {
    snap.scan.run
}
