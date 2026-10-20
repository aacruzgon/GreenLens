//! The handlers, one module per area of [architecture 9]. Each handler's doc comment names its
//! role and contract section; its `#[utoipa::path]` lists its error statuses.

pub mod account;
pub mod chat;
pub mod connectors;
pub mod gardens;
pub mod internal;
pub mod media;
pub mod memories;
pub mod ops;
pub mod plants;
pub mod scans;
pub mod species;
pub mod tasks;
