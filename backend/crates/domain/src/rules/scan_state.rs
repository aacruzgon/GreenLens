//! The scan state machine ([contracts 3], scan lifecycle; architecture 8.1).
//!
//! ```text
//! pending ──submit──▶ queued ──claimed──▶ analysing ──▶ [awaiting_answer] ──▶ diagnosed ──▶ planned
//! analysing | awaiting_answer | diagnosed | planned ──species changed──▶ analysing (new run)
//! queued | analysing | awaiting_answer | diagnosed ──fail──▶ failed ──resubmit (retryable)──▶ queued
//! ```
//!
//! Late work (a late answer or photo while `diagnosed` or `planned`, every guided check) never
//! changes the status: [`ScanTrigger::LateWork`] is allowed in those two states and returns the
//! same status.

use std::time::Duration;

use crate::{
    error::{InvalidStateDetails, InvalidStateHint},
    events::RunReason,
    scan::{ScanFailureCode, ScanStatus},
    wire::Timestamp,
};

/// What moves a scan from one status to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanTrigger {
    /// `POST /scans/{id}/submit` on a pending scan: run 1.
    Submit,
    /// `POST /scans/{id}/submit` on a failed scan; allowed only for a retryable failure.
    Resubmit {
        /// The scan's failure.
        failure: ScanFailureCode,
    },
    /// A worker leased the run's first job.
    Claimed,
    /// The `score` step asked a question (at most 2 per run, [`crate::rules::questions`]).
    QuestionAsked,
    /// The `score` step stored the run's diagnosis, or the scheduler continued without an
    /// answer.
    Diagnosed,
    /// The `plan` step stored the run's plan.
    Planned,
    /// The user picked a different species while the run was past `queued`.
    SpeciesChanged,
    /// A run's job ran out of attempts, no plant was found, the photos were unusable, or an
    /// upstream was down.
    Fail,
    /// A late answer, a late photo or a guided check: allowed while `diagnosed` or `planned`,
    /// and changes nothing.
    LateWork,
}

impl ScanTrigger {
    /// The statuses this trigger is allowed from.
    pub const fn allowed_from(self) -> &'static [ScanStatus] {
        use ScanStatus as S;
        match self {
            Self::Submit => &[S::Pending],
            Self::Resubmit { .. } => &[S::Failed],
            Self::Claimed => &[S::Queued],
            Self::QuestionAsked | Self::Diagnosed => &[S::Analysing, S::AwaitingAnswer],
            Self::Planned => &[S::Diagnosed],
            Self::SpeciesChanged => &[S::Analysing, S::AwaitingAnswer, S::Diagnosed, S::Planned],
            Self::Fail => &[S::Queued, S::Analysing, S::AwaitingAnswer, S::Diagnosed],
            Self::LateWork => &[S::Diagnosed, S::Planned],
        }
    }
}

/// The state machine refused a transition: `409 invalid_state` with the current `status` and
/// the `allowed` ones, and `hint: "new_scan"` for a resubmit of a non-retryable failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("scan in status {from:?} can't take {trigger:?}")]
pub struct TransitionError {
    /// The current status.
    pub from: ScanStatus,
    /// What was attempted.
    pub trigger: ScanTrigger,
}

impl TransitionError {
    /// The `details` of the `409`.
    pub fn details(&self) -> InvalidStateDetails {
        let hint = match self.trigger {
            ScanTrigger::Resubmit { failure } if !failure.retryable() => {
                Some(InvalidStateHint::NewScan)
            }
            _ => None,
        };
        InvalidStateDetails {
            status: Some(self.from.as_str().to_owned()),
            allowed: Some(
                self.trigger
                    .allowed_from()
                    .iter()
                    .map(|s| s.as_str().to_owned())
                    .collect(),
            ),
            hint,
            ..InvalidStateDetails::default()
        }
    }
}

