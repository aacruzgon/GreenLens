//! Chat memory (architecture 7, memory; 8.2 step 4): after a turn, the classifier decides
//! whether it holds a fact worth remembering; only then a cheap model extracts it, and it is
//! stored as a **private** chat memory of the turn's author (`source: "chat"`, `shared: false`),
//! shown with Undo and Share with garden. Long conversations are summarised by the same cheap
//! tier.
//!
//! Privacy is the store's rule (`MemoryStore::remember` writes the caller's own memory; search
//! returns `shared = true OR author = $user`) plus the plan writer's [`crate::plan_writer::
//! SharedMemories`] check: a chat memory reaches no other member and no plan until its author
//! shares it.

use std::fmt::Write as _;

use domain::{
    authz::{ConversationOwner, Viewer},
    chat::{Memory, MemoryKind, MemoryTopic, MessageRole},
    ports::{
        ChatMessage, ChatPurpose, ChatRequest, ChatRole, ClassifierTask, ContentPart, StopReason,
    },
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    deps::AgentDeps,
    error::{MemoryError, ToolError},
    prompt::{DataKind, data, extraction_system, summary_system},
    safety::dose_check,
    tools::{ToolCtx, ui::store_chat_memory},
};

/// What the extraction model returns.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct ExtractedMemory {
    /// False when nothing is worth remembering.
    pub remember: bool,
    /// The fact, one sentence in the third person.
    pub fact: String,
    /// `fact` (true now) or `event` (something done).
    pub kind: ExtractedKind,
    /// About the conversation's plant rather than the whole garden.
    pub about_plant: bool,
    /// What the fact is about; `other` when no topic fits.
    pub topic: ExtractedTopic,
}

/// `MemoryKind` without `summary`, which only the pipeline writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractedKind {
    /// Something true about the plant or garden.
    Fact,
    /// Something done.
    Event,
}

impl From<ExtractedKind> for MemoryKind {
    fn from(k: ExtractedKind) -> Self {
        match k {
            ExtractedKind::Fact => Self::Fact,
            ExtractedKind::Event => Self::Event,
        }
    }
}

/// [`MemoryTopic`] with a JSON Schema for the extraction model (the domain type derives only
/// `ToSchema`, for the API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractedTopic {
    /// How and how often it is watered.
    Watering,
    /// Fertiliser and compost.
    Feeding,
    /// Sprays, treatments and pest control.
    Treatment,
    /// Pruning, pinching and staking.
    Pruning,
    /// Sowing, planting and transplanting.
    Planting,
    /// Where it grows.
    Location,
    /// The pot or container.
    Container,
    /// Soil, mulch and pH.
    Soil,
    /// Sun and shade.
    Light,
    /// Health and symptoms.
    Health,
    /// Harvests and yield.
    Harvest,
    /// Anything else.
    Other,
}

impl From<ExtractedTopic> for MemoryTopic {
    fn from(t: ExtractedTopic) -> Self {
        match t {
            ExtractedTopic::Watering => Self::Watering,
            ExtractedTopic::Feeding => Self::Feeding,
            ExtractedTopic::Treatment => Self::Treatment,
            ExtractedTopic::Pruning => Self::Pruning,
            ExtractedTopic::Planting => Self::Planting,
            ExtractedTopic::Location => Self::Location,
            ExtractedTopic::Container => Self::Container,
            ExtractedTopic::Soil => Self::Soil,
            ExtractedTopic::Light => Self::Light,
            ExtractedTopic::Health => Self::Health,
            ExtractedTopic::Harvest => Self::Harvest,
            ExtractedTopic::Other => Self::Other,
        }
    }
}

fn transcript(user: &str, assistant: &str) -> String {
    format!(
        "{}\n{}",
        data(DataKind::Transcript, &format!("Gardener: {user}")),
        data(DataKind::Transcript, &format!("Assistant: {assistant}"))
    )
}

