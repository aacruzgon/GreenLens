use domain::{
    chat::Preview,
    id::PendingActionId,
    records::plan::{ActionPlan, StepCategory},
};
use serde_json::json;

use super::*;
use crate::{
    fixtures,
    safety::{PlanDraft, ProductCatalogue, ValidationContext, validate_plan},
    tools::{read::RegionArg, revise::apply_preview},
};

#[test]
fn the_registry_has_every_tool_sorted_and_filters_by_role() {
    let r = ToolRegistry::standard();
    let all: Vec<_> = r
        .specs_for(Offer::everything(Role::Owner))
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(all.len(), 20);
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted, "a stable order keeps the prompt cache");

    for role in [Role::Viewer, Role::Reviewer] {
        let names: Vec<_> = r
            .specs_for(Offer::everything(role))
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names.len(), 17);
        for write in ["create_task", "complete_task", "revise_plan"] {
            assert!(
                !names.iter().any(|n| n == write),
                "{role:?} is offered {write}"
            );
            assert!(r.find(write, Offer::everything(role)).is_none());
        }
        assert!(names.iter().any(|n| n == "remember"));
        assert!(names.iter().any(|n| n == "request_rescan"));
        assert!(names.iter().any(|n| n == "search_farm_records"));
    }
    assert!(
        r.find("revise_plan", Offer::everything(Role::Editor))
            .is_some()
    );
}

#[test]
fn search_farm_records_is_left_out_of_a_garden_without_farm_records() {
    let r = ToolRegistry::standard();
    for role in [Role::Viewer, Role::Reviewer, Role::Editor, Role::Owner] {
        let without = Offer {
            role,
            farm_records: false,
        };
        let names: Vec<_> = r.specs_for(without).into_iter().map(|s| s.name).collect();
        assert!(
            !names.iter().any(|n| n == "search_farm_records"),
            "{role:?}"
        );
        assert!(r.find("search_farm_records", without).is_none());
        assert_eq!(
            names.len() + 1,
            r.specs_for(Offer::everything(role)).len(),
            "only the farm tool goes"
        );
        assert!(
            r.find("search_farm_records", Offer::everything(role))
                .is_some()
        );
    }
}

#[test]
fn search_farm_records_frames_its_results_as_farm_records() {
    let r = ToolRegistry::standard();
    let offer = Offer::everything(Role::Viewer);
    let farm = r.find("search_farm_records", offer).unwrap();
    assert_eq!(farm.data_kind(), DataKind::FarmRecords);
    assert!(farm.needs_farm_records());
    assert_eq!(farm.kind(), ToolKind::Read);
    assert!(!farm.spec().strict);
    // Every other tool keeps the default.
    for t in r
        .offered(offer)
        .filter(|t| t.name() != ToolName::SearchFarmRecords)
    {
        assert_eq!(
            t.data_kind(),
            DataKind::ToolResult,
            "{}",
            tool_name(t.name())
        );
        assert!(!t.needs_farm_records());
    }
    let params = &farm.spec().parameters;
    assert_eq!(params["required"], json!(["query"]));
    for arg in ["query", "kind", "since"] {
        assert!(params["properties"][arg].is_object(), "{arg}: {params}");
    }
}

#[test]
fn specs_carry_a_json_schema_of_the_typed_arguments() {
    let r = ToolRegistry::standard();
    let specs = r.specs_for(Offer::everything(Role::Owner));
    let remember = specs.iter().find(|s| s.name == "remember").unwrap();
    assert_eq!(remember.parameters["type"], "object");
    assert!(remember.parameters["properties"]["fact"].is_object());
    assert_eq!(remember.parameters["required"], json!(["fact"]));
    let revise = specs.iter().find(|s| s.name == "revise_plan").unwrap();
    assert!(revise.parameters.to_string().contains("changed_steps"));
}

fn plan() -> ActionPlan {
    let kb = Kb::new();
    let steps = validate_plan(
        PlanDraft {
            summary: "Early blight.".to_owned(),
            steps: vec![fixtures::step("s1", 1), fixtures::step("s2", 2)],
            watch_for: vec!["New spots within 5 days".to_owned()],
        },
        &kb.ctx(),
    )
    .steps;
    serde_json::from_value(json!({
        "id": "01JAX3K9T6R2B8V0QZ4M7D1E60", "scan_id": fixtures::SCAN, "plant_id": fixtures::PLANT,
        "version": 1, "provisional": false, "diagnosis_id": fixtures::DIAGNOSIS,
        "constraints": [], "summary": "Early blight.", "steps": steps,
        "watch_for": ["New spots within 5 days"], "citations": [], "task_ids": [],
        "llm_model": "scripted", "prompt_version": "plan/test", "created_at": "2026-10-16T10:00:00Z"
    }))
    .unwrap()
}

