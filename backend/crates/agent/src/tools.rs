//! The agent's tools (architecture 7, tools; [contracts 8], tool return shapes and scope).
//!
//! **Typed tools, erased once.** Each tool is a type implementing [`Tool`] with an associated
//! `Args` type: `serde` parses the model's JSON into it, `schemars` derives the JSON Schema the
//! model is shown from it, and `run` receives it typed. The trait is generic over its arguments
//! through the associated type and returns `impl Future + Send`, so it is not dyn compatible
//! (Reference, dyn compatibility: no associated consts, no `impl Trait` returns). The registry
//! needs one list of heterogeneous tools, so [`Erased<T>`] wraps each tool and implements the
//! dyn-compatible [`DynTool`], whose `call` takes `serde_json::Value` and returns a boxed future.
//! The JSON is parsed into `T::Args` inside the box, where the type is known again.
//!
//! **Identity from the session, never from the model** (architecture 11). Every tool runs as the
//! session user through [`ToolCtx::owner`], a `ConversationOwner<Viewer>` proof; garden-scoped
//! reads pass `owner.membership()` to the store, so a record outside the garden is `NotFound`.
//! Arguments never name the garden. Write tools upgrade the proof with `require::<Editor>()`.
//!
//! **Dose check before running.** [`Tool::model_strings`] lends every string the model wrote
//! into the call; [`Erased::call`] checks them all before `run`, so a failing call creates no
//! card, task, memory or event, and the model gets the reason as an `is_error` result.

pub mod read;
pub mod revise;
pub mod ui;
pub mod write;

use std::future::Future;

use domain::{
    authz::{ConversationOwner, Viewer},
    chat::{Anchor, Memory, PendingAction, RescanRequest, ToolName},
    id::{MessageId, PlantId},
    ports::{ContentPart, ToolSpec},
    records::diagnosis::Citation,
    wire::{Role, Timestamp},
};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{deps::AgentDeps, error::ToolError, safety::dose_check_all};

/// What a tool does, which decides who is offered it ([contracts 1], roles and access).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// Reads records; every role.
    Read,
    /// A UI action (`request_rescan`); every role, the app gates the button.
    Ui,
    /// Writes the caller's own private memory (`remember`); every role.
    Memory,
    /// Proposes a change as a confirmation card; editors and owners only.
    Write,
}

impl ToolKind {
    /// Whether a member with this role is offered tools of this kind: viewers and reviewers
    /// get the read tools, `request_rescan` and `remember`, never a confirmation card.
    pub fn offered_to(self, role: Role) -> bool {
        match self {
            Self::Read | Self::Ui | Self::Memory => true,
            Self::Write => role >= Role::Editor,
        }
    }
}

/// What a tool call runs with, borrowed from the turn.
///
/// `'t` is the turn's lifetime: the turn task owns `Arc<AgentDeps>` and lends `&AgentDeps` to
/// every call. `ToolCtx` is covariant in `'t`, so a `&'a ToolCtx<'t>` coerces to
/// `&'a ToolCtx<'a>` wherever a shorter borrow is asked for ([`DynTool::call`]).
#[derive(Debug, Clone, Copy)]
pub struct ToolCtx<'t> {
    /// The ports.
    pub deps: &'t AgentDeps,
    /// The session user's proof: owner of the conversation, member of its garden.
    pub owner: ConversationOwner<Viewer>,
    /// The conversation's anchor.
    pub anchor: Anchor,
    /// The plant the conversation is about, if any.
    pub plant: Option<PlantId>,
    /// The assistant message being written: cards and memories link to it.
    pub message_id: MessageId,
    /// Now, from the injected clock, once per turn.
    pub now: Timestamp,
}

/// A side effect the turn turns into an SSE event and, for citations and cards, into the stored
/// message.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolEffect {
    /// A source the answer may cite: `citation`.
    Citation(Citation),
    /// A confirmation card: `pending_action`.
    PendingAction(Box<PendingAction>),
    /// A private chat memory: `memory_saved`.
    MemorySaved(Memory),
    /// A photo request: `rescan_requested`.
    RescanRequested(RescanRequest),
}

