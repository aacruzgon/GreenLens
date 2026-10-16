//! The SSE hub ([architecture 12], operations): one `tokio::sync::broadcast` channel per scan
//! that has live subscribers, in process, which assumes a single `api` instance.
//!
//! No sender is ever cached outside the map: the writer looks the sender up under the lock for
//! every publish, and a subscriber subscribes under the same lock. So the idle sweep, which
//! removes a channel only when it has no receivers, can never leave a writer sending into a
//! channel that a new subscriber no longer listens to.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use tokio::sync::broadcast;

use domain::{events::ScanEvent, id::ScanId, url::Stored};

/// What the writer broadcasts: the stored event, and whether the scan was at rest after the
/// call that produced it ([contracts 4], when the stream closes). `rests` is set only on the last
/// event of a writer call.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    /// The stored event: image and mask ids, no URLs. Each stream signs it as it sends.
    pub event: ScanEvent<Stored>,
    /// The stream closes after sending this event.
    pub rests: bool,
}

/// One broadcast sender per scan.
#[derive(Default)]
pub struct EventHub {
    channels: Mutex<HashMap<ScanId, broadcast::Sender<Arc<Published>>>>,
}

impl EventHub {
    /// 256 events: a burst of findings fits, and a subscriber that falls further behind gets
    /// `Lagged` and replays from the database instead.
    pub const CAPACITY: usize = 256;

    fn channels(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<ScanId, broadcast::Sender<Arc<Published>>>> {
        // A panic while holding this lock leaves the map consistent (every operation is one
        // insert, lookup or retain), so a poisoned lock is still usable.
        self.channels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Subscribes to a scan's live events, creating its channel.
    pub fn subscribe(&self, scan: ScanId) -> broadcast::Receiver<Arc<Published>> {
        self.channels()
            .entry(scan)
            .or_insert_with(|| broadcast::channel(Self::CAPACITY).0)
            .subscribe()
    }

    /// Broadcasts to the scan's subscribers, if it has any. Never blocks: a full channel
    /// overwrites its oldest value and the slow receiver sees `Lagged`.
    pub fn publish(&self, scan: ScanId, published: Published) {
        if let Some(sender) = self.channels().get(&scan) {
            let _ = sender.send(Arc::new(published));
        }
    }

    /// Removes the channels nobody listens to; run every minute by the scheduler.
    pub fn sweep_idle(&self) -> usize {
        let mut channels = self.channels();
        let before = channels.len();
        channels.retain(|_, sender| sender.receiver_count() > 0);
        before - channels.len()
    }
}