struct Kb {
    treatments: Vec<domain::knowledge::TreatmentSeed>,
    organisms: Vec<domain::knowledge::OrganismSeed>,
    problems: Vec<domain::knowledge::ProblemSeed>,
    neighbours: Vec<domain::records::plan::PlanNeighbour>,
    sources: Vec<domain::id::SourceId>,
}

impl Kb {
    fn new() -> Self {
        Self {
            treatments: fixtures::treatments(),
            organisms: fixtures::organisms(),
            problems: fixtures::problems(),
            neighbours: fixtures::neighbours(),
            sources: fixtures::sources(),
        }
    }
    fn ctx(&self) -> ValidationContext<'_> {
        ValidationContext {
            treatments: &self.treatments,
            products: ProductCatalogue::builtin(),
            organisms: &self.organisms,
            problems: &self.problems,
            neighbours: &self.neighbours,
            beneficials_seen: false,
            known_sources: &self.sources,
        }
    }
}

#[test]
fn apply_preview_replaces_adds_removes_and_appends_constraints() {
    let base = plan();
    let mut changed = base.steps[1].clone();
    changed.title = "Remove every infected leaf".to_owned();
    let mut added = base.steps[0].clone();
    added.id = "s3".parse().unwrap();
    added.order = 3;
    let preview = Preview::Plan {
        base_plan_id: base.id,
        plan_summary: "Without copper.".to_owned(),
        changed_steps: vec![changed, added],
        removed_step_ids: vec!["s1".parse().unwrap()],
        constraints_added: vec!["No copper products".to_owned()],
    };
    let action: PendingActionId = "01JAX3K9T6R2B8V0QZ4M7D1E61".parse().unwrap();
    let now = "2026-10-16T12:00:00Z".parse().unwrap();
    let revised = apply_preview(&base, &preview, action, now).unwrap();
    let ids: Vec<_> = revised.steps.iter().map(|s| s.id.to_string()).collect();
    assert_eq!(ids, ["s2", "s3"]);
    assert_eq!(revised.steps[0].title, "Remove every infected leaf");
    assert_eq!(revised.summary, "Without copper.");
    assert_eq!(revised.constraints.len(), 1);
    assert_eq!(revised.constraints[0].from_pending_action_id, action);
    assert_eq!(revised.citation_ids, fixtures::sources());

    let mut other = base.clone();
    other.id = "01JAX3K9T6R2B8V0QZ4M7D1E62".parse().unwrap();
    assert!(apply_preview(&other, &preview, action, now).is_err());
    let task_preview = Preview::Task {
        task: Box::new(
            serde_json::from_value(json!({
                "id": "01JAX3K9T6R2B8V0QZ4M7D1E63", "garden_id": fixtures::NEIGHBOUR,
                "title": "Water", "source": "chat", "created_at": "2026-10-16T10:00:00Z"
            }))
            .unwrap(),
        ),
    };
    assert!(apply_preview(&base, &task_preview, action, now).is_err());
}

#[test]
fn chemical_steps_in_a_stored_plan_keep_their_note() {
    let base = plan();
    assert!(
        base.steps
            .iter()
            .all(|s| s.category == StepCategory::Cultural)
    );
}

#[test]
fn crops_are_checked_and_encoded_as_png() {
    let img = image::RgbImage::from_pixel(40, 20, image::Rgb([10, 200, 30]));
    let mut webp = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut webp, image::ImageFormat::WebP)
        .unwrap();
    let png = read::crop_png(
        webp.get_ref(),
        RegionArg {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
    )
    .unwrap();
    let crop = image::load_from_memory(&png).unwrap();
    assert_eq!((crop.width(), crop.height()), (20, 10));
    assert!(
        read::crop_png(
            webp.get_ref(),
            RegionArg {
                x: 0.8,
                y: 0.0,
                w: 0.5,
                h: 0.5
            }
        )
        .is_err()
    );
}

#[test]
fn write_tools_offer_only_to_editors() {
    assert!(!ToolKind::Write.offered_to(Role::Reviewer));
    assert!(ToolKind::Write.offered_to(Role::Editor));
    assert!(ToolKind::Memory.offered_to(Role::Viewer));
}

