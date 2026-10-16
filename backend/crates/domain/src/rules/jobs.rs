//! Leases, runtimes, attempts and claimable kinds ([contracts 5], leases, runs and attempts;
//! architecture 8.4).

use std::time::Duration;

use crate::{
    jobs::{Capability, JobKind},
    wire::Timestamp,
};

/// A lease lasts 2 minutes; a heartbeat listing the job extends it to 2 minutes from then.
pub const LEASE: Duration = Duration::from_mins(2);
/// Workers heartbeat every 10 seconds.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// A worker whose heartbeat is older than this has its jobs requeued.
pub const WORKER_STALE_AFTER: Duration = Duration::from_secs(30);
/// A job unclaimed this long after `queued_at` becomes claimable by the fallback worker.
pub const FALLBACK_DELAY: Duration = Duration::from_secs(15);
/// After the 3rd attempt the job fails for good.
pub const MAX_ATTEMPTS: u32 = 3;
/// Requeues up to this many are free; every later one counts as an attempt.
pub const FREE_REQUEUES: u32 = 2;

impl JobKind {
    /// `max_runtime` ([contracts 5]): perception 2 minutes, vlm 5, verify and guided 3, twice
    /// that on the fallback worker.
    pub const fn max_runtime(self, on_fallback: bool) -> Duration {
        let secs = match self {
            Self::Perception => 2 * 60,
            Self::Vlm => 5 * 60,
            Self::Verify | Self::Guided => 3 * 60,
        };
        Duration::from_secs(if on_fallback { secs * 2 } else { secs })
    }

    /// Whether the job belongs to a run (a new run cancels it); `guided` jobs belong to the
    /// scan.
    pub const fn belongs_to_run(self) -> bool {
        !matches!(self, Self::Guided)
    }
}

impl Capability {
    /// The kinds this capability claims ([contracts 5]).
    pub const fn kinds(self) -> &'static [JobKind] {
        match self {
            Self::Perception => &[JobKind::Perception],
            Self::Vlm => &[JobKind::Vlm, JobKind::Verify, JobKind::Guided],
            Self::Fallback => &JobKind::ALL,
        }
    }
}

/// The kinds a worker with these capabilities may claim, without repeats, in kind order.
pub fn claimable_kinds(caps: &[Capability]) -> Vec<JobKind> {
    let mut kinds: Vec<JobKind> = caps
        .iter()
        .flat_map(|c| c.kinds().iter().copied())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// What the claim query needs to know about the claimer ([database 4], job claim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimPolicy {
    /// The kinds it may take.
    pub kinds: Vec<JobKind>,
    /// Jobs queued at or before this are eligible: now for GPU workers, now − 15 s for the
    /// fallback.
    pub eligible_before: Timestamp,
    /// The fallback worker takes a photo answer's segmentation job only when no perception
    /// worker has heartbeated for 30 seconds.
    pub hold_question_segmentation: bool,
    /// Whether runtimes are doubled.
    pub on_fallback: bool,
}

/// The claim policy for a worker's capabilities at `now`.
pub fn claim_policy(caps: &[Capability], now: Timestamp) -> ClaimPolicy {
    let on_fallback = caps.contains(&Capability::Fallback);
    ClaimPolicy {
        kinds: claimable_kinds(caps),
        eligible_before: if on_fallback {
            now.minus(FALLBACK_DELAY)
        } else {
            now
        },
        hold_question_segmentation: on_fallback,
        on_fallback,
    }
}

/// The new lease end after a heartbeat: 2 minutes from now, never beyond
/// `claimed_at + max_runtime`.
pub fn extended_lease(now: Timestamp, claimed_at: Timestamp, max_runtime: Duration) -> Timestamp {
    now.plus(LEASE).min(claimed_at.plus(max_runtime))
}

/// `attempts` after a requeue: every requeue after the 2nd counts.
pub const fn attempts_after_requeue(attempts: u32, requeues_after: u32) -> u32 {
    if requeues_after > FREE_REQUEUES {
        attempts + 1
    } else {
        attempts
    }
}