/// A tool's result.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolOutput {
    /// What the model reads: the same records the app sees ([contracts 8]).
    pub content: Value,
    /// Images sent to the model after the result (`view_image`).
    pub images: Vec<ContentPart>,
    /// Side effects for the stream.
    pub effects: Vec<ToolEffect>,
}

impl ToolOutput {
    /// A result with no images or effects.
    pub fn json<T: serde::Serialize>(value: &T) -> Self {
        Self {
            content: serde_json::to_value(value).unwrap_or(Value::Null),
            ..Self::default()
        }
    }
}

/// A typed tool. Static dispatch: not object safe on purpose (see the module docs).
pub trait Tool: Send + Sync + 'static {
    /// The arguments the model writes. `DeserializeOwned` because the JSON is parsed from an
    /// owned `Value` that is dropped at once; `JsonSchema` for the schema the model is shown.
    type Args: DeserializeOwned + JsonSchema + Send + 'static;
    /// The tool's name.
    const NAME: ToolName;
    /// What it does, for the model.
    const DESCRIPTION: &'static str;
    /// The `tool_started.label` the app shows ("Checking the forecast").
    const LABEL: &'static str;
    /// Who may be offered it.
    const KIND: ToolKind;

    /// Every string the model wrote into the call, borrowed from the arguments, for the dose
    /// check. The returned references live as long as the borrow of `args` (elided `'_`), which
    /// ends before `run` takes `args` by value.
    fn model_strings(args: &Self::Args) -> Vec<&str> {
        let _ = args;
        Vec::new()
    }

    /// Runs the call. Under edition 2024 the returned future captures `self`, `ctx` and the
    /// context's own lifetime;
    /// it is `Send` so the turn task stays `Send`.
    fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: Self::Args,
    ) -> impl Future<Output = Result<ToolOutput, ToolError>> + Send;
}

/// The dyn-compatible face of a tool, for the registry.
pub trait DynTool: Send + Sync {
    /// The name.
    fn name(&self) -> ToolName;
    /// The kind.
    fn kind(&self) -> ToolKind;
    /// The UI label.
    fn label(&self) -> &'static str;
    /// The spec sent to the model.
    fn spec(&self) -> &ToolSpec;
    /// Parses, dose-checks and runs a call.
    ///
    /// One lifetime `'a` for the tool, the context and the context's own borrows: the boxed
    /// future borrows all three and must not outlive any. `'a` is spelt out because a boxed
    /// future (`BoxFuture<'a, _>` = `Pin<Box<dyn Future + Send + 'a>>`) needs a named bound,
    /// which elision can't supply for a trait-object return.
    fn call<'a>(
        &'a self,
        ctx: &'a ToolCtx<'a>,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutput, ToolError>>;
}

/// A typed tool and its precomputed spec, behind [`DynTool`].
pub struct Erased<T: Tool> {
    tool: T,
    spec: ToolSpec,
}

impl<T: Tool> Erased<T> {
    /// Wraps a tool, generating its spec from `T::Args` once.
    pub fn new(tool: T) -> Self {
        let parameters =
            serde_json::to_value(schemars::schema_for!(T::Args)).unwrap_or(Value::Null);
        Self {
            tool,
            spec: ToolSpec {
                name: tool_name(T::NAME).to_owned(),
                description: T::DESCRIPTION.to_owned(),
                parameters,
            },
        }
    }
}

impl<T: Tool> DynTool for Erased<T> {
    fn name(&self) -> ToolName {
        T::NAME
    }

    fn kind(&self) -> ToolKind {
        T::KIND
    }

    fn label(&self) -> &'static str {
        T::LABEL
    }

    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call<'a>(
        &'a self,
        ctx: &'a ToolCtx<'a>,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let args: T::Args = serde_json::from_value(arguments)
                .map_err(|e| ToolError::Rejected(format!("The arguments are invalid: {e}.")))?;
            if let Some(m) = dose_check_all(T::model_strings(&args), &ctx.deps.products) {
                return Err(ToolError::Rejected(m.reason()));
            }
            self.tool.run(ctx, args).await
        })
    }
}

