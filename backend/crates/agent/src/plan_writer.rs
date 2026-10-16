//! The plan writer (architecture 8.1 step 8; [contracts 7], action plan): the strongest model
//! writes `ActionPlan`'s model-written fields as strict JSON ([`PlanWriterOutput`]), plan
//! validation checks them, the failures go back once, and the passing steps are returned.
//!
//! It stores nothing. `pipeline` builds the [`PlanInput`] (diagnosis, problems, treatments,
//! organisms, shared memories, constraints, neighbours), calls [`PlanWriter::write`] and
//! assembles and stores the `ActionPlan` under the scan's plan lock.

use std::sync::{Arc, LazyLock};

use domain::{
    chat::{Memory, MemorySource, Usage},
    id::{OrganismId, ProblemId, SourceId, TreatmentId},
    knowledge::{OrganismSeed, ProblemSeed, SeedOrganismRole, SpeciesSeed, TreatmentSeed},
    ports::{ChatMessage, ChatModel, ChatPurpose, ChatRequest, ChatRole, ContentPart, StopReason},
    records::{
        diagnosis::{Diagnosis, DiagnosisStatus, Risk},
        observation::OrganismRole,
        plan::{PlanConstraint, PlanNeighbour, PlanStep, PlanWriterOutput},
        weather::{ForecastDay, WeatherSummary},
    },
    scan::Question,
    url::Stored,
};
use serde::Serialize;

use crate::{
    error::{PlanWriterError, PrivateMemory},
    prompt::{DataKind, PLAN_PROMPT_VERSION, data_json, plan_system},
    safety::{PlanDraft, ProductCatalogue, RuleFailure, ValidationContext, validate_plan},
};

/// The JSON Schema of [`PlanWriterOutput`], generated once by `schemars` (contracts 1: fixed
/// shapes get their schema from the Rust type). The adapter strips keywords the API rejects.
pub static PLAN_SCHEMA: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::to_value(schemars::schema_for!(PlanWriterOutput)).unwrap_or_default()
});

/// The default output budget of one plan-writer call (thinking included).
pub const PLAN_MAX_TOKENS: u32 = 8_192;

/// Shared memories only: the plan writer's memory input, which every member will see in the
/// plan (architecture 7: "the plan writer reads only shared memories").
///
/// The only constructor checks every memory, so a private chat memory can't reach a prompt
/// whose output is shared. `'k` borrows the caller's list (from `MemoryStore::shared_for_plan`).
#[derive(Debug, Clone, Copy)]
pub struct SharedMemories<'k>(&'k [Memory]);

impl<'k> SharedMemories<'k> {
    /// Checks that every memory is shared (`pipeline` or `user`, `shared: true`).
    pub fn new(memories: &'k [Memory]) -> Result<Self, PrivateMemory> {
        match memories
            .iter()
            .find(|m| !m.shared || m.source == MemorySource::Chat)
        {
            Some(m) => Err(PrivateMemory { id: m.id }),
            None => Ok(Self(memories)),
        }
    }

    /// No memories.
    pub const fn none() -> Self {
        Self(&[])
    }

    /// The memories.
    pub const fn as_slice(&self) -> &'k [Memory] {
        self.0
    }
}

/// Everything one plan is written from, borrowed from the caller for the duration of
/// [`PlanWriter::write`].
///
/// `'k` is the lifetime of the records `pipeline` loaded for this plan write. The writer's
/// future borrows them across its `.await`s; that is sound because `pipeline` awaits `write`
/// in place (inside its per-scan plan task) rather than spawning it, so the records outlive it.
#[derive(Debug, Clone, Copy)]
pub struct PlanInput<'k> {
    /// The plant's name.
    pub plant_name: &'k str,
    /// The working species' profile, when there is one.
    pub species: Option<&'k SpeciesSeed>,
    /// The diagnosis this version is written from.
    pub diagnosis: &'k Diagnosis<Stored>,
    /// The primary problem, or for an `uncertain` diagnosis the remaining candidates; empty when
    /// healthy.
    pub problems: &'k [ProblemSeed],
    /// The treatments of those problems.
    pub treatments: &'k [TreatmentSeed],
    /// Profiles of the organisms seen and of every organism a step may target.
    pub organisms: &'k [OrganismSeed],
    /// Shared memories of the plant and the garden.
    pub memories: SharedMemories<'k>,
    /// Constraints carried forward from the current plan.
    pub constraints: &'k [PlanConstraint],
    /// Up to 5 neighbours, nearest first ([contracts 7], neighbours for the plan writer).
    pub neighbours: &'k [PlanNeighbour],
    /// The question still open, for a provisional plan.
    pub open_question: Option<&'k Question>,
    /// The scan's weather.
    pub weather: Option<&'k WeatherSummary>,
    /// The sources steps may cite.
    pub known_sources: &'k [SourceId],
}