/// Whether a job with `attempts` counted is out of attempts.
pub const fn exhausted(attempts: u32) -> bool {
    attempts >= MAX_ATTEMPTS
}

/// What happens when a job fails for good ([contracts 5]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalFailure {
    /// A run's job: the scan goes to `failed` with `processing_error`, superseding its question.
    FailScan,
    /// A late `verify` job: `question_resolved` with `dropped`; the scan stays.
    DropQuestion,
    /// A `guided` job: `guided_check_done` with a failed check; the scan stays.
    FailGuidedCheck,
    /// A question photo's segmentation job: the photo goes on to `verify` unmeasured.
    ContinueUnmeasured,
}

/// Decides what a final failure does. `late` is true while the scan is `diagnosed` or
/// `planned`; `question_segmentation` marks a perception job with `question_id`.
pub const fn final_failure(kind: JobKind, late: bool, question_segmentation: bool) -> FinalFailure {
    match kind {
        JobKind::Guided => FinalFailure::FailGuidedCheck,
        JobKind::Perception if question_segmentation => FinalFailure::ContinueUnmeasured,
        JobKind::Verify if late => FinalFailure::DropQuestion,
        JobKind::Perception | JobKind::Vlm | JobKind::Verify => FinalFailure::FailScan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtimes() {
        assert_eq!(
            JobKind::Perception.max_runtime(false),
            Duration::from_secs(120)
        );
        assert_eq!(JobKind::Vlm.max_runtime(false), Duration::from_secs(300));
        assert_eq!(JobKind::Guided.max_runtime(true), Duration::from_secs(360));
    }

    #[test]
    fn capabilities() {
        assert_eq!(
            claimable_kinds(&[Capability::Perception]),
            vec![JobKind::Perception]
        );
        assert_eq!(
            claimable_kinds(&[Capability::Vlm]),
            vec![JobKind::Vlm, JobKind::Verify, JobKind::Guided]
        );
        assert_eq!(
            claimable_kinds(&[Capability::Fallback, Capability::Vlm]).len(),
            4
        );
    }

    #[test]
    fn leases_never_pass_max_runtime() {
        let claimed: Timestamp = "2026-10-18T14:00:00Z".parse().unwrap();
        let now = claimed.plus(Duration::from_secs(60));
        assert_eq!(
            extended_lease(now, claimed, JobKind::Perception.max_runtime(false)),
            claimed.plus(Duration::from_secs(120))
        );
        assert_eq!(
            extended_lease(now, claimed, JobKind::Vlm.max_runtime(false)),
            now.plus(LEASE)
        );
    }

    #[test]
    fn fallback_policy() {
        let now: Timestamp = "2026-10-18T14:00:00Z".parse().unwrap();
        let p = claim_policy(&[Capability::Fallback], now);
        assert_eq!(p.eligible_before, now.minus(FALLBACK_DELAY));
        assert!(p.hold_question_segmentation);
        let p = claim_policy(&[Capability::Perception], now);
        assert_eq!(p.eligible_before, now);
    }

    #[test]
    fn attempts_and_final_failures() {
        assert_eq!(attempts_after_requeue(0, 1), 0);
        assert_eq!(attempts_after_requeue(0, 2), 0);
        assert_eq!(attempts_after_requeue(0, 3), 1);
        assert!(exhausted(3));
        assert_eq!(
            final_failure(JobKind::Guided, false, false),
            FinalFailure::FailGuidedCheck
        );
        assert_eq!(
            final_failure(JobKind::Verify, true, false),
            FinalFailure::DropQuestion
        );
        assert_eq!(
            final_failure(JobKind::Verify, false, false),
            FinalFailure::FailScan
        );
        assert_eq!(
            final_failure(JobKind::Perception, false, true),
            FinalFailure::ContinueUnmeasured
        );
    }
}
