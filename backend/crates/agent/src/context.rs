//! The context builder (architecture 7, conversation anchors; 8.2 step 2). The context is
//! **rebuilt from the store on every turn**: the conversation summary, the most recent messages,
//! the top memory hits and the anchor's current records, never the full transcript. A turn
//! therefore always sees the latest data (a frost task the scheduler made an hour ago).
//!
//! **Loading and rendering are separate.** [`load`] reads owned records ([`AnchorData`]) over
//! `.await`s; [`TurnContext::turn_message`] renders them synchronously through borrowed views
//! (`…View<'a>` structs of `&'a` fields), serialising each record once with no copies.
//!
//! **Where each part goes.** The summary and recent messages form the stable start of the
//! conversation; the anchor's data, the current beliefs, the memory hits and the user's message
//! go in the turn's last user message, each in a `<data>` block.
//!
//! **Beliefs.** The current beliefs of the anchor's plant and its garden (or of the garden for a
//! garden anchor) come from Agent Memory ([`crate::beliefs::current_beliefs`]), loaded at the
//! same time as the memory search and bounded by `beliefs_read_timeout`: Agent Memory off, down
//! or slow leaves the block out. They hold only shared memories, the latest value of each key,
//! and the system prompt says they win over an older memory hit that disagrees. The system prompt and tools stay byte-identical across
//! turns, so the provider's prompt cache keeps them, and nothing earlier in the conversation is
//! ever edited inside a turn's tool loop (the thinking blocks the loop replays stay valid).

use std::collections::BTreeMap;

use domain::{
    authz::{ConversationOwner, Viewer},
    chat::{Anchor, Belief, MessageRole},
    garden::{
        gardens::Garden,
        map::{GardenMap, neighbours},
        plants::{MeasurementPoint, TimelineEntry},
        tasks::TaskQuery,
    },
    id::{MessageId, PlantId},
    ports::{CellRequest, ChatMessage, ChatRole, ContentPart, Embedding, MemoryHit, StoreError},
    records::{
        diagnosis::Diagnosis,
        plan::ActionPlan,
        task::{Task, TaskState},
        weather::WeatherSummary,
    },
    scan::{Plant, ScanDetail},
    url::Stored,
    wire::PageRequest,
};
use serde::Serialize;

use crate::{
    beliefs::current_beliefs,
    deps::AgentDeps,
    prompt::{DataKind, data, data_json},
};

/// The anchor's records, owned: loaded over `.await`s, then lent to the views.
#[derive(Debug, Clone, PartialEq)]
pub enum AnchorData {
    /// "Ask about this result": the diagnosis, its observations and regions, the weather, the
    /// plan.
    Scan(Box<ScanDetail<Stored>>),
    /// The plant timeline: its scans, open tasks and growth curve.
    Plant(Box<PlantData>),
    /// Garden home: each plant's summary, the forecast, upcoming tasks and the map.
    Garden(Box<GardenData>),
}

/// A plant anchor's records.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantData {
    /// The plant.
    pub plant: Plant<Stored>,
    /// Its scans, newest first.
    pub timeline: Vec<TimelineEntry<Stored>>,
    /// Its open tasks.
    pub open_tasks: Vec<Task>,
    /// Its measured sizes, oldest first.
    pub growth: Vec<MeasurementPoint>,
    /// The garden's current weather.
    pub weather: Option<WeatherSummary>,
}

/// A garden anchor's records.
#[derive(Debug, Clone, PartialEq)]
pub struct GardenData {
    /// The garden.
    pub garden: Garden,
    /// Its plants.
    pub plants: Vec<Plant<Stored>>,
    /// Open tasks, by due date.
    pub upcoming: Vec<Task>,
    /// The current weather and forecast.
    pub weather: Option<WeatherSummary>,
    /// The current map.
    pub map: Option<GardenMap>,
}

