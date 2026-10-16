//! The per-scan writer ([contracts 4], transport: one serialised writer per scan; [contracts
//! 7], re-planning: every plan write of a scan runs one at a time). The pipeline defines the
//! port; `api` implements it as a per-scan `mpsc` actor over `EventRepo::append_event` and the
//! broadcast hub, with a per-scan mutex for plan writes.
//!
//! It lives here rather than in `domain` because [`PlanGuard`] is tokio's
//! `OwnedMutexGuard`, and `domain` depends on no runtime.

use domain::{
    events::{ScanEvent, ScanEventBody},
    id::ScanId,
    ports::StoreError,
    url::Stored,
    wire::Run,
};

/// Proof that the caller holds the scan's plan lock. Dropping it releases the lock. Owned
/// (`OwnedMutexGuard`, not `MutexGuard<'a, ()>`), so it carries no borrow of the writer and can
/// be held across `.await`s inside a spawned, `'static` task.
#[derive(Debug)]
pub struct PlanGuard(
    #[expect(dead_code, reason = "held for its Drop")] tokio::sync::OwnedMutexGuard<()>,
);

impl PlanGuard {
    /// Wraps a guard of the scan's plan mutex.
    pub fn new(guard: tokio::sync::OwnedMutexGuard<()>) -> Self {
        Self(guard)
    }
}

/// The serialised writer of one scan's events and plan writes.
#[async_trait::async_trait]
pub trait ScanWriter: Send + Sync {
    /// Assigns each event its `seq`, stores it, then broadcasts it, in that order, serialised
    /// per scan; `ts` comes from `api`'s clock. Events of one call are stored in order and
    /// returned as stored. After the call, the writer checks `stream_state` once and closes
    /// live streams that reached a resting point.
    async fn emit(
        &self,
        scan: ScanId,
        run: Run,
        events: Vec<ScanEventBody<Stored>>,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError>;

    /// Waits for the scan's plan lock. Every plan write of a scan (a run's first plan, an
    /// automatic re-plan, a confirmed chat revision) holds it from reading the current plan to
    /// storing the new one.
    async fn plan_lock(&self, scan: ScanId) -> PlanGuard;
}
