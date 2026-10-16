//! A photo answer's `measuring` step ([contracts 3], a photo answer that stalls; [contracts
//! 7], photos sent for a question): measure the photo, answer what the measurements settle,
//! then queue `verify` for the rest, or claim `score` when nothing is left.

use std::time::Instant;

use domain::{
    events::{ScanEventBody, Stage},
    id::ProblemId,
    jobs::{JobKind, JobSpec, VerifyReason},
    ports::{
        ClaimAttempt, IdGenExt, NewJob, ObservationWrite, QuestionAdvance, QuestionStepClaim,
        QuestionStepOutput, StepClaim,
    },
    rules::questions::QuestionStep,
    scan::QuestionStatus,
};
use knowledge::{
    candidates::{CandidateSet, ProfilePool},
    scoring::AnswerSet,
};

use super::{completed, started};
use crate::{Pipeline, PipelineError, findings, measure::answer_indicator, payloads};

impl Pipeline {
    /// Runs `measuring` and returns the scan's `score` claim when the measurements settled
    /// everything. Any other step is a job's, owned by the broker.
    #[expect(
        clippy::too_many_lines,
        reason = "a step reads top to bottom in the order of its contract section"
    )]
    pub(super) async fn measuring(
        &self,
        claim: QuestionStepClaim,
    ) -> Result<Option<StepClaim>, PipelineError> {
        if claim.step != QuestionStep::Measuring {
            return Ok(None);
        }
        let snap = self.deps.store.snapshot(claim.scan).await?;
        let Some(question) = snap
            .scan
            .open_question
            .as_ref()
            .filter(|q| q.id == claim.question && q.status == QuestionStatus::Answering)
        else {
            return Ok(None);
        };
        let Some(photo) = snap
            .scan
            .images
            .iter()
            .rev()
            .find(|i| i.answers_question_id == Some(claim.question))
        else {
            return Err(PipelineError::Inconsistent(
                "a photo answer without its photo",
            ));
        };

        // Measure the photo, when it has depth.
        let mut new_obs = Vec::new();
        if photo.depth.is_some() {
            let t = Instant::now();
            self.emit(snap.scan.id, claim.run, vec![started(Stage::Measurement)])
                .await?;
            let measured = self.measure_images(&snap, &[photo]).await;
            self.emit(
                snap.scan.id,
                claim.run,
                vec![completed(Stage::Measurement, t, measured.skipped)],
            )
            .await?;
            new_obs = measured.observations;
        }
        let write = ObservationWrite {
            stage: Stage::Measurement,
            run: claim.run,
            images: vec![photo.id],
            replace_imageless: false,
            observations: new_obs.clone(),
            masks: Vec::new(),
        };

        // Answer what the measurements settle, for the question's candidates.
        let ids: Vec<ProblemId> = question.candidates.clone();
        let profiles = self.deps.knowledge.problems(&ids).await?;
        let pool = ProfilePool::new(&profiles);
        let selected: Vec<_> = snap
            .candidates
            .iter()
            .filter(|c| ids.contains(&c.problem_id))
            .cloned()
            .collect();
        let set = CandidateSet::from_selected(&selected, &pool, &[], snap.weather.as_ref());
        let all_obs: Vec<_> = snap.observations.iter().chain(&new_obs).cloned().collect();
        let measured = findings::measured(&all_obs, &snap.scan.images, &snap.depth);
        let evidence: Vec<_> = set
            .as_slice()
            .iter()
            .flat_map(knowledge::candidates::Candidate::indicators)
            .filter_map(|i| answer_indicator(i, &measured))
            .collect();
        let mut stored = snap.evidence.clone();
        stored.extend(evidence.iter().cloned());
        let prompts = payloads::prompts(set.as_slice(), &AnswerSet::new(&stored));

        let output = if prompts.is_empty() {
            QuestionStepOutput::Score {
                observations: write,
                evidence,
            }
        } else {
            let id = self.deps.ids.id();
            QuestionStepOutput::Verify {
                observations: write,
                evidence,
                job: NewJob {
                    id,
                    scan: snap.scan.id,
                    run: Some(claim.run),
                    question: Some(claim.question),
                    spec: JobSpec::Verify(payloads::verify_payload(
                        id,
                        VerifyReason::NewPhoto,
                        Some(claim.question),
                        &snap.scan.images,
                        Some(photo.id),
                        prompts,
                    )),
                    max_runtime: JobKind::Verify.max_runtime(false),
                    queued_at: self.now(),
                },
            }
        };
        let advance = self
            .deps
            .store
            .advance_question_step(&claim, output, self.now())
            .await?;
        self.emit(
            snap.scan.id,
            claim.run,
            new_obs
                .into_iter()
                .map(|observation| ScanEventBody::Finding { observation })
                .collect(),
        )
        .await?;
        Ok(match advance {
            QuestionAdvance::Scored(ClaimAttempt::Claimed(c)) => Some(c),
            // `verifying`: the store queued the job and signalled the claim long poll.
            QuestionAdvance::Next(_)
            | QuestionAdvance::Scored(ClaimAttempt::Busy | ClaimAttempt::NotNeeded) => None,
        })
    }
}
