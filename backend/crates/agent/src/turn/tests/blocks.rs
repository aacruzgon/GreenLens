//! `show_block` in a turn ([contracts 8], answer blocks): a block is built from one tool result
//! of the same turn and the store, sent after its anchor's `tool_finished` and stored with the
//! message; a refusal sends nothing and gives the model the reason.

use domain::{
    chat::{BlockBody, FrostPlant},
    id::PlantId,
    knowledge::SpeciesSeed,
    ports::{ContextMessage, PlantSummary},
    records::weather::CellWeather,
};

use super::*;

const TOMATO: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5A";
const LETTUCE: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5B";
const BASIL: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";
const MYSTERY: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5D";

/// The cell's weather: one forecast day per flag, from Tuesday 20 October, frost where set.
fn weather(frost: &[bool]) -> CellWeather {
    let forecast: Vec<serde_json::Value> = frost
        .iter()
        .enumerate()
        .map(|(i, &frost_risk)| {
            json!({
                "date": format!("2026-10-{}", 20 + i), "t_min_c": if frost_risk { -1.5 } else { 4.0 },
                "t_max_c": 9.0, "precip_mm": 0.0, "mean_rh_pct": 80.0, "hours_rh_over_90": 5,
                "frost_risk": frost_risk
            })
        })
        .collect();
    serde_json::from_value(json!({
        "provider": "open_meteo", "fetched_at": "2026-10-16T11:00:00Z",
        "current": {"temp_c": 6.0, "rh_pct": 70.0, "wind_kmh": 8.0, "precip_mm_last_24h": 0.0},
        "past_7_days": {"precip_mm": 3.0, "mean_rh_pct": 75.0, "hours_rh_over_90": 12},
        "forecast": forecast,
        "soil": {"moisture_0_9cm_m3m3": 0.3, "moisture_9_27cm_m3m3": 0.3, "temp_6cm_c": 8.0,
                 "et0_mm_last_24h": 1.0}
    }))
    .unwrap()
}

fn species(id: &str, frost: Option<&str>) -> SpeciesSeed {
    let mut seed = json!({
        "id": id, "scientific_name": id, "common_names": [id], "profile": "label_only",
        "classifier_supported": false, "bioclip_label": id, "sources": [], "reviewed_by": "t"
    });
    if let Some(f) = frost {
        seed["frost"] = json!(f);
    }
    serde_json::from_value(seed).unwrap()
}

fn summary(id: &str, name: &str, species: Option<&str>) -> PlantSummary {
    PlantSummary {
        plant: domain::chat::BlockPlant {
            plant_id: id.parse().unwrap(),
            name: name.to_owned(),
            status: domain::scan::PlantStatus::Healthy,
            height_cm: (id == TOMATO).then_some(42.0),
        },
        species: species.map(|s| s.parse().unwrap()),
    }
}

/// A garden with a tomato (tender), a lettuce (hardy), a basil (its species doesn't say) and a
/// plant of no known species, and the forecast `frost`; the map pins the tomato and the lettuce.
fn frost_setup(script: Vec<Vec<ChatChunk>>, frost: &'static [bool]) -> Setup {
    let mut cache = MockWeatherCacheRepo::new();
    cache
        .expect_cached()
        .returning(move |_, _| Ok(Some(weather(frost))));
    let mut plants = no_plants();
    plants.expect_plant_summaries().returning(|member| {
        assert_eq!(member.garden().to_string(), GARDEN);
        Ok(vec![
            summary(BASIL, "Basil", Some("basil")),
            summary(LETTUCE, "Lettuce", Some("lettuce")),
            summary(MYSTERY, "Mystery", None),
            summary(TOMATO, "Tomato 1", Some("tomato")),
        ])
    });
    let mut knowledge = MockKnowledgeStore::new();
    knowledge.expect_species().returning(|id| {
        Ok(Some(match id.to_string().as_str() {
            "tomato" => species("tomato", Some("tender")),
            "lettuce" => species("lettuce", Some("hardy")),
            other => species(other, None),
        }))
    });
    let mut maps = MockMapRepo::new();
    maps.expect_map().returning(|_| {
        Ok(Some(
            serde_json::from_value(json!({
                "garden_id": GARDEN, "version": 1, "beds": [],
                "pins": [{"plant_id": TOMATO, "x_m": 0.0, "y_m": 0.0},
                         {"plant_id": LETTUCE, "x_m": 0.4, "y_m": 0.0}],
                "created_at": "2026-10-10T10:00:00Z"
            }))
            .unwrap(),
        ))
    });
    Setup {
        gardens: garden_repo(true),
        weather_cache: cache,
        plants,
        knowledge,
        maps,
        ..Setup::new(script)
    }
}

