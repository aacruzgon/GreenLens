//! `show_block` and the answer blocks it builds ([contracts 8], answer blocks; design 5, chat).
//!
//! **One anchor, filled in from the store.** The model names a block's kind and one tool call of
//! the same turn, its anchor. The block takes the anchor's result as it was returned and fills
//! in the rest from the store (plant names, statuses, latest heights, species frost, problem
//! names), never from the model, so it holds records and ids only. The only thing the model
//! chooses is `focus_plant_ids`, for `garden_map`, and each must be a live plant pinned on that
//! map. A plant's frost tenderness comes only from a confirmed species, and the frost planner
//! leaves out dead plants.
//!
//! **Naming the anchor.** The model reads each call's id in the `call_id` attribute of the
//! result's `data` element, the only place it sees one. As a fallback it may write the tool's
//! name when the answer called that tool once with success; [`resolve`] maps the name to that
//! call's id, and the block carries the id.
//!
//! **A refusal sends nothing.** A call the turn doesn't hold, a call that errored, a tool name
//! the answer called several times ("called X n times"), a result that doesn't fit the kind, a
//! result with nothing to show, a fourth block or the same `(kind, call_id)` twice is a
//! `Rejected` tool error: the model reads the reason and its text answer stands alone.
//!
//! **Why the turn runs it, not the registry.** `show_block` reads what the turn holds: its
//! earlier tool calls and results, and the blocks already sent. Both live in the turn's reply,
//! which `run_tool` borrows mutably for every call, so a [`super::ToolCtx`] can't also borrow
//! them, and a snapshot in it would clone every result for every call. The turn calls [`build`]
//! with shared borrows of the two lists before it records the call, which keeps `ToolCtx` `Copy`
//! and the other tools unaware of the turn. The registry only offers its spec ([`spec`]), to
//! every role, never strict.

use std::collections::BTreeSet;

use domain::{
    chat::{
        AnswerBlock, AnswerBlockKind, BlockBody, BlockPlant, FrostPlant, GardenMapView, Lookalike,
        ProblemRef, ToolName,
    },
    garden::plants::{GrowthQuantity, MeasurementPoint},
    id::{PlantId, ProblemId, ScanId, SpeciesId},
    knowledge::Frost,
    ports::{ContentPart, PlantSummary, ToolSpec},
    records::weather::ForecastDay,
    rules::limits::MESSAGE_MAX_BLOCKS,
    scan::PlantStatus,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::{ToolCtx, read::GetPlantMeasurementsArgs, tool_name};
use crate::error::ToolError;

/// What `show_block` does, for the model.
pub const DESCRIPTION: &str = "Show an answer block under your text: a native view the app \
    builds from one earlier tool result of this answer, named by its call_id. garden_map needs \
    get_garden_map; weather_hours needs get_forecast or get_scan; frost_planner needs \
    get_forecast with a frost night; measurement_trend needs get_plant_measurements; candidates \
    needs compare_lookalikes. focus_plant_ids is for garden_map only. At most 3 per answer.";

/// Arguments of `show_block`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowBlockArgs {
    /// Which block.
    pub kind: AnswerBlockKind,
    /// The earlier tool call of this answer whose result the block shows: the `call_id`
    /// attribute of its result's `data` element, or the tool's name if it ran once.
    pub call_id: String,
    /// `garden_map` only: the plants the answer is about, each pinned on the map.
    #[serde(default)]
    pub focus_plant_ids: Option<Vec<PlantId>>,
}

/// The spec the model is shown. Not strict: only the tools that propose changes are.
pub fn spec() -> ToolSpec {
    ToolSpec {
        name: tool_name(ToolName::ShowBlock).to_owned(),
        description: DESCRIPTION.to_owned(),
        parameters: serde_json::to_value(schemars::schema_for!(ShowBlockArgs))
            .unwrap_or(Value::Null),
        strict: false,
    }
}

fn refuse(reason: impl Into<String>) -> ToolError {
    ToolError::Rejected(reason.into())
}