#[test]
fn only_the_tools_that_propose_changes_are_strict() {
    let strict: Vec<String> = ToolRegistry::standard()
        .specs_for(Offer::everything(Role::Owner))
        .into_iter()
        .filter(|s| s.strict)
        .map(|s| s.name)
        .collect();
    assert_eq!(strict, ["complete_task", "create_task", "revise_plan"]);
}

mod search_memory {
    use std::{sync::Arc, time::Duration};

    use domain::{
        chat::{Anchor, BeliefHit, MemorySearch},
        ports::{MemoryHit, MockBeliefStore, MockMemoryStore},
    };

    use super::*;
    use crate::testing::{GARDEN, SlowBeliefs, garden_deps, now, owner};

    fn memory() -> MockMemoryStore {
        let mut memory = MockMemoryStore::new();
        memory.expect_search().returning(|_, _, _, _| {
            Ok(vec![MemoryHit {
                memory: fixtures::memory(
                    "01JAX3K9T6R2B8V0QZ4M7D1E71",
                    "user",
                    true,
                    "Watered every 3 days.",
                ),
                similarity: 0.8,
            }])
        });
        memory
    }

    async fn run(deps: &AgentDeps) -> MemorySearch {
        let ctx = ToolCtx {
            deps,
            owner: owner(Role::Viewer).await,
            anchor: Anchor::Garden(GARDEN.parse().unwrap()),
            plant: None,
            message_id: "01JAX3K9T6R2B8V0QZ4M7D1E72".parse().unwrap(),
            now: now(),
        };
        let out = Erased::new(read::SearchMemory)
            .call(&ctx, json!({"query": "watering"}))
            .await
            .unwrap();
        serde_json::from_value(out.content).unwrap()
    }

    #[tokio::test]
    async fn belief_hits_are_merged_with_the_local_hits() {
        let mut beliefs = MockBeliefStore::new();
        beliefs.expect_enabled().return_const(true);
        beliefs
            .expect_search()
            .withf(|_, plant, query, k| plant.is_none() && query == "watering" && *k == 5)
            .times(1)
            .returning(|_, _, _, _| {
                Ok(vec![BeliefHit {
                    key: "watering".to_owned(),
                    text: "watering is every 2 days".to_owned(),
                    about_plant: false,
                    score: 0.9,
                }])
            });
        let deps = garden_deps(memory(), Arc::new(beliefs));
        let found = run(&deps).await;
        assert_eq!(found.memories.len(), 1);
        assert_eq!(found.memories[0].text, "Watered every 3 days.");
        assert_eq!(found.beliefs.len(), 1);
        assert_eq!(found.beliefs[0].key, "watering");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_belief_store_gives_no_beliefs_within_the_timeout() {
        let deps = garden_deps(memory(), Arc::new(SlowBeliefs));
        let start = tokio::time::Instant::now();
        let found = run(&deps).await;
        assert_eq!(start.elapsed(), Duration::from_secs(4));
        assert_eq!(found.memories.len(), 1);
        assert_eq!(found.beliefs, []);
    }
}

mod search_farm_records {
    use std::sync::Arc;

    use domain::{
        authz::{ConversationOwner, Viewer},
        chat::Anchor,
        connectors::{FarmRecordHit, FarmRecordKind},
        garden::gardens::TimeZone,
        ports::{
            FarmRecordFilter, FarmRecordMatch, GardenContext, MockFarmRecordStore, MockGardenRepo,
            MockMemoryStore, NoBeliefs,
        },
        records::weather::{Coordinates, LocationPrecision, WeatherCell},
        wire::{LocalDate, Timestamp},
    };

    use super::*;
    use crate::{
        error::ToolError,
        testing::{GARDEN, garden_deps, now, owner, spray_log_hit},
        tools::read::{FARM_RECORD_HITS, local_midnight},
    };

    fn toronto() -> MockGardenRepo {
        let mut gardens = MockGardenRepo::new();
        gardens.expect_garden_context().returning(|garden| {
            let at = Coordinates {
                lat: 43.65,
                lon: -79.38,
            };
            Ok(GardenContext {
                id: garden,
                name: "Maple Row Farm".to_owned(),
                coordinates: at,
                precision: LocationPrecision::About1Km,
                weather_cell: WeatherCell::of(at),
                time_zone: TimeZone("America/Toronto".to_owned()),
                soil_profile: None,
                map_version: 0,
            })
        });
        gardens
    }