/// Everything a turn starts from.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnContext {
    /// The conversation's anchor.
    pub anchor: Anchor,
    /// The plant the conversation is about: the scan's or the anchor's; none for a garden.
    pub plant: Option<PlantId>,
    /// The summary and recent messages, oldest first, text only.
    pub history: Vec<ChatMessage>,
    /// The anchor's records.
    pub data: AnchorData,
    /// The top memory hits: shared memories and the caller's own chat memories only.
    pub memories: Vec<MemoryHit>,
    /// The current shared beliefs of the plant and its garden, or of the garden; empty when
    /// Agent Memory is off, down or slow.
    pub beliefs: Vec<Belief>,
}

/// How many open tasks and timeline entries the context carries.
const LIST_LIMIT: u32 = 20;

/// Loads the context. `query` is the user message's embedding, for the memory hits. `current` is the user message this turn just stored, left out of the
/// history because the turn message carries it with the context.
pub async fn load(
    deps: &AgentDeps,
    owner: ConversationOwner<Viewer>,
    anchor: Anchor,
    current: MessageId,
    query: Option<&Embedding>,
) -> Result<TurnContext, StoreError> {
    let member = owner.membership();
    let s = &deps.stores;
    let recent = deps.config.recent_messages;
    let (summary, messages) = s.chats.context(owner, recent + 1).await?;

    let mut history = Vec::new();
    if let Some(summary) = summary.as_deref() {
        history.push(ChatMessage {
            role: ChatRole::User,
            content: vec![ContentPart::Text(data(DataKind::Summary, summary))],
        });
    }
    let mut messages: Vec<_> = messages
        .into_iter()
        .filter(|m| m.message.id != current && !m.message.text.trim().is_empty())
        .collect();
    messages.sort_by_key(|m| m.message.created_at);
    for m in messages.into_iter().rev().take(recent as usize).rev() {
        let (role, text) = match m.message.role {
            // Earlier user words are data, like this turn's.
            MessageRole::User => (ChatRole::User, data(DataKind::UserMessage, &m.message.text)),
            MessageRole::Assistant => (ChatRole::Assistant, m.message.text),
        };
        history.push(ChatMessage {
            role,
            content: vec![ContentPart::Text(text)],
        });
    }

    let (data, plant) = match anchor {
        Anchor::Scan(scan) => {
            let detail = s.scans.scan_detail(member, scan).await?;
            let plant = detail.plant.id;
            (AnchorData::Scan(Box::new(detail)), Some(plant))
        }
        Anchor::Plant(plant_id) => {
            let plant = s.plants.plant(member, plant_id).await?;
            let timeline = s
                .plants
                .timeline(member, plant_id, page(LIST_LIMIT))
                .await?
                .items;
            let growth = s
                .plants
                .growth_series(member, plant_id, None, page(50))
                .await?
                .items;
            let open_tasks = open_tasks(deps, owner, Some(plant_id)).await?;
            let weather = garden_weather(deps, member.garden()).await;
            let data = PlantData {
                plant,
                timeline,
                open_tasks,
                growth,
                weather,
            };
            (AnchorData::Plant(Box::new(data)), Some(plant_id))
        }
        Anchor::Garden(_) => {
            let garden = s.gardens.garden(member).await?;
            let plants = s.plants.plants(member, page(100)).await?.items;
            let upcoming = open_tasks(deps, owner, None).await?;
            let weather = garden_weather(deps, member.garden()).await;
            let map = s.maps.map(member).await?;
            let data = GardenData {
                garden,
                plants,
                upcoming,
                weather,
                map,
            };
            (AnchorData::Garden(Box::new(data)), None)
        }
    };

    // Without a query vector (the embedder failed) the turn goes on without memory hits. The
    // belief read runs beside the search; it never fails, only comes back empty.
    let search = async {
        match query {
            Some(q) => {
                deps.memory
                    .search(member, q, plant, deps.config.memory_hits)
                    .await
            }
            None => Ok(Vec::new()),
        }
    };
    let beliefs = current_beliefs(
        deps.beliefs.as_ref(),
        member.garden(),
        plant,
        deps.config.beliefs_read_timeout,
    );
    let (memories, beliefs) = tokio::join!(search, beliefs);
    let memories = memories?;

    Ok(TurnContext {
        anchor,
        plant,
        history,
        data,
        memories,
        beliefs,
    })
}