/// The anchor: one tool call of the turn and its result.
struct Anchor<'c> {
    tool: &'c str,
    arguments: &'c Value,
    result: &'c Value,
}

/// Finds the call and its result among the turn's tool calls; refuses one the turn doesn't
/// hold and one that errored.
fn anchor<'c>(calls: &'c [ContentPart], call_id: &str) -> Result<Anchor<'c>, ToolError> {
    let call = calls.iter().find_map(|p| match p {
        ContentPart::ToolCall {
            call_id: id,
            name,
            arguments,
        } if id == call_id => Some((name.as_str(), arguments)),
        _ => None,
    });
    let result = calls.iter().find_map(|p| match p {
        ContentPart::ToolResult {
            call_id: id,
            content,
            is_error,
        } if id == call_id => Some((content, *is_error)),
        _ => None,
    });
    match (call, result) {
        (Some((tool, arguments)), Some((result, false))) => Ok(Anchor {
            tool,
            arguments,
            result,
        }),
        (Some(_), Some((_, true))) => Err(refuse(format!(
            "The call {call_id} failed, so it has nothing to show. Answer in text."
        ))),
        _ => Err(refuse(format!(
            "No earlier tool call of this answer has call_id {call_id}. Name one of this \
             answer's calls, or answer in text."
        ))),
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, ToolError> {
    T::deserialize(value).map_err(|_| refuse("That result can't be shown as a block."))
}

/// The tools a kind may be anchored on.
const fn anchors_of(kind: AnswerBlockKind) -> &'static str {
    match kind {
        AnswerBlockKind::GardenMap => "get_garden_map",
        AnswerBlockKind::WeatherHours => "get_forecast or get_scan",
        AnswerBlockKind::FrostPlanner => "get_forecast",
        AnswerBlockKind::MeasurementTrend => "get_plant_measurements",
        AnswerBlockKind::Candidates => "compare_lookalikes",
    }
}

/// `get_scan`'s result, as far as `weather_hours` reads it.
#[derive(Deserialize)]
struct ScanWeather {
    id: ScanId,
    #[serde(default)]
    weather: Option<ScanForecast>,
}

#[derive(Deserialize)]
struct ScanForecast {
    forecast: Vec<ForecastDay>,
}

/// `compare_lookalikes`' arguments, as far as `candidates` reads them.
#[derive(Deserialize)]
struct LookalikesArgs {
    disease: ProblemId,
}

/// Builds the block, or refuses it with a reason for the model. `calls` are the turn's tool
/// calls and results so far; `sent` the blocks it has already sent.
pub async fn build(
    ctx: &ToolCtx<'_>,
    calls: &[ContentPart],
    sent: &[AnswerBlock],
    args: ShowBlockArgs,
) -> Result<AnswerBlock, ToolError> {
    let kind = args.kind;
    if sent.len() >= MESSAGE_MAX_BLOCKS {
        return Err(refuse(format!(
            "This answer already shows {MESSAGE_MAX_BLOCKS} blocks, the most it can. Answer in text."
        )));
    }
    let call_id = resolve(calls, &args.call_id)?;
    if sent
        .iter()
        .any(|b| b.kind() == kind && b.call_id == call_id)
    {
        return Err(refuse(format!(
            "This answer already shows a {} block of {call_id}.",
            kind.as_str(),
        )));
    }
    let focus = args.focus_plant_ids.unwrap_or_default();
    if kind != AnswerBlockKind::GardenMap && !focus.is_empty() {
        return Err(refuse("focus_plant_ids is for garden_map only."));
    }
    let anchor = anchor(calls, &call_id)?;
    let wrong_anchor = || {
        refuse(format!(
            "A {} block is built from {}, not from {}.",
            kind.as_str(),
            anchors_of(kind),
            anchor.tool
        ))
    };
    let tool = anchor.tool;
    let body = match kind {
        AnswerBlockKind::GardenMap if tool == tool_name(ToolName::GetGardenMap) => {
            garden_map(ctx, parse(anchor.result)?, focus).await?
        }
        AnswerBlockKind::WeatherHours if tool == tool_name(ToolName::GetForecast) => {
            let days: Vec<ForecastDay> = parse(anchor.result)?;
            if days.is_empty() {
                return Err(refuse("That forecast has no days to show."));
            }
            BlockBody::WeatherHours {
                days,
                scan_id: None,
            }
        }
        AnswerBlockKind::WeatherHours if tool == tool_name(ToolName::GetScan) => {
            let scan: ScanWeather = parse(anchor.result)?;
            let days = scan.weather.map(|w| w.forecast).unwrap_or_default();
            if days.is_empty() {
                return Err(refuse("That scan has no weather snapshot to show."));
            }
            BlockBody::WeatherHours {
                days,
                scan_id: Some(scan.id),
            }
        }
        AnswerBlockKind::FrostPlanner if tool == tool_name(ToolName::GetForecast) => {
            frost_planner(ctx, parse(anchor.result)?).await?
        }
        AnswerBlockKind::MeasurementTrend if tool == tool_name(ToolName::GetPlantMeasurements) => {
            measurement_trend(ctx, parse(anchor.arguments)?, parse(anchor.result)?).await?
        }
        AnswerBlockKind::Candidates if tool == tool_name(ToolName::CompareLookalikes) => {
            candidates(ctx, parse(anchor.arguments)?, parse(anchor.result)?).await?
        }
        _ => return Err(wrong_anchor()),
    };
    Ok(AnswerBlock { call_id, body })
}