impl ScanStatus {
    /// Applies a trigger: the new status, or why not.
    pub fn apply(self, trigger: ScanTrigger) -> Result<Self, TransitionError> {
        let refuse = Err(TransitionError {
            from: self,
            trigger,
        });
        if !trigger.allowed_from().contains(&self) {
            return refuse;
        }
        Ok(match trigger {
            ScanTrigger::Submit => Self::Queued,
            ScanTrigger::Resubmit { failure } => {
                if !failure.retryable() {
                    return refuse;
                }
                Self::Queued
            }
            ScanTrigger::Claimed | ScanTrigger::SpeciesChanged => Self::Analysing,
            ScanTrigger::QuestionAsked => Self::AwaitingAnswer,
            ScanTrigger::Diagnosed => Self::Diagnosed,
            ScanTrigger::Planned => Self::Planned,
            ScanTrigger::Fail => Self::Failed,
            ScanTrigger::LateWork => self,
        })
    }

    /// A scan at rest has no run work in flight: `planned` or `failed`.
    pub const fn at_rest(self) -> bool {
        matches!(self, Self::Planned | Self::Failed)
    }

    /// The sweep re-claims steps on any scan that isn't `pending` or `failed` ([contracts 3]).
    pub const fn sweepable(self) -> bool {
        !matches!(self, Self::Pending | Self::Failed)
    }
}

/// What `POST /scans/{id}/submit` does ([contracts 3], submit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitDecision {
    /// A run starts: `202 Scan`.
    StartRun(RunReason),
    /// Nothing; submit is idempotent: `200 Scan`.
    AlreadySubmitted,
}

/// Decides a submit. `failure` is the scan's failure when it is `failed`.
pub fn decide_submit(
    status: ScanStatus,
    failure: Option<ScanFailureCode>,
) -> Result<SubmitDecision, TransitionError> {
    match (status, failure) {
        (ScanStatus::Pending, _) => Ok(SubmitDecision::StartRun(RunReason::Submit)),
        (ScanStatus::Failed, Some(code)) => status
            .apply(ScanTrigger::Resubmit { failure: code })
            .map(|_| SubmitDecision::StartRun(RunReason::Resubmit)),
        (ScanStatus::Failed, None) => Err(TransitionError {
            from: status,
            trigger: ScanTrigger::Resubmit {
                failure: ScanFailureCode::ProcessingError,
            },
        }),
        _ => Ok(SubmitDecision::AlreadySubmitted),
    }
}

/// How long after its creation a scan still follows a species change ([contracts 3]; open
/// question 6 of contracts 12).
pub const SPECIES_CHANGE_WINDOW: Duration = Duration::from_hours(24);

/// What a species confirmation does to the plant's most recent scan ([contracts 3],
/// `POST /plants/{id}/species`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesFollow {
    /// Too old, failed, or the same species: nothing.
    Nothing,
    /// `pending` or `queued`: the run uses the new species when it starts.
    AtRunStart,
    /// A new run starts from `analysing`.
    NewRun,
}

/// Decides whether the most recent scan follows the species change.
pub fn species_follow(
    status: ScanStatus,
    scan_created_at: Timestamp,
    now: Timestamp,
    differs_from_working_species: bool,
) -> SpeciesFollow {
    if now.since(scan_created_at) > SPECIES_CHANGE_WINDOW || status == ScanStatus::Failed {
        return SpeciesFollow::Nothing;
    }
    match status {
        ScanStatus::Pending | ScanStatus::Queued => SpeciesFollow::AtRunStart,
        _ if differs_from_working_species => SpeciesFollow::NewRun,
        _ => SpeciesFollow::Nothing,
    }
}

/// Only a `pending` scan can be deleted ([contracts 3], `DELETE /scans/{id}`).
pub fn check_deletable(status: ScanStatus) -> Result<(), InvalidStateDetails> {
    if status == ScanStatus::Pending {
        Ok(())
    } else {
        Err(InvalidStateDetails {
            status: Some(status.as_str().to_owned()),
            allowed: Some(vec![ScanStatus::Pending.as_str().to_owned()]),
            ..InvalidStateDetails::default()
        })
    }
}

