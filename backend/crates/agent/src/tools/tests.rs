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
    assert_eq!(all.len(), 21, "twenty tools and show_block");
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted, "a stable order keeps the prompt cache");

    for role in [Role::Viewer, Role::Reviewer] {
        let names: Vec<_> = r
            .specs_for(Offer::everything(role))
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names.len(), 18);
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
        // A read and UI tool for every role, run by the turn, not found in the registry.
        assert!(names.iter().any(|n| n == "show_block"));
        assert!(r.find("show_block", Offer::everything(role)).is_none());
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
            farm_records: &[],
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
        tools::read::{FARM_KIND_RESERVED, FARM_RECORD_HITS, local_midnight, merge_kinds},
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
    async fn at_most_8_hits_from_its_own_garden_filtered_by_kind_with_a_window_ignored() {
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(|member, query, filter, k| {
                // The window is ignored for documents (EON gap G18).
                member.garden().to_string() == GARDEN
                    && query.0.len() == 4
                    && *filter
                        == FarmRecordFilter {
                            kind: Some(FarmRecordKind::Document),
                            ..FarmRecordFilter::default()
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

    /// A hit whose text names it, at a similarity.
    fn named(name: &str, n: u32, similarity: f32) -> FarmRecordMatch {
        let mut hit = spray_log_hit();
        hit.text = name.to_owned();
        hit.citation.record_id = format!("01JAX3K9T6R2B8V0QZ4M7D1E5C.{n}.D_SPRAY")
            .parse()
            .unwrap();
        FarmRecordMatch { hit, similarity }
    }

    fn texts(found: &[FarmRecordMatch]) -> Vec<&str> {
        found.iter().map(|m| m.hit.text.as_str()).collect()
    }

    #[test]
    fn each_kind_keeps_its_reserve_and_the_rest_goes_by_similarity() {
        // Ten close messages and three far documents: the documents keep 3 of the 8.
        let messages: Vec<_> = (0..10_u8)
            .map(|i| named(&format!("m{i}"), i.into(), 0.9 - f32::from(i) * 0.02))
            .collect();
        let documents = vec![
            named("d0", 20, 0.4),
            named("d1", 21, 0.3),
            named("d2", 22, 0.2),
        ];
        let out = merge_kinds(messages.clone(), documents.clone(), 8, 3);
        assert_eq!(
            texts(&out),
            ["m0", "m1", "m2", "m3", "m4", "d0", "d1", "d2"]
        );
        // The two free slots go by similarity: the fourth document (0.85) and the fourth
        // message (0.84), not the fifth message (0.82).
        let close = vec![
            named("d3", 23, 0.95),
            named("d4", 24, 0.93),
            named("d5", 25, 0.91),
            named("d6", 26, 0.85),
        ];
        let out = merge_kinds(messages.clone(), close, 8, 3);
        assert_eq!(
            texts(&out),
            ["d3", "d4", "d5", "m0", "m1", "m2", "d6", "m3"]
        );
        // One document: the messages fill the other 7 slots.
        let out = merge_kinds(messages.clone(), documents[..1].to_vec(), 8, 3);
        assert_eq!(out.len(), 8);
        assert_eq!(texts(&out).iter().filter(|t| t.starts_with('d')).count(), 1);
        // No documents, or no messages: the other kind takes every slot, closest first.
        let out = merge_kinds(messages.clone(), Vec::new(), 8, 3);
        assert_eq!(texts(&out), texts(&messages[..8]));
        let out = merge_kinds(Vec::new(), documents.clone(), 8, 3);
        assert_eq!(texts(&out), ["d0", "d1", "d2"]);
        // Out-of-order input comes back closest first.
        let out = merge_kinds(
            vec![named("m-low", 1, 0.1), named("m-high", 2, 0.8)],
            vec![named("d-mid", 3, 0.5)],
            8,
            3,
        );
        assert_eq!(texts(&out), ["m-high", "d-mid", "m-low"]);
        // A NaN similarity (a zero vector) is left out, not ranked first.
        let out = merge_kinds(
            vec![named("m-nan", 1, f32::NAN), named("m", 2, 0.5)],
            vec![named("d-inf", 3, f32::INFINITY), named("d", 4, 0.4)],
            8,
            3,
        );
        assert_eq!(texts(&out), ["m", "d"]);
        assert_eq!(FARM_KIND_RESERVED, 3);
    }

    #[tokio::test]
    async fn without_kind_each_kind_is_searched_and_keeps_its_reserve() {
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(|_, _, filter, k| {
                *filter
                    == FarmRecordFilter {
                        kind: Some(FarmRecordKind::Message),
                        ..FarmRecordFilter::default()
                    }
                    && *k == FARM_RECORD_HITS
            })
            .times(1)
            .returning(|_, _, _, _| Ok((0..8).map(|i| named(&format!("m{i}"), i, 0.9)).collect()));
        farm.expect_search_records()
            .withf(|_, _, filter, k| {
                *filter
                    == FarmRecordFilter {
                        kind: Some(FarmRecordKind::Document),
                        ..FarmRecordFilter::default()
                    }
                    && *k == FARM_RECORD_HITS
            })
            .times(1)
            .returning(|_, _, _, _| Ok(vec![named("spray log", 20, 0.2)]));
        let out = call(farm, json!({"query": "was bed 3 sprayed"}))
            .await
            .unwrap();
        let returned: Vec<FarmRecordHit> = serde_json::from_value(out.content).unwrap();
        assert_eq!(returned.len(), 8);
        assert_eq!(returned[7].text, "spray log");
        assert_eq!(out.effects.len(), 8);
    }

    #[tokio::test]
    async fn a_window_applies_to_messages_and_never_to_documents() {
        // EON gap G18: a document's time is its last edit, so a question about July with
        // `until` set must still find the spray log, edited later. Midnight in Toronto is
        // 04:00Z (EDT).
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(|_, _, filter, _| {
                let since: Timestamp = "2026-07-09T04:00:00Z".parse().unwrap();
                let until: Timestamp = "2026-07-29T04:00:00Z".parse().unwrap();
                filter.kind == Some(FarmRecordKind::Message)
                    && filter.since == Some(since)
                    && filter.until == Some(until)
            })
            .times(1)
            .returning(|_, _, _, _| Ok(vec![named("a slack message", 50, 0.5)]));
        farm.expect_search_records()
            .withf(|_, _, filter, _| {
                filter.kind == Some(FarmRecordKind::Document)
                    && filter.since.is_none()
                    && filter.until.is_none()
            })
            .times(1)
            .returning(|_, _, _, _| Ok(hits(2)));
        let out = call(
            farm,
            json!({"query": "spray log", "since": "2026-07-09", "until": "2026-07-28"}),
        )
        .await
        .unwrap();
        let returned: Vec<FarmRecordHit> = serde_json::from_value(out.content).unwrap();
        // The spray log is returned though the window ended before it was edited.
        assert!(returned.contains(&spray_log_hit()));
        assert_eq!(returned.len(), 3);
    }

    #[tokio::test]
    async fn until_includes_its_day_and_author_is_trimmed() {
        // Q19: "What did Riley post in #field-reports on August 31?". Riley's message is at
        // 1788216000 (2026-08-31T22:40:00Z), within Toronto's August 31 (04:00Z to 04:00Z).
        let riley: Timestamp = "2026-08-31T22:40:00Z".parse().unwrap();
        assert_eq!(riley.unix_seconds(), 1_788_216_000);
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(move |_, _, filter, _| {
                let since: Timestamp = "2026-08-31T04:00:00Z".parse().unwrap();
                let until: Timestamp = "2026-09-01T04:00:00Z".parse().unwrap();
                // Documents have no author: one search, of the messages.
                filter.kind == Some(FarmRecordKind::Message)
                    && filter.since == Some(since)
                    && filter.until == Some(until)
                    && filter.author.as_deref() == Some("Riley")
                    && since <= riley
                    && riley < until
            })
            .times(1)
            .returning(|_, _, _, _| Ok(Vec::new()));
        let out = call(
            farm,
            json!({"query": "Riley's post", "author": "  Riley ", "since": "2026-08-31",
                "until": "2026-08-31"}),
        )
        .await
        .unwrap();
        assert_eq!(out.content, json!([]));

        // A blank author is no filter.
        let mut farm = MockFarmRecordStore::new();
        farm.expect_search_records()
            .withf(|_, _, filter, _| filter.author.is_none() && filter.until.is_none())
            .times(1)
            .returning(|_, _, _, _| Ok(Vec::new()));
        call(
            farm,
            json!({"query": "x", "author": "   ", "kind": "message"}),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn unknown_arguments_and_kinds_are_refused_before_the_search() {
        for args in [
            json!({"query": "x", "garden_id": GARDEN}),
            json!({"query": "x", "kind": "email"}),
            json!({"query": "x", "since": "1 July"}),
            json!({"query": "x", "until": "31 August"}),
            json!({"query": "x", "author": 7}),
            // `until` before `since`, and an author for documents, which have none.
            json!({"query": "x", "since": "2026-09-01", "until": "2026-08-31"}),
            json!({"query": "x", "author": "Maria", "kind": "document"}),
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

/// `remember`'s fact is dose-checked before the tool runs; a date before a colon is no ratio
/// (EON gap G22: E8.3's third run had Opus's `remember` refused twice for this fact).
#[test]
fn remember_keeps_a_dated_fact_and_refuses_a_ratio() {
    let args = |fact: &str| -> ui::RememberArgs {
        serde_json::from_value(json!({ "fact": fact })).unwrap()
    };
    let products = ProductCatalogue::builtin();
    for fact in [
        "2026-07-15: 420 bulbs",
        "2026-07-15: 420 bulbs went into bed 4.",
        "Bed 3 was sprayed with copper on 2 July at 18:40.",
        "Bed 1 had no sprays in 2026.",
    ] {
        let a = args(fact);
        assert!(
            dose_check_all(ui::Remember::model_strings(&a), products).is_none(),
            "{fact:?} should pass"
        );
    }
    let a = args("Maria mixes the copper 1:10.");
    assert!(dose_check_all(ui::Remember::model_strings(&a), products).is_some());
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

/// What one JSON Schema costs against Anthropic's strict-mode budget: regex patterns, optional
/// properties and union types (`anyOf`, or a `type` array).
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
struct StrictCost {
    patterns: usize,
    optional: usize,
    unions: usize,
}

fn strict_cost(v: &serde_json::Value, cost: &mut StrictCost) {
    match v {
        serde_json::Value::Object(o) => {
            if o.contains_key("pattern") {
                cost.patterns += 1;
            }
            if o.contains_key("anyOf") || o.get("type").is_some_and(serde_json::Value::is_array) {
                cost.unions += 1;
            }
            if let Some(serde_json::Value::Object(props)) = o.get("properties") {
                let required = o.get("required").and_then(serde_json::Value::as_array);
                cost.optional += props
                    .keys()
                    .filter(|k| !required.is_some_and(|r| r.iter().any(|x| x == k.as_str())))
                    .count();
            }
            o.values().for_each(|x| strict_cost(x, cost));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| strict_cost(x, cost)),
        _ => {}
    }
}

/// The strict tools' schemas, sanitised as the Anthropic adapter sends them, stay inside the
/// budget measured when they last passed the real API. One more pattern (PR #176's on
/// `PlanStepDraft.farm_citations`) got every chat turn `400 Schema is too complex`. Raise a
/// limit only after `live_chat_accepts_every_tool_schema_and_streams` passes with the change.
#[test]
fn the_strict_tool_schemas_stay_inside_anthropics_budget() {
    let mut cost = StrictCost::default();
    for spec in ToolRegistry::standard()
        .specs_for(Offer::everything(Role::Owner))
        .into_iter()
        .filter(|s| s.strict)
    {
        strict_cost(
            &integrations::anthropic::sanitize_schema(&spec.parameters).schema,
            &mut cost,
        );
    }
    assert!(cost.patterns <= 8, "{cost:?}");
    assert!(cost.optional <= 12, "{cost:?}");
    assert!(cost.unions <= 10, "{cost:?}");
}

/// `show_block`'s kinds the turn tests don't reach: `weather_hours` from a scan's snapshot,
/// `measurement_trend` and `candidates`, each built from the turn's results and the store.
mod show_block {
    use std::sync::Arc;

    use domain::{
        chat::{Anchor, AnswerBlock, AnswerBlockKind, BlockBody},
        ports::{MockKnowledgeStore, MockMemoryStore, MockPlantRepo, NoBeliefs},
    };

    use super::*;
    use crate::{
        error::ToolError,
        testing::{GARDEN, garden_deps, now, owner},
        tools::blocks::{ShowBlockArgs, build},
    };

    const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5F";
    const SCAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5H";

    fn calls(
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
    ) -> Vec<ContentPart> {
        vec![
            ContentPart::ToolCall {
                call_id: "toolu_1".to_owned(),
                name: name.to_owned(),
                arguments,
            },
            ContentPart::ToolResult {
                call_id: "toolu_1".to_owned(),
                content: result,
                is_error: false,
            },
        ]
    }

    fn point(quantity: &str, value: f64) -> serde_json::Value {
        json!({"scan_id": SCAN, "created_at": "2026-10-11T15:00:00Z", "quantity": quantity,
               "value": value, "confidence_band": "high"})
    }

    async fn show(
        deps: &AgentDeps,
        kind: AnswerBlockKind,
        calls: &[ContentPart],
    ) -> Result<AnswerBlock, ToolError> {
        let ctx = ToolCtx {
            deps,
            owner: owner(Role::Viewer).await,
            anchor: Anchor::Garden(GARDEN.parse().unwrap()),
            plant: None,
            message_id: "01JAX3K9T6R2B8V0QZ4M7D1E72".parse().unwrap(),
            now: now(),
        };
        let args = ShowBlockArgs {
            kind,
            call_id: "toolu_1".to_owned(),
            focus_plant_ids: None,
        };
        build(&ctx, calls, &[], args).await
    }

    fn deps() -> AgentDeps {
        let mut deps = garden_deps(MockMemoryStore::new(), Arc::new(NoBeliefs));
        let mut plants = MockPlantRepo::new();
        plants.expect_plant().returning(|_, id| {
            Ok(serde_json::from_value(json!({
                "id": id, "garden_id": GARDEN, "name": "Tomato 1", "species_status": "unknown",
                "species_candidates": [], "status": "watch", "created_at": "2026-10-01T00:00:00Z"
            }))
            .unwrap())
        });
        deps.stores.plants = Arc::new(plants);
        let mut knowledge = MockKnowledgeStore::new();
        knowledge
            .expect_problem()
            .returning(|_| Ok(fixtures::problems().into_iter().next()));
        deps.knowledge = Arc::new(knowledge);
        deps
    }

    #[tokio::test]
    async fn weather_hours_from_a_scan_takes_its_snapshot_and_names_the_scan() {
        let day = json!({"date": "2026-10-11", "t_min_c": 12.0, "t_max_c": 21.0, "precip_mm": 3.0,
                         "mean_rh_pct": 91.0, "hours_rh_over_90": 14, "frost_risk": false});
        let deps = deps();
        let result = json!({"id": SCAN, "status": "planned", "weather": {"forecast": [day]}});
        let block = show(
            &deps,
            AnswerBlockKind::WeatherHours,
            &calls("get_scan", json!({"scan_id": SCAN}), result),
        )
        .await
        .unwrap();
        let BlockBody::WeatherHours { days, scan_id } = block.body else {
            panic!()
        };
        assert_eq!(scan_id.map(|s| s.to_string()).as_deref(), Some(SCAN));
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].hours_rh_over_90, 14);

        let bare = json!({"id": SCAN, "status": "planned"});
        let refused = show(
            &deps,
            AnswerBlockKind::WeatherHours,
            &calls("get_scan", json!({"scan_id": SCAN}), bare),
        )
        .await
        .unwrap_err();
        assert!(refused.for_model().contains("no weather"), "{refused}");
    }

    #[tokio::test]
    async fn a_measurement_trend_names_its_plant_and_refuses_mixed_quantities() {
        let deps = deps();
        let one = json!([point("plant_height_cm", 38.5)]);
        let block = show(
            &deps,
            AnswerBlockKind::MeasurementTrend,
            &calls("get_plant_measurements", json!({"plant_id": PLANT}), one),
        )
        .await
        .unwrap();
        let BlockBody::MeasurementTrend {
            plant_id,
            plant_name,
            quantity,
            points,
        } = block.body
        else {
            panic!()
        };
        assert_eq!(plant_id.to_string(), PLANT);
        assert_eq!(plant_name, "Tomato 1");
        assert_eq!(
            quantity,
            domain::garden::plants::GrowthQuantity::PlantHeightCm
        );
        assert_eq!(points.len(), 1);

        let mixed = json!([
            point("plant_height_cm", 38.5),
            point("canopy_width_cm", 30.0)
        ]);
        let refused = show(
            &deps,
            AnswerBlockKind::MeasurementTrend,
            &calls("get_plant_measurements", json!({"plant_id": PLANT}), mixed),
        )
        .await
        .unwrap_err();
        assert!(refused.for_model().contains("mix"), "{refused}");
        // With the call's quantity, the points are taken as returned.
        let asked = calls(
            "get_plant_measurements",
            json!({"plant_id": PLANT, "quantity": "canopy_width_cm"}),
            json!([point("canopy_width_cm", 30.0)]),
        );
        assert!(
            show(&deps, AnswerBlockKind::MeasurementTrend, &asked)
                .await
                .is_ok()
        );
        let none = calls(
            "get_plant_measurements",
            json!({"plant_id": PLANT}),
            json!([]),
        );
        assert!(
            show(&deps, AnswerBlockKind::MeasurementTrend, &none)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn candidates_name_the_problem_from_the_knowledge_base() {
        let deps = deps();
        let lookalikes = json!([{"problem_id": "septoria_leaf_spot", "name": "Septoria leaf spot",
                                 "how_to_tell": "Smaller spots with grey centres."}]);
        let block = show(
            &deps,
            AnswerBlockKind::Candidates,
            &calls(
                "compare_lookalikes",
                json!({"disease": "tomato_early_blight"}),
                lookalikes,
            ),
        )
        .await
        .unwrap();
        let BlockBody::Candidates {
            disease,
            lookalikes,
        } = block.body
        else {
            panic!()
        };
        assert_eq!(disease.problem_id.to_string(), "tomato_early_blight");
        assert_eq!(disease.name, "tomato_early_blight", "the fixture's name");
        assert_eq!(lookalikes.len(), 1);

        let none = calls(
            "compare_lookalikes",
            json!({"disease": "tomato_early_blight"}),
            json!([]),
        );
        assert!(
            show(&deps, AnswerBlockKind::Candidates, &none)
                .await
                .is_err()
        );
    }
}
