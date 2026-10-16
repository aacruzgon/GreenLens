//! Ops endpoints ([contracts 9], ops): no sign-in, no user data.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::WorkerId,
    jobs::Capability,
    wire::{Timestamp, True},
};

/// `Status.workers[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkerStatus {
    /// The worker.
    pub worker_id: WorkerId,
    /// Its capabilities.
    pub caps: Vec<Capability>,
    /// Heartbeat within 30 seconds.
    pub healthy: bool,
    /// Its last heartbeat.
    pub last_heartbeat: Timestamp,
    /// Its GPU's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub gpu: Option<String>,
}

/// `Status.queue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct QueueCounts {
    /// Jobs waiting.
    pub queued: u32,
    /// Jobs leased.
    pub leased: u32,
}

/// `GET /status` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Status {
    /// Every known worker.
    pub workers: Vec<WorkerStatus>,
    /// The queue.
    pub queue: QueueCounts,
}

/// `GET /healthz`: `200 { ok: true }` when `SurrealDB` and `RustFS` are reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct Healthz {
    /// `true`.
    pub ok: True,
}