/// Feedback needs a submitted scan ([contracts 3], feedback).
pub fn check_feedback_allowed(status: ScanStatus) -> Result<(), InvalidStateDetails> {
    if status == ScanStatus::Pending {
        Err(InvalidStateDetails {
            status: Some(status.as_str().to_owned()),
            ..InvalidStateDetails::default()
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ScanStatus as S;

    #[test]
    fn happy_path() {
        let s = S::Pending.apply(ScanTrigger::Submit).unwrap();
        let s = s.apply(ScanTrigger::Claimed).unwrap();
        let s = s.apply(ScanTrigger::QuestionAsked).unwrap();
        assert_eq!(s, S::AwaitingAnswer);
        let s = s.apply(ScanTrigger::QuestionAsked).unwrap();
        let s = s.apply(ScanTrigger::Diagnosed).unwrap();
        let s = s.apply(ScanTrigger::Planned).unwrap();
        assert_eq!(s, S::Planned);
        assert_eq!(s.apply(ScanTrigger::LateWork).unwrap(), S::Planned);
    }

    #[test]
    fn species_change_restarts_from_any_post_queued_status() {
        for s in [S::Analysing, S::AwaitingAnswer, S::Diagnosed, S::Planned] {
            assert_eq!(s.apply(ScanTrigger::SpeciesChanged).unwrap(), S::Analysing);
        }
        assert!(S::Queued.apply(ScanTrigger::SpeciesChanged).is_err());
        assert!(S::Failed.apply(ScanTrigger::SpeciesChanged).is_err());
    }

    #[test]
    fn failures_and_resubmits() {
        assert!(S::Planned.apply(ScanTrigger::Fail).is_err());
        assert!(S::Pending.apply(ScanTrigger::Fail).is_err());
        let failed = S::Analysing.apply(ScanTrigger::Fail).unwrap();
        let retry = ScanTrigger::Resubmit {
            failure: ScanFailureCode::UpstreamUnavailable,
        };
        assert_eq!(failed.apply(retry).unwrap(), S::Queued);
        let no_plant = ScanTrigger::Resubmit {
            failure: ScanFailureCode::NoPlantFound,
        };
        let err = failed.apply(no_plant).unwrap_err();
        assert_eq!(err.details().hint, Some(InvalidStateHint::NewScan));
        assert_eq!(err.details().status.as_deref(), Some("failed"));
    }

    #[test]
    fn submit_decisions() {
        assert_eq!(
            decide_submit(S::Pending, None).unwrap(),
            SubmitDecision::StartRun(RunReason::Submit)
        );
        assert_eq!(
            decide_submit(S::Failed, Some(ScanFailureCode::ProcessingError)).unwrap(),
            SubmitDecision::StartRun(RunReason::Resubmit)
        );
        assert!(decide_submit(S::Failed, Some(ScanFailureCode::PhotosUnusable)).is_err());
        assert_eq!(
            decide_submit(S::Planned, None).unwrap(),
            SubmitDecision::AlreadySubmitted
        );
    }

    #[test]
    fn species_follow_window() {
        let t0: Timestamp = "2026-10-18T10:00:00Z".parse().unwrap();
        let soon = t0.plus(Duration::from_hours(1));
        let late = t0.plus(Duration::from_hours(25));
        assert_eq!(
            species_follow(S::Queued, t0, soon, true),
            SpeciesFollow::AtRunStart
        );
        assert_eq!(
            species_follow(S::Planned, t0, soon, true),
            SpeciesFollow::NewRun
        );
        assert_eq!(
            species_follow(S::Planned, t0, soon, false),
            SpeciesFollow::Nothing
        );
        assert_eq!(
            species_follow(S::Planned, t0, late, true),
            SpeciesFollow::Nothing
        );
        assert_eq!(
            species_follow(S::Failed, t0, soon, true),
            SpeciesFollow::Nothing
        );
    }

    #[test]
    fn deletion_and_feedback() {
        assert!(check_deletable(S::Pending).is_ok());
        assert!(check_deletable(S::Queued).is_err());
        assert!(check_feedback_allowed(S::Pending).is_err());
        assert!(check_feedback_allowed(S::Failed).is_ok());
    }
}