/// The call a `show_block` names: its `call_id` as the result's `data` block gives it, or, as a
/// fallback, the name of a tool this answer called once with success (the model can't know an
/// id it was never shown, and live it named the tool: S5 checks, 2026-10-24). A tool called
/// several times is refused, so the block is never built from a guess.
fn resolve(calls: &[ContentPart], named: &str) -> Result<String, ToolError> {
    let is_call =
        |p: &ContentPart| matches!(p, ContentPart::ToolCall { call_id, .. } if call_id == named);
    if calls.iter().any(is_call) {
        return Ok(named.to_owned());
    }
    let ok = |id: &str| {
        calls.iter().any(|p| {
            matches!(p, ContentPart::ToolResult { call_id, is_error: false, .. } if call_id == id)
        })
    };
    let ids: Vec<&str> = calls
        .iter()
        .filter_map(|p| match p {
            ContentPart::ToolCall { call_id, name, .. } if name == named && ok(call_id) => {
                Some(call_id.as_str())
            }
            _ => None,
        })
        .collect();
    match ids.as_slice() {
        [one] => Ok((*one).to_owned()),
        // No successful call: the only call of that tool, if it errored, so the refusal says
        // it failed; otherwise the name, which the anchor lookup refuses as unknown.
        [] => {
            let all: Vec<&str> = calls
                .iter()
                .filter_map(|p| match p {
                    ContentPart::ToolCall { call_id, name, .. } if name == named => {
                        Some(call_id.as_str())
                    }
                    _ => None,
                })
                .collect();
            Ok(match all.as_slice() {
                [only] => (*only).to_owned(),
                _ => named.to_owned(),
            })
        }
        several => Err(refuse(format!(
            "This answer called {named} {} times: name one by the call_id of its result.",
            several.len()
        ))),
    }
}

async fn garden_map(
    ctx: &ToolCtx<'_>,
    view: GardenMapView,
    focus: Vec<PlantId>,
) -> Result<BlockBody, ToolError> {
    let Some(map) = view.map else {
        return Err(refuse("The garden has no map to show."));
    };
    let pinned: BTreeSet<PlantId> = map.pins.iter().map(|p| p.plant_id).collect();
    // Every pinned live plant; a pin of a plant being deleted has no summary.
    let plants: Vec<BlockPlant> = summaries(ctx)
        .await?
        .into_iter()
        .filter(|s| pinned.contains(&s.plant.plant_id))
        .map(|s| s.plant)
        .collect();
    // The focus is checked against those plants and kept once each, first seen first.
    let mut focus_plant_ids: Vec<PlantId> = Vec::with_capacity(focus.len());
    for id in focus {
        if !plants.iter().any(|p| p.plant_id == id) {
            return Err(refuse(format!(
                "The plant {id} is not a live plant pinned on the map. Focus only plants on it."
            )));
        }
        if !focus_plant_ids.contains(&id) {
            focus_plant_ids.push(id);
        }
    }
    Ok(BlockBody::GardenMap {
        map,
        neighbours: view.neighbours,
        plants,
        focus_plant_ids,
    })
}