/// A validated plan, ready for `pipeline` to assemble into an `ActionPlan`.
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenPlan {
    /// The summary.
    pub summary: String,
    /// The passing steps, renumbered, categories fixed, safety notes added.
    pub steps: Vec<PlanStep>,
    /// What to watch for.
    pub watch_for: Vec<String>,
    /// The sources the steps cite; resolve with `KnowledgeStore::citations`.
    pub citations: Vec<SourceId>,
    /// The rules the final answer still broke; their steps were dropped.
    pub dropped: Vec<RuleFailure>,
    /// The model that wrote it (`action_plan.llm_model`).
    pub llm_model: String,
    /// `action_plan.prompt_version`.
    pub prompt_version: String,
    /// Tokens over every call.
    pub usage: Usage,
}

/// The plan writer: a model and the product list, shared as `Arc`s so `api` builds one at
/// start-up and `pipeline` keeps a clone.
#[derive(Clone)]
pub struct PlanWriter {
    model: Arc<dyn ChatModel>,
    products: Arc<ProductCatalogue>,
    max_tokens: u32,
}

impl std::fmt::Debug for PlanWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanWriter")
            .field("model", &self.model.model_name(ChatPurpose::PlanWriter))
            .field("max_tokens", &self.max_tokens)
            .finish_non_exhaustive()
    }
}

impl PlanWriter {
    /// A writer over the configured model.
    pub fn new(model: Arc<dyn ChatModel>, products: Arc<ProductCatalogue>) -> Self {
        Self {
            model,
            products,
            max_tokens: PLAN_MAX_TOKENS,
        }
    }

    /// Overrides the output budget.
    #[must_use]
    pub const fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Writes and validates a plan: one call, one retry when the answer doesn't parse, one
    /// validation round-trip, then the passing steps.
    ///
    /// The future is `Send` (no lock or `!Send` value lives across an `.await`), so `pipeline`
    /// can call it through its own `#[async_trait]` trait object.
    pub async fn write(&self, input: &PlanInput<'_>) -> Result<WrittenPlan, PlanWriterError> {
        let mut request = self.request(input);
        let mut usage = Usage::default();
        let ctx = ValidationContext {
            treatments: input.treatments,
            products: &self.products,
            organisms: input.organisms,
            problems: input.problems,
            neighbours: input.neighbours,
            beneficials_seen: beneficials_seen(input.diagnosis, input.organisms),
            known_sources: input.known_sources,
        };

        let (text, parsed) = self.call(&request, &mut usage).await?;
        let (text, output) = match parsed {
            Ok(o) => (text, o),
            Err(problem) => {
                push_round(
                    &mut request,
                    text,
                    format!(
                        "Your answer did not match the required JSON schema ({problem}). Answer again with only the JSON object."
                    ),
                );
                let (text, parsed) = self.call(&request, &mut usage).await?;
                (text, parsed.map_err(PlanWriterError::Output)?)
            }
        };

        let mut validation = validate_plan(PlanDraft::from(output), &ctx);
        if !validation.is_clean() {
            push_round(&mut request, text, validation.retry_message());
            let (_, parsed) = self.call(&request, &mut usage).await?;
            if let Ok(second) = parsed {
                validation = validate_plan(PlanDraft::from(second), &ctx);
            }
        }

        Ok(WrittenPlan {
            summary: validation.summary,
            steps: validation.steps,
            watch_for: validation.watch_for,
            citations: validation.citations,
            dropped: validation.failures,
            llm_model: self.model.model_name(ChatPurpose::PlanWriter),
            prompt_version: PLAN_PROMPT_VERSION.to_owned(),
            usage,
        })
    }

    /// The first request: the fixed system prompt, then one user message with the plan's data.
    pub fn request(&self, input: &PlanInput<'_>) -> ChatRequest {
        let mut content = vec![ContentPart::Text(data_json(
            DataKind::Context,
            &PromptView::new(input),
        ))];
        if !input.memories.as_slice().is_empty() {
            let texts: Vec<&str> = input
                .memories
                .as_slice()
                .iter()
                .map(|m| m.text.as_str())
                .collect();
            content.push(ContentPart::Text(data_json(DataKind::Memories, &texts)));
        }
        content.push(ContentPart::Text(
            "Write the action plan for this plant as the JSON object the schema describes."
                .to_owned(),
        ));
        ChatRequest {
            purpose: ChatPurpose::PlanWriter,
            system: plan_system(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content,
            }],
            tools: Vec::new(),
            response_schema: Some(PLAN_SCHEMA.clone()),
            max_tokens: self.max_tokens,
        }
    }

    /// One call: the raw text and the parsed output, or why it didn't parse.
    async fn call(
        &self,
        request: &ChatRequest,
        usage: &mut Usage,
    ) -> Result<(String, Result<PlanWriterOutput, String>), PlanWriterError> {
        let completion = self.model.complete(request).await?;
        usage.tokens_in = usage.tokens_in.saturating_add(completion.usage.tokens_in);
        usage.tokens_out = usage.tokens_out.saturating_add(completion.usage.tokens_out);
        match completion.stop {
            StopReason::Refusal => return Err(PlanWriterError::Refused),
            StopReason::MaxTokens => {
                return Ok((completion.text, Err("the answer was cut off".to_owned())));
            }
            StopReason::EndTurn | StopReason::ToolUse => {}
        }
        let parsed = serde_json::from_str::<PlanWriterOutput>(completion.text.trim())
            .map_err(|e| e.to_string());
        Ok((completion.text, parsed))
    }
}