/// The gate, then the extraction, then a private chat memory. `Ok(None)` when nothing was worth
/// remembering, the extracted text gave a dose, or the model declined.
pub async fn after_turn(
    ctx: &ToolCtx<'_>,
    user_text: &str,
    reply_text: &str,
) -> Result<Option<Memory>, MemoryError> {
    let deps = ctx.deps;
    let exchange = transcript(user_text, reply_text);

    let scores = deps
        .classifier
        .classify(ClassifierTask::MemoryWorthy, &exchange)
        .await?;
    let score = |label: &str| {
        scores
            .iter()
            .find(|s| s.label == label)
            .map_or(0.0, |s| s.score)
    };
    if score("remember") - score("skip") < deps.config.memory_gate_margin {
        return Ok(None);
    }

    let request = ChatRequest {
        purpose: ChatPurpose::MemoryExtraction,
        system: extraction_system(),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: vec![ContentPart::Text(exchange)],
        }],
        tools: Vec::new(),
        response_schema: serde_json::to_value(schemars::schema_for!(ExtractedMemory)).ok(),
        max_tokens: deps.config.cheap_max_tokens,
    };
    let completion = deps.chat.complete(&request).await?;
    if completion.stop != StopReason::EndTurn {
        return Ok(None);
    }
    let extracted: ExtractedMemory =
        serde_json::from_str(completion.text.trim()).map_err(|_| MemoryError::Output)?;
    let fact = extracted.fact.trim();
    if !extracted.remember || fact.is_empty() || dose_check(fact, &deps.products).is_some() {
        return Ok(None);
    }
    let plant = if extracted.about_plant {
        ctx.plant
    } else {
        None
    };
    store_chat_memory(
        ctx,
        plant,
        extracted.kind.into(),
        extracted.topic.into(),
        fact.to_owned(),
    )
    .await
    .map(Some)
    .map_err(|e| match e {
        ToolError::Store(s) => MemoryError::Store(s),
        ToolError::Model(m) => MemoryError::Model(m),
        ToolError::Rejected(_) | ToolError::Blob(_) => MemoryError::Output,
    })
}

/// Whether a conversation with `count` messages just crossed a multiple of `every` (a turn
/// adds two messages).
pub const fn summary_due(count: u32, every: u32) -> bool {
    every > 0 && count >= every && count % every < 2
}

/// Summarises the conversation when it has grown by `summarise_every` messages since the last
/// multiple. Runs after the stream has closed, so it never delays the reply.
pub async fn summarise_if_due(
    deps: &AgentDeps,
    owner: ConversationOwner<Viewer>,
) -> Result<(), MemoryError> {
    let chats = &deps.stores.chats;
    let every = deps.config.summarise_every;
    if !summary_due(chats.message_count(owner).await?, every) {
        return Ok(());
    }
    let (summary, messages) = chats
        .context(owner, every + deps.config.recent_messages)
        .await?;
    let mut messages: Vec<_> = messages.into_iter().map(|m| m.message).collect();
    messages.sort_by_key(|m| m.created_at);
    let mut content = Vec::new();
    if let Some(s) = summary.as_deref() {
        content.push(ContentPart::Text(data(DataKind::Summary, s)));
    }
    let mut lines = String::new();
    for m in &messages {
        let who = match m.role {
            MessageRole::User => "Gardener",
            MessageRole::Assistant => "Assistant",
        };
        let _ = writeln!(lines, "{who}: {}", m.text);
    }
    content.push(ContentPart::Text(data(DataKind::Transcript, &lines)));
    let request = ChatRequest {
        purpose: ChatPurpose::MemoryExtraction,
        system: summary_system(),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content,
        }],
        tools: Vec::new(),
        response_schema: None,
        max_tokens: deps.config.cheap_max_tokens,
    };
    let completion = deps.chat.complete(&request).await?;
    let text = completion.text.trim();
    if completion.stop != StopReason::EndTurn
        || text.is_empty()
        || dose_check(text, &deps.products).is_some()
    {
        return Ok(());
    }
    chats.set_summary(owner, text.to_owned()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_are_due_at_each_multiple() {
        assert!(!summary_due(18, 20));
        assert!(summary_due(20, 20));
        assert!(summary_due(21, 20));
        assert!(!summary_due(22, 20));
        assert!(summary_due(40, 20));
        assert!(!summary_due(5, 0));
    }

    #[test]
    fn the_extraction_schema_is_strict() {
        let s = serde_json::to_value(schemars::schema_for!(ExtractedMemory)).unwrap();
        assert_eq!(s["additionalProperties"], false);
        assert!(s["required"].as_array().unwrap().contains(&"topic".into()));
    }

    #[test]
    fn every_extracted_topic_maps_to_its_wire_name() {
        for (wire, topic) in [
            ("watering", ExtractedTopic::Watering),
            ("feeding", ExtractedTopic::Feeding),
            ("treatment", ExtractedTopic::Treatment),
            ("pruning", ExtractedTopic::Pruning),
            ("planting", ExtractedTopic::Planting),
            ("location", ExtractedTopic::Location),
            ("container", ExtractedTopic::Container),
            ("soil", ExtractedTopic::Soil),
            ("light", ExtractedTopic::Light),
            ("health", ExtractedTopic::Health),
            ("harvest", ExtractedTopic::Harvest),
            ("other", ExtractedTopic::Other),
        ] {
            let parsed: ExtractedTopic = serde_json::from_value(wire.into()).unwrap();
            assert_eq!(parsed, topic);
            assert_eq!(MemoryTopic::from(topic).as_str(), wire);
        }
    }
}