fn call(id: &str, name: &str, arguments: serde_json::Value) -> ChatChunk {
    ChatChunk::ToolCall {
        call_id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    }
}

fn tool_round(calls: Vec<ChatChunk>) -> Vec<ChatChunk> {
    let mut round = calls;
    round.push(ChatChunk::Done(StopReason::ToolUse));
    round
}

fn answer(text: &str) -> Vec<ChatChunk> {
    vec![
        ChatChunk::TextDelta(text.to_owned()),
        ChatChunk::Done(StopReason::EndTurn),
    ]
}

fn blocks_of(events: &[ChatEvent]) -> Vec<&AnswerBlock> {
    events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::AnswerBlock(b) => Some(&b.block),
            _ => None,
        })
        .collect()
}

/// What the model was sent as each `show_block` result, in order: `(is_error, content)`.
fn show_block_results(model: &ScriptedModel) -> Vec<(bool, serde_json::Value)> {
    let sent = model.sent();
    let last = sent.last().unwrap();
    let mut ids = Vec::new();
    for m in &last.messages {
        for part in &m.content {
            if let ContentPart::ToolCall { call_id, name, .. } = part
                && name == "show_block"
            {
                ids.push(call_id.clone());
            }
        }
    }
    let mut out = Vec::new();
    for id in ids {
        for m in &last.messages {
            for part in &m.content {
                if let ContentPart::ToolResult {
                    call_id,
                    content,
                    is_error,
                } = part
                    && *call_id == id
                {
                    out.push((*is_error, content.clone()));
                }
            }
        }
    }
    out
}

#[tokio::test]
async fn a_frost_question_streams_a_frost_planner_after_the_forecast_and_stores_it() {
    let (agent, model, seen) = harness(frost_setup(
        vec![
            tool_round(vec![call("toolu_fc", "get_forecast", json!({"days": 7}))]),
            tool_round(vec![call(
                "toolu_show",
                "show_block",
                json!({"kind": "frost_planner", "call_id": "toolu_fc"}),
            )]),
            answer(
                "Thursday night drops below zero. Cover the tomato; the lettuce stands a light frost.",
            ),
        ],
        &[false, false, true, true],
    ));
    let stream = agent
        .start_turn(request(
            owner(Role::Viewer).await,
            "Will the frost on Thursday hurt my plants?",
        ))
        .await
        .unwrap();
    let (events, outcome) = collect(stream).await;

    // No chip for show_block: one tool_started and tool_finished, the forecast's.
    assert_eq!(
        names(&events)[..5],
        [
            "message_started",
            "tool_started",
            "tool_finished",
            "answer_block",
            "text_delta"
        ]
    );
    let ChatEvent::ToolFinished(f) = &events[2] else {
        panic!()
    };
    assert_eq!((f.call_id.as_str(), f.ok), ("toolu_fc", true));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, ChatEvent::ToolStarted(_)))
            .count(),
        1
    );

    let blocks = blocks_of(&events);
    assert_eq!(blocks.len(), 1);
    let block = blocks[0].clone();
    assert_eq!(block.call_id, "toolu_fc");
    let BlockBody::FrostPlanner { nights, plants } = &block.body else {
        panic!("{block:?}")
    };
    let dates: Vec<String> = nights.iter().map(|d| d.date.to_string()).collect();
    assert_eq!(
        dates,
        ["2026-10-22", "2026-10-23"],
        "frost nights, first first"
    );
    let tender = |id: &str| {
        plants
            .iter()
            .find(|p: &&FrostPlant| p.plant.plant_id.to_string() == id)
            .unwrap()
            .frost_tender
    };
    assert_eq!(tender(TOMATO), Some(true));
    assert_eq!(tender(LETTUCE), Some(false));
    assert_eq!(tender(BASIL), None, "a species with no frost field");
    assert_eq!(tender(MYSTERY), None, "a plant of no known species");
    assert_eq!(
        plants.len(),
        4,
        "the garden's live plants, not the map's pins"
    );
    let json = serde_json::to_value(&block).unwrap();
    let basil = json["plants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["plant_id"] == BASIL)
        .unwrap();
    assert!(basil.get("frost_tender").is_none(), "{basil}");

    // The model read a plain success, outside a data block.
    assert_eq!(
        show_block_results(&model),
        [(false, json!({"ok": true, "kind": "frost_planner"}))]
    );

    // The stored message holds the same block.
    let TurnOutcome::Completed(message) = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(message.blocks, std::slice::from_ref(&block));
    let stored = seen.stored.lock().unwrap();
    assert_eq!(stored[0].blocks, [], "the user's message has none");
    assert_eq!(stored[1].blocks, [block]);
    assert_eq!(
        stored[1].prompt_version.as_deref(),
        Some(crate::prompt::CHAT_PROMPT_VERSION)
    );
    assert_eq!(
        stored[1].tool_calls.len(),
        4,
        "both calls and both results are stored"
    );
}