/// Appends the model's answer and the follow-up, keeping the conversation append-only.
fn push_round(request: &mut ChatRequest, answer: String, follow_up: String) {
    request.messages.push(ChatMessage {
        role: ChatRole::Assistant,
        content: vec![ContentPart::Text(answer)],
    });
    request.messages.push(ChatMessage {
        role: ChatRole::User,
        content: vec![ContentPart::Text(follow_up)],
    });
}

/// The scan found a beneficial organism: by the diagnosis's role, or by the profile's.
fn beneficials_seen(diagnosis: &Diagnosis<Stored>, organisms: &[OrganismSeed]) -> bool {
    diagnosis.organisms.iter().any(|o| {
        o.role == OrganismRole::Beneficial
            || o.organism_id.as_ref().is_some_and(|id| {
                organisms
                    .iter()
                    .any(|p| &p.id == id && p.role == SeedOrganismRole::Beneficial)
            })
    })
}

// ---------------------------------------------------------------------------------------------
// The prompt's data, as borrowed views serialised once
// ---------------------------------------------------------------------------------------------

/// What the plan writer reads, borrowing the input (`'k`): serialised straight into the prompt
/// without copying a profile.
#[derive(Serialize)]
struct PromptView<'k> {
    plant: &'k str,
    #[serde(skip_serializing_if = "Option::is_none")]
    species: Option<SpeciesView<'k>>,
    diagnosis: DiagnosisView<'k>,
    problems: Vec<ProblemView<'k>>,
    treatments: &'k [TreatmentSeed],
    organisms: Vec<OrganismView<'k>>,
    constraints: Vec<&'k str>,
    neighbours: &'k [PlanNeighbour],
    #[serde(skip_serializing_if = "Option::is_none")]
    open_question: Option<&'k str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forecast: Option<&'k [ForecastDay]>,
    sources: &'k [SourceId],
}

#[derive(Serialize)]
struct SpeciesView<'k> {
    scientific_name: &'k str,
    common_names: &'k [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    care: Option<&'k domain::knowledge::Care>,
}

#[derive(Serialize)]
struct DiagnosisView<'k> {
    status: DiagnosisStatus,
    low_evidence: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary: Option<&'k ProblemId>,
    candidates: Vec<&'k ProblemId>,
    organisms_seen: Vec<SeenView<'k>>,
    risks: &'k [Risk],
}

#[derive(Serialize)]
struct SeenView<'k> {
    name: &'k str,
    role: OrganismRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    organism_id: Option<&'k OrganismId>,
}

#[derive(Serialize)]
struct ProblemView<'k> {
    id: &'k ProblemId,
    name: &'k str,
    kind: domain::events::ProblemKind,
    caused_by: &'k [OrganismId],
    #[serde(skip_serializing_if = "Option::is_none")]
    conditions: Option<&'k str>,
    treatments: &'k [TreatmentId],
    sources: &'k [SourceId],
}

#[derive(Serialize)]
struct OrganismView<'k> {
    id: &'k OrganismId,
    name: &'k str,
    role: SeedOrganismRole,
}

impl<'k> PromptView<'k> {
    fn new(input: &PlanInput<'k>) -> Self {
        let d = input.diagnosis;
        Self {
            plant: input.plant_name,
            species: input.species.map(|s| SpeciesView {
                scientific_name: &s.scientific_name,
                common_names: &s.common_names,
                care: s.care.as_ref(),
            }),
            diagnosis: DiagnosisView {
                status: d.status,
                low_evidence: d.low_evidence,
                primary: d.primary.as_ref().map(|p| &p.problem_id),
                candidates: d.candidates.iter().map(|c| &c.problem_id).collect(),
                organisms_seen: d
                    .organisms
                    .iter()
                    .map(|o| SeenView {
                        name: &o.name,
                        role: o.role,
                        organism_id: o.organism_id.as_ref(),
                    })
                    .collect(),
                risks: &d.risks,
            },
            problems: input
                .problems
                .iter()
                .map(|p| ProblemView {
                    id: &p.id,
                    name: &p.name,
                    kind: p.kind,
                    caused_by: &p.caused_by,
                    conditions: p.conditions.notes.as_deref(),
                    treatments: &p.treatments,
                    sources: &p.sources,
                })
                .collect(),
            treatments: input.treatments,
            organisms: input
                .organisms
                .iter()
                .map(|o| OrganismView {
                    id: &o.id,
                    name: &o.name,
                    role: o.role,
                })
                .collect(),
            constraints: input.constraints.iter().map(|c| c.text.as_str()).collect(),
            neighbours: input.neighbours,
            open_question: input.open_question.map(|q| q.text.as_str()),
            forecast: input.weather.map(|w| w.forecast.as_slice()),
            sources: input.known_sources,
        }
    }
}

/// The plan's data as the model sees it, for tests and the document.
pub fn prompt_data(input: &PlanInput<'_>) -> String {
    data_json(DataKind::Context, &PromptView::new(input))
}

#[cfg(test)]
mod tests;
