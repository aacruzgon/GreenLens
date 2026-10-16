//! `api`'s steps between jobs ([contracts 3], steps between jobs, and recovery; [database 4],
//! steps between jobs).

use std::time::Duration;

/// `scan.pipeline_step`: `retrieve`, `score` or `plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineStep {
    /// After the perception result: measurement, context, retrieval, queueing the `vlm` job.
    Retrieve,
    /// Scoring, fusion and the diagnosis, then a question or the plan.
    Score,
    /// The plan writer.
    Plan,
}

impl PipelineStep {
    /// Every step.
    pub const ALL: [Self; 3] = [Self::Retrieve, Self::Score, Self::Plan];

    /// The stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retrieve => "retrieve",
            Self::Score => "score",
            Self::Plan => "plan",
        }
    }

    /// Parses the stored value.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.as_str() == s)
    }

    /// How old a claim may get before the sweep re-claims it: 2 minutes, 5 for `plan`, which
    /// waits on the LLM API.
    pub const fn stale_after(self) -> Duration {
        match self {
            Self::Retrieve | Self::Score => Duration::from_mins(2),
            Self::Plan => Duration::from_mins(5),
        }
    }
}

/// What a step's commit sets next ([contracts 3], handing over).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handover {
    /// Release the claim: a job is in flight (`retrieve`), a question was asked or nothing is
    /// left (`score`), or the plan is stored (`plan`).
    Release,
    /// Set the claim to this step with a new stamp.
    Next(PipelineStep),
}

/// The next step after a commit: `score` whenever input arrived while the step ran
/// (`rescore_pending`), else the step's own hand-over.
pub const fn handover(rescore_pending: bool, own: Handover) -> Handover {
    if rescore_pending {
        Handover::Next(PipelineStep::Score)
    } else {
        own
    }
}

/// What a `score` step decided, for its hand-over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreOutcome {
    /// A question was asked: the scan goes to `awaiting_answer`.
    AskedQuestion,
    /// A plan must be written (a run's first plan or a re-plan).
    NeedsPlan,
    /// Nothing more (a guided check with an unchanged texture).
    Done,
}

impl ScoreOutcome {
    /// The step's own hand-over.
    pub const fn handover(self) -> Handover {
        match self {
            Self::AskedQuestion | Self::Done => Handover::Release,
            Self::NeedsPlan => Handover::Next(PipelineStep::Plan),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_limits() {
        assert_eq!(PipelineStep::Score.stale_after(), Duration::from_secs(120));
        assert_eq!(PipelineStep::Plan.stale_after(), Duration::from_secs(300));
        assert_eq!(PipelineStep::parse("plan"), Some(PipelineStep::Plan));
    }

    #[test]
    fn pending_input_hands_over_to_score() {
        assert_eq!(
            handover(true, Handover::Release),
            Handover::Next(PipelineStep::Score)
        );
        assert_eq!(handover(false, Handover::Release), Handover::Release);
        assert_eq!(
            ScoreOutcome::NeedsPlan.handover(),
            Handover::Next(PipelineStep::Plan)
        );
    }
}
