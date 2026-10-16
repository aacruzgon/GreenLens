//! The plan writer as a port of the pipeline. The writer itself is `agent::plan_writer`
//! (the LLM call with one validation round-trip inside, [contracts 7], plan validation); this
//! trait lets the steps hold it as `Arc<dyn PlanAuthor>` and tests replace it.

use agent::{
    PlanWriterError,
    plan_writer::{PlanInput, PlanWriter, WrittenPlan},
};

/// Writes a plan from its input. Async and dyn compatible through `#[async_trait]`; the
/// input is borrowed for the call only (`PlanInput<'_>` borrows the step's records).
#[async_trait::async_trait]
pub trait PlanAuthor: Send + Sync {
    /// One plan, validated; failing steps are dropped and listed in `WrittenPlan::dropped`.
    async fn write_plan(&self, input: &PlanInput<'_>) -> Result<WrittenPlan, PlanWriterError>;
}

#[async_trait::async_trait]
impl PlanAuthor for PlanWriter {
    async fn write_plan(&self, input: &PlanInput<'_>) -> Result<WrittenPlan, PlanWriterError> {
        self.write(input).await
    }
}