#[tokio::test]
async fn a_garden_map_block_fills_its_pinned_plants_and_keeps_the_focus() {
    let (agent, model, _) = harness(frost_setup(
        vec![
            tool_round(vec![call("toolu_map", "get_garden_map", json!({}))]),
            tool_round(vec![call(
                "toolu_show",
                "show_block",
                json!({"kind": "garden_map", "call_id": "toolu_map", "focus_plant_ids": [TOMATO]}),
            )]),
            answer("The tomato is 40 cm from the lettuce."),
        ],
        &[false],
    ));
    let stream = agent
        .start_turn(request(
            owner(Role::Editor).await,
            "How far apart are they?",
        ))
        .await
        .unwrap();
    let (events, _) = collect(stream).await;
    let blocks = blocks_of(&events);
    let BlockBody::GardenMap {
        plants,
        focus_plant_ids,
        neighbours,
        ..
    } = &blocks[0].body
    else {
        panic!()
    };
    let names: Vec<&str> = plants.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Lettuce", "Tomato 1"], "only the pinned plants");
    assert_eq!(plants[1].height_cm, Some(42.0));
    assert_eq!(focus_plant_ids, &[TOMATO.parse::<PlantId>().unwrap()]);
    assert_eq!(neighbours.len(), 2);
    assert!(!show_block_results(&model)[0].0);
}

/// One refusal case: the script's calls, then what must hold.
struct Refusal {
    name: &'static str,
    frost: &'static [bool],
    history: Vec<ContextMessage>,
    calls: Vec<Vec<ChatChunk>>,
    /// Blocks sent.
    sent: usize,
    /// `is_error` of each `show_block` result, in order.
    errors: Vec<bool>,
    /// A word the refusal's reason must hold.
    reason: &'static str,
}

fn show(id: &str, kind: &str, anchor: &str) -> ChatChunk {
    call(id, "show_block", json!({"kind": kind, "call_id": anchor}))
}