    /// `n` copies of the spray log hit, numbered as distinct chunks.
    fn hits(n: u32) -> Vec<FarmRecordMatch> {
        (0..n)
            .map(|i| {
                let mut hit = spray_log_hit();
                hit.citation.record_id = format!("01JAX3K9T6R2B8V0QZ4M7D1E5C.{i}.D_SPRAY")
                    .parse()
                    .unwrap();
                FarmRecordMatch {
                    hit,
                    similarity: 0.9,
                }
            })
            .collect()
    }

    async fn call(
        farm: MockFarmRecordStore,
        args: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let mut deps = garden_deps(MockMemoryStore::new(), Arc::new(NoBeliefs));
        deps.stores.farm = Arc::new(farm);
        deps.stores.gardens = Arc::new(toronto());
        let owner: ConversationOwner<Viewer> = owner(Role::Viewer).await;
        let ctx = ToolCtx {
            deps: &deps,
            owner,
            anchor: Anchor::Garden(GARDEN.parse().unwrap()),
            plant: None,
            message_id: "01JAX3K9T6R2B8V0QZ4M7D1E72".parse().unwrap(),
            now: now(),
        };
        Erased::new(read::SearchFarmRecords).call(&ctx, args).await
    }

    #[tokio::test]
    async fn at_most_8_hits_from_its_own_garden_filtered_by_kind_and_since() {
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(|member, query, filter, k| {
                // Midnight in Toronto on 1 July is 04:00 UTC (EDT).
                let since: Timestamp = "2026-07-01T04:00:00Z".parse().unwrap();
                member.garden().to_string() == GARDEN
                    && query.0.len() == 4
                    && *filter
                        == FarmRecordFilter {
                            kind: Some(FarmRecordKind::Document),
                            since: Some(since),
                        }
                    && *k == FARM_RECORD_HITS
            })
            .times(1)
            // A store that returned too many is cut to 8.
            .returning(|_, _, _, _| Ok(hits(10)));
        let out = call(
            farm,
            json!({"query": "copper on bed 3", "kind": "document", "since": "2026-07-01"}),
        )
        .await
        .unwrap();
        let returned: Vec<FarmRecordHit> = serde_json::from_value(out.content).unwrap();
        assert_eq!(returned.len(), 8);
        assert_eq!(returned[0], spray_log_hit());
        assert_eq!(out.effects.len(), 8);
        assert!(
            out.effects
                .iter()
                .zip(&returned)
                .all(|(e, h)| *e == ToolEffect::FarmCitation(h.citation.clone()))
        );
    }

    #[tokio::test]
    async fn without_filters_the_search_takes_none() {
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(|_, _, filter, _| *filter == FarmRecordFilter::default())
            .times(1)
            .returning(|_, _, _, _| Ok(hits(2)));
        let out = call(farm, json!({"query": "late blight"})).await.unwrap();
        assert_eq!(out.content.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn unknown_arguments_and_kinds_are_refused_before_the_search() {
        for args in [
            json!({"query": "x", "garden_id": GARDEN}),
            json!({"query": "x", "kind": "email"}),
            json!({"query": "x", "since": "1 July"}),
        ] {
            // No expectation: a search would panic the test.
            let err = call(MockFarmRecordStore::new(), args.clone())
                .await
                .unwrap_err();
            assert!(matches!(err, ToolError::Rejected(_)), "{args}");
        }
    }

    #[test]
    fn since_is_the_gardens_midnight() {
        let day: LocalDate = "2026-01-15".parse().unwrap();
        assert_eq!(
            local_midnight(day, "America/Toronto"),
            "2026-01-15T05:00:00Z".parse().unwrap()
        );
        assert_eq!(
            local_midnight(day, "not a zone"),
            "2026-01-15T00:00:00Z".parse().unwrap()
        );
        // Midnight doesn't exist in Santiago on the day DST starts (2026-09-06): 01:00 local.
        let gap: LocalDate = "2026-09-06".parse().unwrap();
        assert_eq!(
            local_midnight(gap, "America/Santiago"),
            "2026-09-06T04:00:00Z".parse().unwrap()
        );
    }
}

#[test]
fn remember_takes_an_optional_topic() {
    let specs = ToolRegistry::standard().specs_for(Offer::everything(Role::Viewer));
    let remember = specs.iter().find(|s| s.name == "remember").unwrap();
    assert!(remember.parameters["properties"]["topic"].is_object());
    let args: ui::RememberArgs = serde_json::from_value(json!({"fact": "x"})).unwrap();
    assert!(args.topic.is_none());
    let args: ui::RememberArgs =
        serde_json::from_value(json!({"fact": "x", "topic": "watering"})).unwrap();
    assert_eq!(args.topic, Some(crate::memory::ExtractedTopic::Watering));
}
