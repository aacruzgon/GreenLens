//! The job broker's in-process half ([architecture 8.4]; [database 4], job claim): the claim
//! long-poll waits on the store's `tokio::sync::Notify`, which the store signals after every
//! write that queues or requeues a job commits (`enqueue_job`, step commits, run starts,
//! `add_image`, a retryable failure in `complete_job`, `requeue_stale`). That is the only
//! wake-up path: nothing in `api` or the pipeline signals it by hand. A 1-second fallback poll
//! covers inserts it can't see (another process, a missed wake-up).

use std::{sync::Arc, time::Duration};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use domain::{
    id::WorkerId,
    jobs::Capability,
    ports::{ClaimedJob, Clock, JobRepo, StoreError},
    rules::jobs::claim_policy,
};

/// The fallback poll interval of the long-poll.
pub const CLAIM_POLL: Duration = Duration::from_secs(1);

/// The claim long poll over the store's `Notify`.
pub struct JobBroker {
    notify: Arc<Notify>,
    jobs: Arc<dyn JobRepo>,
    clock: Arc<dyn Clock>,
}

impl JobBroker {
    /// A broker over the job repository, waiting on `notify`: the store's own `Notify`
    /// (`SurrealStore::jobs_queued`). The store calls `notify_waiters`, which wakes every
    /// waiting claim (each re-runs its claim query and one wins) and stores no permit, so a
    /// claim that starts later doesn't wake for nothing.
    pub fn new(jobs: Arc<dyn JobRepo>, clock: Arc<dyn Clock>, notify: Arc<Notify>) -> Self {
        Self {
            notify,
            jobs,
            clock,
        }
    }

    /// The long poll: claim at once if a job is eligible, else wait for a wake-up, the 1-second
    /// poll, the deadline (`None`: `204`) or shutdown (`None`, so the worker reconnects to the
    /// next process).
    ///
    /// The `Notified` future is created and `enable`d before each claim query, so a job queued
    /// between the query and the wait still wakes this claim.
    pub async fn claim(
        &self,
        worker: &WorkerId,
        caps: &[Capability],
        slots: u32,
        wait: Duration,
        shutdown: &CancellationToken,
    ) -> Result<Option<ClaimedJob>, StoreError> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let now = self.clock.now();
            if let Some(job) = self
                .jobs
                .claim_job(worker, slots, claim_policy(caps, now), now)
                .await?
            {
                return Ok(Some(job));
            }
            tokio::select! {
                () = &mut notified => {}
                () = tokio::time::sleep(CLAIM_POLL) => {}
                () = tokio::time::sleep_until(deadline) => return Ok(None),
                () = shutdown.cancelled() => return Ok(None),
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{ports::MockJobRepo, testing::FixedClock};

    #[tokio::test(start_paused = true)]
    async fn an_empty_queue_answers_none_at_the_deadline_after_polling() {
        let mut jobs = MockJobRepo::new();
        // One query at once, then one per second until the 3-second deadline.
        jobs.expect_claim_job()
            .times(3)
            .returning(|_, _, _, _| Ok(None));
        let clock = Arc::new(FixedClock::at("2026-10-18T14:00:00Z".parse().unwrap()));
        let broker = JobBroker::new(Arc::new(jobs), clock, Arc::new(Notify::new()));
        let worker: WorkerId = "rig-3070-perception-1".parse().unwrap();
        let start = tokio::time::Instant::now();
        let got = broker
            .claim(
                &worker,
                &[Capability::Perception],
                1,
                Duration::from_secs(3),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(got.is_none());
        assert_eq!(start.elapsed(), Duration::from_secs(3));
    }
}
