//! Scan events ([contracts 4]): the broadcast hub, the per-scan writer actors that assign `seq`,
//! store and broadcast, and the resumable SSE stream.

pub mod hub;
pub mod stream;
pub mod writer;
