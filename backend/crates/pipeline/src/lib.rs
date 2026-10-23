//! The PDM `pipeline` crate: `api`'s own work between a scan's jobs ([architecture 6],
//! [architecture 8.1]; [contracts 3], steps between jobs, and recovery).
//!
//! Three steps, `retrieve`, `score` and `plan`, each started with the scan's [`StepClaim`],
//! re-reading its inputs, and committing through `StepRepo::commit_step`, which re-checks the
//! claim and hands over to the next step in the same transaction. Entry points (job results,
//! answers, the scheduler) claim first; anything long comes back to the caller as a
//! [`FollowUp`] to run on its task tracker. Events go out through the per-scan
//! [`ScanWriter`] in the order of contracts 4.
//!
//! Everything external is injected as `Arc<dyn …>` in [`PipelineDeps`], so a test builds a
//! pipeline from mocks and fakes.

use std::sync::Arc;

use domain::{
    events::ScanEventBody,
    id::ScanId,
    ports::{
        BeliefStore, BlobStore, Clock, IdGen, KnowledgeStore, MemoryStore, QuestionStepClaim,
        StepClaim, Store, TextEmbedder, WeatherProvider,
    },
    url::Stored,
    wire::{LocalDate, Run, Timestamp},
};

mod catch_up;
mod entry;
mod error;
pub mod findings;
pub mod fusion;
pub mod measure;
pub mod payloads;
mod plan_author;
pub mod questions;
pub mod replan;
mod results;
mod runs;
mod steps;
#[cfg(test)]
mod testkit;
mod writer;

pub use entry::{PhotoInfo, SubmitOutcome};
pub use error::PipelineError;
pub use measure::{ImageMeasurer, MeasureFailed, MeasureRequest, MeasuredQuantity};
pub use plan_author::PlanAuthor;
pub use writer::{PlanGuard, ScanWriter};

/// Tunables that the contracts leave to the evaluation set.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// How far the top candidate must lead the second to be `confirmed` (open question 3 of
    /// contracts 12). Default 2: one weight-2 indicator.
    pub diagnosis_margin: i32,
    /// The weather thresholds fusion reads (the humid stretch, frost and hard frost): the same
    /// instance `api` gives Open-Meteo and its triggers.
    pub weather: domain::rules::weather::WeatherThresholds,
    /// Bounds concurrent measurement on the blocking pool: one permit per core.
    pub cpu_permits: Arc<tokio::sync::Semaphore>,
    /// How long the plan step waits for the plant's current beliefs from Agent Memory before
    /// writing the plan without them. `api` sets it from the agent's `beliefs_read_timeout`.
    pub beliefs_read_timeout: std::time::Duration,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            diagnosis_margin: 2,
            weather: domain::rules::weather::WeatherThresholds::default(),
            cpu_permits: Arc::new(tokio::sync::Semaphore::new(
                std::thread::available_parallelism().map_or(2, std::num::NonZero::get),
            )),
            beliefs_read_timeout: agent::config::BELIEFS_READ_TIMEOUT,
        }
    }
}

/// Every port the pipeline calls, as trait objects (runtime-swappable adapters, dyn
/// compatible and `Send + Sync`). `api` builds it in its wiring.
#[derive(Clone)]
pub struct PipelineDeps {
    /// Every repository (`Store` is the supertrait of all of them).
    pub store: Arc<dyn Store>,
    /// The per-scan event and plan writer.
    pub writer: Arc<dyn ScanWriter>,
    /// The knowledge base.
    pub knowledge: Arc<dyn KnowledgeStore>,
    /// Open-Meteo, behind the weather cache this crate reads first.
    pub weather: Arc<dyn WeatherProvider>,
    /// Garden memory: shared memories for the plan writer, the pipeline's summary. The summary
    /// is shared, so `api`'s belief sweep records it in Agent Memory.
    pub memory: Arc<dyn MemoryStore>,
    /// Agent Memory: the plant's current beliefs for the plan writer; `NoBeliefs` when it
    /// isn't configured.
    pub beliefs: Arc<dyn BeliefStore>,
    /// Text embeddings for the pipeline's memory summary, embedded as
    /// [`EmbedMode::Document`](domain::ports::EmbedMode::Document): the pipeline only stores
    /// memories and never searches.
    pub embedder: Arc<dyn TextEmbedder>,
    /// Photos and depth maps for measurement, and a perception job's masks. Its deletes must
    /// not stop when a request is dropped ([storage 4], never cancel a delete): `api` passes
    /// its delete queue's store (`blob::DeleteQueue::queued_store`).
    pub blobs: Arc<dyn BlobStore>,
    /// The measurement geometry (`vision::measure` behind [`ImageMeasurer`]).
    pub measurer: Arc<dyn ImageMeasurer>,
    /// The plan writer (`agent::plan_writer::PlanWriter`).
    pub plan_author: Arc<dyn PlanAuthor>,
    /// Time.
    pub clock: Arc<dyn Clock>,
    /// Record keys.
    pub ids: Arc<dyn IdGen>,
    /// Tunables.
    pub config: PipelineConfig,
}

/// Work an entry point started that the caller runs next, on its task tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUp {
    /// Run the scan's steps from this claim, following hand-overs.
    Step(StepClaim),
    /// Run a photo answer's `measuring` step.
    QuestionStep(QuestionStepClaim),
}

/// An entry point's answer plus the work it handed to the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct Handled<T> {
    /// What the endpoint returns.
    pub value: T,
    /// What to spawn next.
    pub follow_up: Option<FollowUp>,
}

/// The pipeline. Cheap to share: `api` holds one in an `Arc`.
pub struct Pipeline {
    deps: PipelineDeps,
}

impl std::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipeline").finish_non_exhaustive()
    }
}

impl Pipeline {
    /// A pipeline over its ports.
    pub fn new(deps: PipelineDeps) -> Self {
        Self { deps }
    }

    fn now(&self) -> Timestamp {
        self.deps.clock.now()
    }

    /// Emits events through the scan's writer; nothing to send is not a call.
    async fn emit(
        &self,
        scan: ScanId,
        run: Run,
        events: Vec<ScanEventBody<Stored>>,
    ) -> Result<(), PipelineError> {
        if !events.is_empty() {
            self.deps.writer.emit(scan, run, events).await?;
        }
        Ok(())
    }
}

/// Today in a garden's IANA time zone; UTC when the zone doesn't parse.
pub fn local_today(now: Timestamp, time_zone: &str) -> LocalDate {
    let utc = now.into_inner();
    let date = time_zone.parse::<chrono_tz::Tz>().map_or_else(
        |_| utc.date_naive(),
        |tz| utc.with_timezone(&tz).date_naive(),
    );
    LocalDate::new(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_dates_follow_the_garden() {
        let late: Timestamp = "2026-10-19T02:30:00Z".parse().unwrap();
        assert_eq!(
            local_today(late, "America/Toronto").to_string(),
            "2026-10-18"
        );
        assert_eq!(local_today(late, "nowhere").to_string(), "2026-10-19");
    }
}