async fn frost_planner(ctx: &ToolCtx<'_>, days: Vec<ForecastDay>) -> Result<BlockBody, ToolError> {
    let nights: Vec<ForecastDay> = days.into_iter().filter(|d| d.frost_risk).collect();
    if nights.is_empty() {
        return Err(refuse(
            "No day of that forecast has a frost risk, so there is no frost to plan for.",
        ));
    }
    // A dead plant needs no cover.
    let summaries: Vec<PlantSummary> = summaries(ctx)
        .await?
        .into_iter()
        .filter(|s| s.plant.status != PlantStatus::Dead)
        .collect();
    // One knowledge lookup per confirmed species (the knowledge base answers from memory). A
    // suggested species is never asked: a guess doesn't decide how a plant takes a frost.
    let species: BTreeSet<&SpeciesId> = summaries
        .iter()
        .filter_map(|s| s.confirmed_species.as_ref())
        .collect();
    let mut frost = std::collections::BTreeMap::new();
    for id in species {
        if let Some(seed) = ctx.deps.knowledge.species(id).await? {
            frost.insert(id.clone(), seed.frost);
        }
    }
    let plants = summaries
        .into_iter()
        .map(|s| {
            let tender = s
                .confirmed_species
                .as_ref()
                .and_then(|id| frost.get(id).copied().flatten())
                .map(|f| f == Frost::Tender);
            FrostPlant {
                plant: s.plant,
                frost_tender: tender,
            }
        })
        .collect();
    Ok(BlockBody::FrostPlanner { nights, plants })
}

async fn measurement_trend(
    ctx: &ToolCtx<'_>,
    args: GetPlantMeasurementsArgs,
    points: Vec<MeasurementPoint>,
) -> Result<BlockBody, ToolError> {
    let Some(first) = points.first() else {
        return Err(refuse("That plant has no measurements to show."));
    };
    let quantity: GrowthQuantity = match args.quantity {
        Some(q) => q.into(),
        None if points.iter().all(|p| p.quantity == first.quantity) => first.quantity,
        None => {
            return Err(refuse(
                "Those points mix height and canopy width. Call get_plant_measurements with \
                 one quantity first.",
            ));
        }
    };
    let plant = ctx
        .deps
        .stores
        .plants
        .plant(ctx.owner.membership(), args.plant_id)
        .await?;
    Ok(BlockBody::MeasurementTrend {
        plant_id: args.plant_id,
        plant_name: plant.name,
        quantity,
        points,
    })
}

async fn candidates(
    ctx: &ToolCtx<'_>,
    args: LookalikesArgs,
    lookalikes: Vec<Lookalike>,
) -> Result<BlockBody, ToolError> {
    if lookalikes.is_empty() {
        return Err(refuse("That problem has no look-alikes to show."));
    }
    let name = ctx
        .deps
        .knowledge
        .problem(&args.disease)
        .await?
        .ok_or_else(|| refuse(format!("Unknown problem {}.", args.disease)))?
        .name;
    Ok(BlockBody::Candidates {
        disease: ProblemRef {
            problem_id: args.disease,
            name,
        },
        lookalikes,
    })
}

async fn summaries(ctx: &ToolCtx<'_>) -> Result<Vec<PlantSummary>, ToolError> {
    Ok(ctx
        .deps
        .stores
        .plants
        .plant_summaries(ctx.owner.membership())
        .await?)
}