fn earlier_forecast() -> ContextMessage {
    let message: Message<Stored> = serde_json::from_value(json!({
        "id": "01JAX3K9T6R2B8V0QZ4M7D1E40", "role": "assistant", "text": "Frost on Thursday.",
        "attachments": [], "citations": [], "pending_actions": [], "interrupted": false,
        "created_at": "2026-10-16T11:00:00Z"
    }))
    .unwrap();
    ContextMessage {
        message,
        tool_calls: vec![
            ContentPart::ToolCall {
                call_id: "toolu_old".to_owned(),
                name: "get_forecast".to_owned(),
                arguments: json!({"days": 7}),
            },
            ContentPart::ToolResult {
                call_id: "toolu_old".to_owned(),
                content: json!([]),
                is_error: false,
            },
        ],
    }
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one case per refusal")]
async fn refused_blocks_send_no_event_and_give_the_model_a_tool_error() {
    let forecast = || call("toolu_fc", "get_forecast", json!({"days": 7}));
    let cases = vec![
        Refusal {
            name: "a call_id from an earlier turn",
            frost: &[true],
            history: vec![earlier_forecast()],
            calls: vec![tool_round(vec![show("s1", "frost_planner", "toolu_old")])],
            sent: 0,
            errors: vec![true],
            reason: "toolu_old",
        },
        Refusal {
            name: "a kind that doesn't fit the result",
            frost: &[true],
            history: Vec::new(),
            calls: vec![
                tool_round(vec![forecast()]),
                tool_round(vec![show("s1", "candidates", "toolu_fc")]),
            ],
            sent: 0,
            errors: vec![true],
            reason: "compare_lookalikes",
        },
        Refusal {
            name: "a forecast with no frost night",
            frost: &[false, false],
            history: Vec::new(),
            calls: vec![
                tool_round(vec![forecast()]),
                tool_round(vec![show("s1", "frost_planner", "toolu_fc")]),
            ],
            sent: 0,
            errors: vec![true],
            reason: "frost",
        },
        Refusal {
            name: "a focus plant not on the map",
            frost: &[false],
            history: Vec::new(),
            calls: vec![
                tool_round(vec![call("toolu_map", "get_garden_map", json!({}))]),
                tool_round(vec![call(
                    "s1",
                    "show_block",
                    json!({"kind": "garden_map", "call_id": "toolu_map", "focus_plant_ids": [BASIL]}),
                )]),
            ],
            sent: 0,
            errors: vec![true],
            reason: BASIL,
        },
        Refusal {
            name: "a call that failed",
            frost: &[true],
            history: Vec::new(),
            calls: vec![
                tool_round(vec![call("toolu_bad", "get_forecast", json!({"weeks": 1}))]),
                tool_round(vec![show("s1", "weather_hours", "toolu_bad")]),
            ],
            sent: 0,
            errors: vec![true],
            reason: "failed",
        },
        Refusal {
            name: "a fourth block",
            frost: &[true, true],
            history: Vec::new(),
            calls: vec![
                tool_round(vec![
                    forecast(),
                    call("toolu_fc2", "get_forecast", json!({"days": 2})),
                ]),
                tool_round(vec![
                    show("s1", "weather_hours", "toolu_fc"),
                    show("s2", "frost_planner", "toolu_fc"),
                    show("s3", "weather_hours", "toolu_fc2"),
                    show("s4", "frost_planner", "toolu_fc2"),
                ]),
            ],
            sent: 3,
            errors: vec![false, false, false, true],
            reason: "3 blocks",
        },
        Refusal {
            name: "the same kind and call twice",
            frost: &[true],
            history: Vec::new(),
            calls: vec![
                tool_round(vec![forecast()]),
                tool_round(vec![
                    show("s1", "weather_hours", "toolu_fc"),
                    show("s2", "weather_hours", "toolu_fc"),
                ]),
            ],
            sent: 1,
            errors: vec![false, true],
            reason: "already shows",
        },
    ];
    for case in cases {
        let mut script = case.calls;
        script.push(answer("Here is what the forecast says."));
        let (agent, model, seen) = harness(Setup {
            history: case.history,
            ..frost_setup(script, case.frost)
        });
        let stream = agent
            .start_turn(request(owner(Role::Viewer).await, "Frost this week?"))
            .await
            .unwrap();
        let (events, outcome) = collect(stream).await;
        let name = case.name;
        assert!(matches!(outcome, TurnOutcome::Completed(_)), "{name}");
        assert_eq!(blocks_of(&events).len(), case.sent, "{name}");
        let results = show_block_results(&model);
        let errors: Vec<bool> = results.iter().map(|(e, _)| *e).collect();
        assert_eq!(errors, case.errors, "{name}");
        let (_, refusal) = results.iter().rfind(|(e, _)| *e).unwrap();
        let reason = refusal["error"].as_str().unwrap();
        assert!(reason.contains(case.reason), "{name}: {reason}");
        // The text answer stands alone, and the message stores only the blocks sent.
        assert_eq!(
            text_of(&events),
            "Here is what the forecast says.",
            "{name}"
        );
        assert_eq!(
            seen.stored.lock().unwrap()[1].blocks.len(),
            case.sent,
            "{name}"
        );
    }
}