/// The wire name of a tool.
pub const fn tool_name(name: ToolName) -> &'static str {
    match name {
        ToolName::GetScan => "get_scan",
        ToolName::ViewImage => "view_image",
        ToolName::GetForecast => "get_forecast",
        ToolName::SearchMemory => "search_memory",
        ToolName::FindSimilarCases => "find_similar_cases",
        ToolName::SearchKnowledge => "search_knowledge",
        ToolName::GetSpeciesProfile => "get_species_profile",
        ToolName::GetDiseaseProfile => "get_disease_profile",
        ToolName::GetOrganismProfile => "get_organism_profile",
        ToolName::CompareLookalikes => "compare_lookalikes",
        ToolName::GetSoilProfile => "get_soil_profile",
        ToolName::GetPlantMeasurements => "get_plant_measurements",
        ToolName::GetGardenMap => "get_garden_map",
        ToolName::ListTasks => "list_tasks",
        ToolName::Remember => "remember",
        ToolName::RequestRescan => "request_rescan",
        ToolName::CreateTask => "create_task",
        ToolName::CompleteTask => "complete_task",
        ToolName::RevisePlan => "revise_plan",
    }
}

/// Every tool, erased, sorted by name: built once at start-up and shared by every turn.
pub struct ToolRegistry {
    tools: Vec<Box<dyn DynTool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.tools.iter().map(|t| tool_name(t.name())))
            .finish()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::standard()
    }
}

impl ToolRegistry {
    /// The nineteen tools of architecture 7.
    pub fn standard() -> Self {
        let mut tools: Vec<Box<dyn DynTool>> = vec![
            Box::new(Erased::new(read::GetScan)),
            Box::new(Erased::new(read::ViewImage)),
            Box::new(Erased::new(read::GetForecast)),
            Box::new(Erased::new(read::SearchMemory)),
            Box::new(Erased::new(read::FindSimilarCases)),
            Box::new(Erased::new(read::SearchKnowledge)),
            Box::new(Erased::new(read::GetSpeciesProfile)),
            Box::new(Erased::new(read::GetDiseaseProfile)),
            Box::new(Erased::new(read::GetOrganismProfile)),
            Box::new(Erased::new(read::CompareLookalikes)),
            Box::new(Erased::new(read::GetSoilProfile)),
            Box::new(Erased::new(read::GetPlantMeasurements)),
            Box::new(Erased::new(read::GetGardenMap)),
            Box::new(Erased::new(read::ListTasks)),
            Box::new(Erased::new(ui::Remember)),
            Box::new(Erased::new(ui::RequestRescan)),
            Box::new(Erased::new(write::CreateTask)),
            Box::new(Erased::new(write::CompleteTask)),
            Box::new(Erased::new(revise::RevisePlan)),
        ];
        tools.sort_by_key(|t| tool_name(t.name()));
        Self { tools }
    }

    /// The specs offered to a member with this role, sorted by name (a stable tool list keeps
    /// the provider's prompt cache).
    pub fn specs_for(&self, role: Role) -> Vec<ToolSpec> {
        self.offered(role).map(|t| t.spec().clone()).collect()
    }

    /// The tools offered to this role.
    pub fn offered(&self, role: Role) -> impl Iterator<Item = &dyn DynTool> + '_ {
        self.tools
            .iter()
            .map(AsRef::as_ref)
            .filter(move |t| t.kind().offered_to(role))
    }

    /// A tool by its wire name, if offered to this role. A call to a tool the role isn't
    /// offered is answered like an unknown tool.
    pub fn find(&self, name: &str, role: Role) -> Option<&dyn DynTool> {
        self.offered(role).find(|t| tool_name(t.name()) == name)
    }
}

#[cfg(test)]
mod tests;