const fn page(limit: u32) -> PageRequest {
    PageRequest {
        cursor: None,
        limit: Some(limit),
    }
}

async fn open_tasks(
    deps: &AgentDeps,
    owner: ConversationOwner<Viewer>,
    plant: Option<PlantId>,
) -> Result<Vec<Task>, StoreError> {
    let member = owner.membership();
    let query = TaskQuery {
        garden_id: member.garden(),
        source: None,
        state: Some(TaskState::Open),
    };
    let tasks = deps
        .stores
        .plans
        .tasks(member, query, page(LIST_LIMIT))
        .await?
        .items;
    Ok(match plant {
        Some(p) => tasks
            .into_iter()
            .filter(|t| t.plant_id == Some(p))
            .collect(),
        None => tasks,
    })
}

/// The garden's weather for this hour: the cache, else Open-Meteo (cached for the next turn).
/// A failure leaves the weather out of the context rather than failing the turn.
pub async fn garden_weather(
    deps: &AgentDeps,
    garden: domain::id::GardenId,
) -> Option<WeatherSummary> {
    let s = &deps.stores;
    let ctx = s.gardens.garden_context(garden).await.ok()?;
    let now = deps.clock.now();
    if let Ok(Some(cached)) = s.weather_cache.cached(ctx.weather_cell, now).await {
        return Some(cached.for_garden(ctx.precision));
    }
    let request = CellRequest {
        cell: ctx.weather_cell,
        time_zone: ctx.time_zone,
    };
    let fresh = deps.weather.fetch_cell(&request).await.ok()?;
    if let Err(e) = s
        .weather_cache
        .put(ctx.weather_cell, now, fresh.clone())
        .await
    {
        tracing::warn!(error = %e, "weather cache write failed");
    }
    Some(fresh.for_garden(ctx.precision))
}

impl TurnContext {
    /// The turn's last user message: the anchor's data, the current beliefs, the memory hits,
    /// the user's images and the user's words, each untrusted part in its own `<data>` block.
    pub fn turn_message(&self, user_text: &str, images: Vec<ContentPart>) -> ChatMessage {
        let mut content = vec![ContentPart::Text(self.render())];
        if !self.beliefs.is_empty() {
            let beliefs: Vec<BeliefView<'_>> = self.beliefs.iter().map(BeliefView::new).collect();
            content.push(ContentPart::Text(data_json(DataKind::Beliefs, &beliefs)));
        }
        if !self.memories.is_empty() {
            let texts: Vec<MemoryView<'_>> = self
                .memories
                .iter()
                .map(|h| MemoryView {
                    text: &h.memory.text,
                    about_plant: h.memory.plant_id.is_some(),
                })
                .collect();
            content.push(ContentPart::Text(data_json(DataKind::Memories, &texts)));
        }
        content.extend(images);
        content.push(ContentPart::Text(data(DataKind::UserMessage, user_text)));
        ChatMessage {
            role: ChatRole::User,
            content,
        }
    }

    /// The anchor's records as one `<data kind="context">` block.
    pub fn render(&self) -> String {
        match &self.data {
            AnchorData::Scan(d) => data_json(DataKind::Context, &ScanView::new(d)),
            AnchorData::Plant(p) => data_json(DataKind::Context, &PlantView::new(p)),
            AnchorData::Garden(g) => data_json(DataKind::Context, &GardenView::new(g)),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Borrowed views: `'a` is the lifetime of the `AnchorData` being rendered
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct MemoryView<'a> {
    text: &'a str,
    about_plant: bool,
}

/// A belief as the model reads it: the key, the value and whether it is about the plant. The
/// ids and dates stay out of the prompt.
#[derive(Serialize)]
pub(crate) struct BeliefView<'a> {
    key: &'a str,
    value: &'a str,
    about_plant: bool,
}

impl<'a> BeliefView<'a> {
    pub(crate) fn new(b: &'a Belief) -> Self {
        Self {
            key: &b.key,
            value: &b.value,
            about_plant: b.about_plant,
        }
    }
}

#[derive(Serialize)]
struct PlantRefView<'a> {
    id: PlantId,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    species: Option<&'a str>,
    status: domain::scan::PlantStatus,
}

