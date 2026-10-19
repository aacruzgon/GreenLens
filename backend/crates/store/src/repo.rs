//! One module per repository area of `domain::ports::store`, each implementing its trait for
//! [`SurrealStore`](crate::SurrealStore). Every statement is a `pub const &str` in the module's
//! `sql` submodule, copied from [database 4] and [database 5] where they give one; a whole
//! transaction is one constant sent as one query through [`run_tx`](crate::tx::run_tx).

pub mod access;
pub mod analysis;
pub mod chat;
pub mod deletion;
pub mod events;
pub mod farm;
pub mod gardens;
pub mod idempotency;
pub mod identity;
pub mod jobs;
pub mod knowledge;
pub mod load;
pub mod map;
pub mod memory;
pub mod plans;
pub mod plants;
pub mod scans;
pub mod steps;
pub mod weather;
