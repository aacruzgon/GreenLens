//! Scan events ([contracts 4]; [database 4], scan events).

use crate::{
    authz::{Membership, Viewer},
    events::{ScanEvent, ScanEventBody},
    id::ScanId,
    ports::StoreError,
    rules::stream::StreamState,
    url::Stored,
    wire::{EventSeq, Run, Timestamp},
};

/// An event to append. `seq` is assigned by the store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    /// Its run.
    pub run: Run,
    /// When it is stored.
    pub ts: Timestamp,
    /// The body, with image and mask ids only.
    pub body: ScanEventBody<Stored>,
}

/// The `scan_event` table and `scan.last_event_seq`.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait EventRepo: Send + Sync {
    /// Appends one event: `UPDATE scan SET last_event_seq += 1` and
    /// `CREATE scan_event:[scan, seq]` in one transaction, so `seq` is gap-free. Called only by
    /// the scan's serialised writer, which then broadcasts the returned event.
    async fn append_event(
        &self,
        scan: ScanId,
        event: NewEvent,
    ) -> Result<ScanEvent<Stored>, StoreError>;

    /// Replay: a key-range read of `scan_event:[scan, after + 1]..=[scan, ..]`.
    async fn events_after(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
        after: EventSeq,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError>;

    /// What the stream's closing rule needs ([contracts 4], when the stream closes).
    async fn stream_state(&self, scan: ScanId) -> Result<StreamState, StoreError>;
}
