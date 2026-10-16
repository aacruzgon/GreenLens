//! The PDM `domain` crate: every shape that crosses a boundary ([contracts]), the typed ids,
//! the error codes, the pure rules, the authorization proofs, the signed-URL typing and every
//! boundary trait. It depends on no other crate of the workspace and does no I/O.
//!
//! Module per contract section: [`wire`] and [`id`] (contracts 1), [`error`] (2), [`scan`] (3),
//! [`events`] (4), [`jobs`] (5), [`vlm`] (6), [`records`] (7), [`chat`] (8), [`garden`] (9),
//! [`knowledge`] (10).
//!
//! [contracts]: https://github.com/aacruzgon/GreenLens/blob/main/docs/architecture/contracts.md

pub mod authz;
pub mod chat;
pub mod error;
pub mod events;
pub mod garden;
pub mod id;
pub mod jobs;
pub mod knowledge;
pub mod ports;
pub mod prelude;
pub mod records;
pub mod rules;
pub mod scan;
mod schema_util;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod url;
pub mod vlm;
pub mod wire;

/// S0 probe: a deliberate Clippy warning (`len_zero`). Never merged.
#[must_use]
pub fn s0_clippy_probe(v: &[u8]) -> bool {
    v.len() == 0
}
