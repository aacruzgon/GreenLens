//! The `api` binary's modules ([architecture 3]), as a library so a second binary can reuse
//! them: `main.rs` starts the server, and `backend/tools/era-eval` (docs/EON E8) runs the real
//! farm sync ([`farm_sync`]) on an embedded store. Only what those two use is public.
//!
//! [architecture 3]: https://github.com/aacruzgon/GreenLens/blob/main/docs/architecture/README.md#3-components

mod broker;
pub mod config;
mod connect;
mod deletion;
mod error;
mod events;
mod extract;
pub mod farm_sync;
mod limits;
mod media;
mod openapi;
mod push;
mod reply;
pub mod router;
mod routes;
pub mod scheduler;
pub mod sealing;
mod seams;
mod signing;
mod state;
mod uploads;
pub mod wiring;
mod workers;

#[cfg(test)]
mod tests;
