//! `create_task` and `complete_task`: write tools that only propose. Each stores a
//! `proposed` [`PendingAction`] (the confirmation card) and returns it; confirming is `api`'s
//! `POST /pending-actions/{id}/confirm` ([contracts 8], pending actions). Editors and owners only.

use domain::{
    authz::Editor,
    chat::{Card, PendingAction, PendingStatus, Preview, ToolName, WriteTool},
    id::{PlantId, TaskId},
    ports::{IdGenExt as _, NewPendingAction},
    records::task::{Repeat, Task, TaskSource},
    rules::limits::PENDING_ACTION_TTL,
    wire::LocalDate,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Tool, ToolCtx, ToolEffect, ToolKind, ToolOutput};
use crate::error::ToolError;

/// Stores a card for the session user (upgraded to an editor's proof) and returns the tool
/// output with its `pending_action` effect. The card's labels are fixed by the server; only
/// `body` and the preview hold model-written text, already dose-checked.
pub async fn propose(
    ctx: &ToolCtx<'_>,
    tool: WriteTool,
    card: Card,
    preview: Preview,
) -> Result<ToolOutput, ToolError> {
    let editor = ctx.owner.require::<Editor>().map_err(|_| {
        ToolError::Rejected("Only editors and owners can change the garden.".to_owned())
    })?;
    let action = PendingAction {
        id: ctx.deps.ids.id(),
        conversation_id: ctx.owner.conversation(),
        tool,
        status: PendingStatus::Proposed,
        card,
        preview,
        expires_at: ctx.now.plus(PENDING_ACTION_TTL),
    };
    let stored = ctx
        .deps
        .stores
        .chats
        .create_pending_action(
            editor,
            NewPendingAction {
                action,
                message: ctx.message_id,
            },
        )
        .await?;
    let mut out = ToolOutput::json(&stored);
    out.effects
        .push(ToolEffect::PendingAction(Box::new(stored)));
    Ok(out)
}

/// `create_task`.
#[derive(Debug)]
pub struct CreateTask;

/// Arguments of `create_task`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskArgs {
    /// "Spray the tomato with water to knock off aphids". Dose-checked.
    pub title: String,
    /// More detail. Dose-checked.
    #[serde(default)]
    pub detail: Option<String>,
    /// The plant; null for a garden task.
    #[serde(default)]
    pub plant_id: Option<PlantId>,
    /// The due date.
    #[serde(default)]
    pub due: Option<LocalDate>,
    /// Repeat every this many days (at least 1).
    #[serde(default)]
    pub repeat_every_days: Option<u32>,
}

impl Tool for CreateTask {
    type Args = CreateTaskArgs;
    const NAME: ToolName = ToolName::CreateTask;
    const DESCRIPTION: &'static str =
        "Propose a new task. The user sees a card and decides; nothing changes until they confirm.";
    const LABEL: &'static str = "Preparing a task";
    const KIND: ToolKind = ToolKind::Write;

    fn model_strings(args: &CreateTaskArgs) -> Vec<&str> {
        let mut v = vec![args.title.as_str()];
        v.extend(args.detail.as_deref());
        v
    }

    async fn run(&self, ctx: &ToolCtx<'_>, args: CreateTaskArgs) -> Result<ToolOutput, ToolError> {
        let member = ctx.owner.membership();
        if let Some(plant) = args.plant_id {
            ctx.deps.stores.plants.plant(member, plant).await?;
        }
        if args.repeat_every_days == Some(0) {
            return Err(ToolError::Rejected(
                "repeat_every_days must be at least 1.".to_owned(),
            ));
        }
        let task = Task {
            id: ctx.deps.ids.id::<domain::id::table::Task>(),
            garden_id: member.garden(),
            plant_id: args.plant_id,
            plan_id: None,
            plan_step_id: None,
            scan_id: None,
            title: args.title.clone(),
            // A required field invites "" for none: store it as no detail.
            detail: args.detail.filter(|d| !d.trim().is_empty()),
            due: args.due,
            repeat: args.repeat_every_days.map(|every_days| Repeat {
                every_days,
                until: None,
            }),
            source: TaskSource::Chat,
            trigger: None,
            done_at: None,
            dismissed_at: None,
            dismissed_reason: None,
            created_at: ctx.now,
        };
        let card = Card {
            title: "Add a task?".to_owned(),
            body: args.title,
            confirm_label: "Add task".to_owned(),
            reject_label: "Not now".to_owned(),
        };
        propose(
            ctx,
            WriteTool::CreateTask,
            card,
            Preview::Task {
                task: Box::new(task),
            },
        )
        .await
    }
}

/// `complete_task`.
#[derive(Debug)]
pub struct CompleteTask;

/// Arguments of `complete_task`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteTaskArgs {
    /// An open task of this garden.
    pub task_id: TaskId,
}

impl Tool for CompleteTask {
    type Args = CompleteTaskArgs;
    const NAME: ToolName = ToolName::CompleteTask;
    const DESCRIPTION: &'static str =
        "Propose marking a task done. The user sees a card and decides.";
    const LABEL: &'static str = "Preparing to complete a task";
    const KIND: ToolKind = ToolKind::Write;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: CompleteTaskArgs,
    ) -> Result<ToolOutput, ToolError> {
        let mut task = ctx
            .deps
            .stores
            .plans
            .task(ctx.owner.membership(), args.task_id)
            .await?;
        if task.done_at.is_some() || task.dismissed_at.is_some() {
            return Err(ToolError::Rejected(
                "That task is already closed.".to_owned(),
            ));
        }
        task.done_at = Some(ctx.now);
        let card = Card {
            title: "Mark this task done?".to_owned(),
            body: task.title.clone(),
            confirm_label: "Mark done".to_owned(),
            reject_label: "Not now".to_owned(),
        };
        propose(
            ctx,
            WriteTool::CompleteTask,
            card,
            Preview::Task {
                task: Box::new(task),
            },
        )
        .await
    }
}