impl<'a> PlantRefView<'a> {
    fn new(p: &'a Plant<Stored>) -> Self {
        Self {
            id: p.id,
            name: &p.name,
            species: p
                .species
                .as_ref()
                .map(|s| s.common_name.as_str())
                .or(p.custom_species_name.as_deref()),
            status: p.status,
        }
    }
}

#[derive(Serialize)]
struct ImageRefView {
    image_id: domain::id::ImageId,
    kind: domain::scan::ImageKind,
}

#[derive(Serialize)]
struct ScanView<'a> {
    anchor: &'static str,
    scan_id: domain::id::ScanId,
    status: domain::scan::ScanStatus,
    plant: PlantRefView<'a>,
    images: Vec<ImageRefView>,
    observations: &'a [domain::records::observation::Observation<Stored>],
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnosis: Option<&'a Diagnosis<Stored>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<&'a ActionPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_question: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weather: Option<&'a WeatherSummary>,
}

impl<'a> ScanView<'a> {
    fn new(d: &'a ScanDetail<Stored>) -> Self {
        Self {
            anchor: "scan",
            scan_id: d.scan.id,
            status: d.scan.status,
            plant: PlantRefView::new(&d.plant),
            images: d
                .scan
                .images
                .iter()
                .map(|i| ImageRefView {
                    image_id: i.id,
                    kind: i.kind,
                })
                .collect(),
            observations: &d.observations,
            diagnosis: d.diagnosis.as_ref(),
            plan: d.plan.as_ref(),
            open_question: d.scan.open_question.as_ref().map(|q| q.text.as_str()),
            weather: d.weather.as_ref(),
        }
    }
}

#[derive(Serialize)]
struct PlantView<'a> {
    anchor: &'static str,
    plant: PlantRefView<'a>,
    scans: &'a [TimelineEntry<Stored>],
    open_tasks: &'a [Task],
    growth: &'a [MeasurementPoint],
    #[serde(skip_serializing_if = "Option::is_none")]
    weather: Option<&'a WeatherSummary>,
}

impl<'a> PlantView<'a> {
    fn new(p: &'a PlantData) -> Self {
        Self {
            anchor: "plant",
            plant: PlantRefView::new(&p.plant),
            scans: &p.timeline,
            open_tasks: &p.open_tasks,
            growth: &p.growth,
            weather: p.weather.as_ref(),
        }
    }
}

#[derive(Serialize)]
struct GardenView<'a> {
    anchor: &'static str,
    /// The name only: the garden's location never goes to the model.
    garden: &'a str,
    plants: Vec<PlantRefView<'a>>,
    upcoming_tasks: &'a [Task],
    #[serde(skip_serializing_if = "Option::is_none")]
    weather: Option<&'a WeatherSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<MapView<'a>>,
}

#[derive(Serialize)]
struct MapView<'a> {
    version: u32,
    beds: Vec<&'a str>,
    neighbours: BTreeMap<PlantId, Vec<PlantId>>,
}

impl<'a> GardenView<'a> {
    fn new(g: &'a GardenData) -> Self {
        Self {
            anchor: "garden",
            garden: &g.garden.name,
            plants: g.plants.iter().map(PlantRefView::new).collect(),
            upcoming_tasks: &g.upcoming,
            weather: g.weather.as_ref(),
            map: g.map.as_ref().map(|m| MapView {
                version: m.version,
                beds: m
                    .beds
                    .iter()
                    .map(|b| b.name.as_deref().unwrap_or(&b.id))
                    .collect(),
                neighbours: neighbours(&m.pins),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
