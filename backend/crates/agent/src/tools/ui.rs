//! `request_rescan` and `remember`: offered to every role, each emitting its own event
//! ([contracts 8], `rescan_requested` and `memory_saved`).

use domain::{
    chat::{MemoryKind, RescanRequest, ToolName},
    id::PlantId,
    ports::{IdGenExt as _, NewMemory},
    scan::ImageKind,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Tool, ToolCtx, ToolEffect, ToolKind, ToolOutput};
use crate::error::ToolError;

/// `request_rescan(plant_id, photo_kind, instructions)`.
#[derive(Debug)]
pub struct RequestRescan;

/// Arguments of `request_rescan`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestRescanArgs {
    /// A plant of this garden.
    pub plant_id: PlantId,
    /// The photo wanted.
    pub photo_kind: ImageKind,
    /// What to photograph and how ("the underside of a spotted leaf, in daylight"). Dose-checked.
    pub instructions: String,
}

impl Tool for RequestRescan {
    type Args = RequestRescanArgs;
    const NAME: ToolName = ToolName::RequestRescan;
    const DESCRIPTION: &'static str = "Ask the user for a specific new photo of a plant; the app \
        offers to start a new scan.";
    const LABEL: &'static str = "Asking for a photo";
    const KIND: ToolKind = ToolKind::Ui;

    fn model_strings(args: &RequestRescanArgs) -> Vec<&str> {
        vec![&args.instructions]
    }

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: RequestRescanArgs,
    ) -> Result<ToolOutput, ToolError> {
        // Scope: the plant must be live in the conversation's garden.
        ctx.deps
            .stores
            .plants
            .plant(ctx.owner.membership(), args.plant_id)
            .await?;
        let request = RescanRequest {
            plant_id: args.plant_id,
            photo_kind: args.photo_kind,
            instructions: args.instructions,
        };
        let mut out = ToolOutput::json(&request);
        out.effects.push(ToolEffect::RescanRequested(request));
        Ok(out)
    }
}

/// `remember(fact, plant_id?)`.
#[derive(Debug)]
pub struct Remember;

/// Arguments of `remember`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RememberArgs {
    /// One short sentence in the third person ("The tomato is in a container"). Dose-checked.
    pub fact: String,
    /// The plant it is about; omit for a fact about the whole garden.
    #[serde(default)]
    pub plant_id: Option<PlantId>,
}

impl Tool for Remember {
    type Args = RememberArgs;
    const NAME: ToolName = ToolName::Remember;
    const DESCRIPTION: &'static str = "Remember a fact about a plant or the garden for later \
        conversations. It stays private to this user until they share it.";
    const LABEL: &'static str = "Remembering";
    const KIND: ToolKind = ToolKind::Memory;

    fn model_strings(args: &RememberArgs) -> Vec<&str> {
        vec![&args.fact]
    }

    async fn run(&self, ctx: &ToolCtx<'_>, args: RememberArgs) -> Result<ToolOutput, ToolError> {
        let member = ctx.owner.membership();
        if let Some(plant) = args.plant_id {
            ctx.deps.stores.plants.plant(member, plant).await?;
        }
        let memory = store_chat_memory(ctx, args.plant_id, MemoryKind::Fact, args.fact).await?;
        let mut out = ToolOutput::json(&memory);
        out.effects.push(ToolEffect::MemorySaved(memory));
        Ok(out)
    }
}

/// Embeds and stores a private chat memory of the session user (`source: "chat"`), linked to
/// the assistant message. Shared by `remember` and automatic extraction.
pub async fn store_chat_memory(
    ctx: &ToolCtx<'_>,
    plant: Option<PlantId>,
    kind: MemoryKind,
    text: String,
) -> Result<domain::chat::Memory, ToolError> {
    let deps = ctx.deps;
    let embedding = deps
        .embedder
        .embed(
            domain::ports::EmbedMode::Document,
            std::slice::from_ref(&text),
        )
        .await?
        .pop()
        .ok_or_else(|| ToolError::Rejected("Memory is unavailable right now.".to_owned()))?;
    let memory = NewMemory {
        id: deps.ids.id(),
        plant,
        kind,
        text,
        embedding,
        embedding_model: deps.embedder.model().name,
        source_msg: Some(ctx.message_id),
        created_at: ctx.now,
    };
    Ok(deps.memory.remember(ctx.owner.membership(), memory).await?)
}
