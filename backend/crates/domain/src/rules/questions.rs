//! Question lifecycle ([contracts 3], questions and photo answers; [database 3]
//! `question.status` and `question.step`).

use std::time::Duration;

use crate::{events::ResolvedBy, scan::QuestionStatus};

/// At most 2 questions per run, all before the scan is `planned`.
pub const MAX_QUESTIONS_PER_RUN: u32 = 2;
/// The scheduler continues a scan without an answer after 10 minutes.
pub const CONTINUE_AFTER: Duration = Duration::from_mins(10);

/// The stored question states: `open` and `answering` are what the app sees, the rest record
/// how it resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestionState {
    /// Waiting for an answer.
    Open,
    /// A photo for it is being analysed.
    Answering,
    /// Answered by the user or a photo.
    Answered,
    /// A new run, or the scan failing, superseded it.
    Superseded,
    /// The photo sent for it couldn't be analysed.
    Dropped,
}

impl QuestionState {
    /// The stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answering => "answering",
            Self::Answered => "answered",
            Self::Superseded => "superseded",
            Self::Dropped => "dropped",
        }
    }

    /// What the app sees, while the question is unresolved.
    pub const fn wire(self) -> Option<QuestionStatus> {
        match self {
            Self::Open => Some(QuestionStatus::Open),
            Self::Answering => Some(QuestionStatus::Answering),
            Self::Answered | Self::Superseded | Self::Dropped => None,
        }
    }

    /// An indicator answer or a photo is accepted only while `open`; `answered`, `superseded`
    /// and `answering` get `409 invalid_state`.
    pub const fn accepts_answer(self) -> bool {
        matches!(self, Self::Open)
    }

    /// The state after a resolution.
    pub const fn resolved(by: ResolvedBy) -> Self {
        match by {
            ResolvedBy::User | ResolvedBy::Photo => Self::Answered,
            ResolvedBy::Superseded => Self::Superseded,
            ResolvedBy::Dropped => Self::Dropped,
        }
    }

    /// The question after its photo answer's `verify` job failed for good
    /// ([`crate::rules::jobs::final_failure`]; [contracts 5], attempts): late work (the scan is
    /// `diagnosed` or `planned`) drops the question and the scan stays; in a run, the `verify`
    /// job is a run's job, so the scan fails and the failure supersedes the question in the
    /// same transaction ([contracts 3]: a `failed` scan never holds a question). It is never
    /// reopened. A failed segmentation-only job changes nothing here: the photo goes on to
    /// `verify` unmeasured.
    pub const fn after_failed_photo(late: bool) -> Self {
        if late {
            Self::Dropped
        } else {
            Self::Superseded
        }
    }
}

/// A photo answer's step ([contracts 3]): `segmenting` (the perception job), `measuring`
/// (`api`'s own work), `verifying` (the `verify` job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestionStep {
    /// Its segmentation-only `perception` job is queued or running.
    Segmenting,
    /// `api` measures the photo; recovered by the sweep after 2 minutes.
    Measuring,
    /// Its `verify` job is queued or running.
    Verifying,
}

impl QuestionStep {
    /// How long `measuring` may hold before the sweep re-claims it. Job steps belong to the
    /// broker's leases instead, so they have no limit here.
    pub const MEASURING_STALE_AFTER: Duration = Duration::from_mins(2);

    /// The stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Segmenting => "segmenting",
            Self::Measuring => "measuring",
            Self::Verifying => "verifying",
        }
    }

    /// The step a photo answer starts in: segmenting when it has depth and a measurable,
    /// unanswered indicator its kind can show; otherwise straight to verifying.
    pub const fn first(needs_segmentation: bool) -> Self {
        if needs_segmentation {
            Self::Segmenting
        } else {
            Self::Verifying
        }
    }
}

/// Whether the run may ask another question: fewer than 2 asked, still `analysing` or
/// `awaiting_answer`, and the result ambiguous. Late answers and guided checks never ask.
pub const fn may_ask(asked_this_run: u32, still_in_run: bool) -> bool {
    still_in_run && asked_this_run < MAX_QUESTIONS_PER_RUN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answering_blocks_further_answers() {
        assert!(QuestionState::Open.accepts_answer());
        assert!(!QuestionState::Answering.accepts_answer());
        assert!(!QuestionState::Superseded.accepts_answer());
        assert_eq!(QuestionState::Answered.wire(), None);
        assert_eq!(
            QuestionState::resolved(ResolvedBy::Photo),
            QuestionState::Answered
        );
    }

    #[test]
    fn question_cap() {
        assert!(may_ask(0, true));
        assert!(may_ask(1, true));
        assert!(!may_ask(2, true));
        assert!(!may_ask(0, false));
    }

    #[test]
    fn photo_answer_steps() {
        assert_eq!(QuestionStep::first(true), QuestionStep::Segmenting);
        assert_eq!(QuestionStep::first(false), QuestionStep::Verifying);
        assert_eq!(
            QuestionState::after_failed_photo(true),
            QuestionState::Dropped
        );
    }

    #[test]
    fn a_failed_photo_answer_agrees_with_the_final_failure_rule() {
        use crate::{
            jobs::JobKind,
            rules::jobs::{FinalFailure, final_failure},
        };
        // Late: the verify job drops the question, the scan stays.
        assert_eq!(
            final_failure(JobKind::Verify, true, false),
            FinalFailure::DropQuestion
        );
        assert_eq!(
            QuestionState::after_failed_photo(true),
            QuestionState::Dropped
        );
        // In a run: the scan fails, which supersedes the question; it is never reopened.
        assert_eq!(
            final_failure(JobKind::Verify, false, false),
            FinalFailure::FailScan
        );
        assert_eq!(
            QuestionState::after_failed_photo(false),
            QuestionState::Superseded
        );
    }
}
