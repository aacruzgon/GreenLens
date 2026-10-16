//! Shared row building blocks: the literal-union enums, JSON for `FLEXIBLE` objects, and time.
//!
//! **Why rows are owned.** A row is decoded from a `surrealdb::types::Value` the SDK owns and
//! drops after `take(n)`, and `SurrealValue::from_value` takes the value by value, so a row can't
//! borrow from it: every row field is owned (`String`, `RecordId`, `Vec<_>`). Rows also cross
//! no task: they're converted into domain types inside the repository method.
//!
//! **Enums.** `domain` derives serde, not `SurrealValue`, so each stored literal union has a row
//! mirror made by [`stored_enum!`]: `#[surreal(untagged, rename_all = "snake_case")]`, because a
//! derived unit enum without `untagged` is written as `{ "open": {} }` and fails to decode the
//! stored string ✓ (research `surrealdb.md` §3). `From` both ways makes the mirror free to use.

use chrono::{DateTime, Utc};
use domain::wire::Timestamp;
use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::SurrealValue;

use crate::error::RowError;

/// Declares a row enum mirroring a domain enum variant for variant, with `From` both ways. A
/// variant may carry the stored literal when `snake_case` wouldn't produce it
/// (`About1Km = "about_1_km"`).
macro_rules! stored_enum {
    (
        $(#[$doc:meta])*
        $row:ident <=> $domain:ty { $($variant:ident $(= $rename:literal)?),+ $(,)? }
    ) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, ::surrealdb::types::SurrealValue)]
        #[surreal(untagged, rename_all = "snake_case")]
        pub enum $row {
            $(
                #[allow(missing_docs, reason = "mirrors the domain variant of the same name")]
                $(#[surreal(rename = $rename)])?
                $variant,
            )+
        }

        impl From<$domain> for $row {
            fn from(value: $domain) -> Self {
                type D = $domain;
                match value {
                    $(D::$variant => Self::$variant,)+
                }
            }
        }

        impl From<$row> for $domain {
            fn from(value: $row) -> Self {
                match value {
                    $($row::$variant => Self::$variant,)+
                }
            }
        }
    };
}

stored_enum! {
    /// `scan.status`.
    ScanStatusRow <=> domain::scan::ScanStatus {
        Pending, Queued, Analysing, AwaitingAnswer, Diagnosed, Planned, Failed,
    }
}

stored_enum! {
    /// `member_of.role` and `invite.role`.
    RoleRow <=> domain::wire::Role { Viewer, Reviewer, Editor, Owner }
}

stored_enum! {
    /// `job.kind`.
    JobKindRow <=> domain::jobs::JobKind { Perception, Vlm, Verify, Guided }
}

stored_enum! {
    /// `scan.pipeline_step`.
    PipelineStepRow <=> domain::rules::steps::PipelineStep { Retrieve, Score, Plan }
}

stored_enum! {
    /// `question.step`.
    QuestionStepRow <=> domain::rules::questions::QuestionStep { Segmenting, Measuring, Verifying }
}

stored_enum! {
    /// `garden.location_precision`.
    LocationPrecisionRow <=> domain::records::weather::LocationPrecision {
        Exact, About1Km = "about_1_km", About10Km = "about_10_km", PostalCode, City,
    }
}

stored_enum! {
    /// `garden.soil_profile_status`.
    SoilProfileStatusRow <=> domain::garden::gardens::SoilProfileStatus { Pending, Ready, Unavailable }
}

stored_enum! {
    /// `action_plan.reason`.
    PlanReasonRow <=> domain::records::plan::PlanReason {
        Initial, LateAnswer, SpeciesChanged, GuidedCheck, ChatRevision,
    }
}

stored_enum! {
    /// `task.source`.
    TaskSourceRow <=> domain::records::task::TaskSource { Plan, ForecastAlert, Chat, User }
}

stored_enum! {
    /// `task.dismissed_reason`.
    DismissedReasonRow <=> domain::records::task::DismissedReason { User, PlanSuperseded }
}

stored_enum! {
    /// `task.trigger.kind`, `plan_trigger.trigger`.
    TriggerKindRow <=> domain::records::plan::TriggerKind {
        Frost, Heat, HumidStretch, HeavyRain, AfterRain, DrySoil,
    }
}

stored_enum! {
    /// `indicator_answer.source` (schema v1.4).
    EvidenceSourceRow <=> domain::records::diagnosis::EvidenceSource { Vlm, Measurement, User }
}

stored_enum! {
    /// `memory.kind`.
    MemoryKindRow <=> domain::chat::MemoryKind { Fact, Event, Summary }
}

stored_enum! {
    /// `memory.source`.
    MemorySourceRow <=> domain::chat::MemorySource { Pipeline, Chat, User }
}

/// `job.status`: the broker's own states, not a contract enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum JobStatusRow {
    /// Waiting for a worker.
    Queued,
    /// Held under a lease.
    Leased,
    /// Completed.
    Done,
    /// Failed for good.
    Failed,
    /// Cancelled by a new run or a deletion.
    Cancelled,
}

/// `idempotency_key.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum KeyStateRow {
    /// The first request is still running.
    Running,
    /// The response is stored.
    Done,
}

/// A contract shape as the JSON a `FLEXIBLE` object stores. Timestamps inside become RFC 3339
/// strings, which is right for these blobs: queries never filter on them.
pub fn to_json<T: Serialize>(
    field: &'static str,
    value: &T,
) -> Result<serde_json::Value, RowError> {
    serde_json::to_value(value).map_err(|source| RowError::Json { field, source })
}

/// A `FLEXIBLE` object read back as its contract shape.
pub fn from_json<T: DeserializeOwned>(
    field: &'static str,
    value: serde_json::Value,
) -> Result<T, RowError> {
    serde_json::from_value(value).map_err(|source| RowError::Json { field, source })
}

/// A `Timestamp` as the `datetime` `SurrealDB` stores: bind `DateTime<Utc>`, never a formatted
/// string, which would be stored as a string ([database 2]).
pub fn dt(t: Timestamp) -> DateTime<Utc> {
    t.into_inner()
}

/// A stored `datetime` as a `Timestamp`.
pub fn ts(t: DateTime<Utc>) -> Timestamp {
    Timestamp::from(t)
}

/// A stored `int` that must fit a `u32` (versions, runs, counts).
pub fn u32_of(field: &'static str, n: i64) -> Result<u32, RowError> {
    u32::try_from(n).map_err(|_| RowError::BadField {
        field,
        problem: format!("{n} is out of range"),
    })
}
