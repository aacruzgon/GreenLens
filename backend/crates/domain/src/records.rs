//! Pipeline records ([contracts 7]): what the app reads through `GET /scans/{id}` and scan
//! events, and what the pipeline stores.

pub mod diagnosis;
pub mod measurement;
pub mod observation;
pub mod plan;
pub mod task;
pub mod weather;
