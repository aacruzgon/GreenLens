//! Pure rules from the contracts: no I/O, no clock reads (time is passed in), fully
//! implemented and unit-tested. Every crate that enforces a rule calls these, so the rule lives
//! in one place.

pub mod jobs;
pub mod limits;
pub mod questions;
pub mod roles;
pub mod routing;
pub mod scan_state;
pub mod stages;
pub mod steps;
pub mod stream;
pub mod uploads;
pub mod weather;
