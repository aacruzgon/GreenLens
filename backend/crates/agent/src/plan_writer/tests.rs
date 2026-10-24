use domain::ports::{FarmRecordMatch, MockFarmRecordStore, MockTextEmbedder, StoreError};
use serde_json::json;

use super::*;
use crate::{fixtures, safety::PlanRule, testing::ScriptedModel};

struct Loaded {
    diagnosis: Diagnosis<Stored>,
    problems: Vec<ProblemSeed>,
    treatments: Vec<TreatmentSeed>,
    organisms: Vec<OrganismSeed>,
    memories: Vec<Memory>,
    beliefs: Vec<Belief>,
    neighbours: Vec<PlanNeighbour>,
    sources: Vec<SourceId>,
    farm_records: Vec<FarmRecordHit>,
}

impl Loaded {
    fn new() -> Self {
        Self {
            diagnosis: fixtures::diagnosis(),
            problems: fixtures::problems(),
            treatments: fixtures::treatments(),
            organisms: fixtures::organisms(),
            memories: vec![fixtures::memory(
                "01JAX3K9T6R2B8V0QZ4M7D1E5M",
                "user",
                true,
                "I garden organically",
            )],
            beliefs: vec![fixtures::belief(
                "container",
                "The tomato is in a 30 cm pot.",
                true,
            )],
            neighbours: fixtures::neighbours(),
            sources: fixtures::sources(),
            farm_records: Vec::new(),
        }
    }

    fn input(&self) -> PlanInput<'_> {
        PlanInput {
            plant_name: "Tomato by the fence",
            species: None,
            diagnosis: &self.diagnosis,
            problems: &self.problems,
            treatments: &self.treatments,
            organisms: &self.organisms,
            memories: SharedMemories::new(&self.memories).unwrap(),
            beliefs: &self.beliefs,
            constraints: &[],
            neighbours: &self.neighbours,
            open_question: None,
            weather: None,
            known_sources: &self.sources,
            farm_records: &self.farm_records,
        }
    }
}

fn step_json(id: &str, title: &str) -> serde_json::Value {
    json!({
        "id": id, "order": 1, "title": title, "detail": "Bag them.",
        "treatment_id": "remove_lower_infected_leaves", "category": "cultural",
        "when": {"kind": "now"},
        "targets": [{"table": "disease", "id": "tomato_early_blight"}],
        "citations": ["ext_tomato"]
    })
}

fn answer(steps: &[serde_json::Value]) -> (String, StopReason) {
    (
        json!({"summary": "Early blight.", "steps": steps, "watch_for": ["New spots within 5 days"]})
            .to_string(),
        StopReason::EndTurn,
    )
}

fn writer(model: &Arc<ScriptedModel>) -> PlanWriter {
    PlanWriter::new(
        model.clone(),
        Arc::new(ProductCatalogue::new(&fixtures::products())),
    )
}

#[test]
fn private_memories_never_reach_the_plan_writer() {
    let private = [fixtures::memory(
        "01JAX3K9T6R2B8V0QZ4M7D1E5N",
        "chat",
        false,
        "secret",
    )];
    assert!(SharedMemories::new(&private).is_err());
    let unshared_user = [fixtures::memory(
        "01JAX3K9T6R2B8V0QZ4M7D1E5N",
        "user",
        false,
        "x",
    )];
    assert!(SharedMemories::new(&unshared_user).is_err());
}

#[tokio::test]
async fn a_clean_answer_is_returned_after_one_call() {
    let model = Arc::new(ScriptedModel::completions(vec![answer(&[step_json(
        "s1",
        "Remove the lowest infected leaves",
    )])]));
    let loaded = Loaded::new();
    let plan = writer(&model).write(&loaded.input()).await.unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.dropped.len(), 0);
    assert_eq!(plan.prompt_version, PLAN_PROMPT_VERSION);
    assert_eq!(plan.citations, fixtures::sources());
    let sent = model.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].response_schema.is_some());
    assert_eq!(sent[0].purpose, ChatPurpose::PlanWriter);
}

#[tokio::test]
async fn failures_go_back_once_then_failing_steps_are_dropped() {
    let dosed = step_json("s2", "Spray 2 tbsp of soap");
    let model = Arc::new(ScriptedModel::completions(vec![
        answer(&[
            step_json("s1", "Remove the lowest infected leaves"),
            dosed.clone(),
        ]),
        answer(&[step_json("s1", "Remove the lowest infected leaves"), dosed]),
    ]));
    let loaded = Loaded::new();
    let plan = writer(&model).write(&loaded.input()).await.unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.dropped.len(), 1);
    assert_eq!(plan.dropped[0].rule, PlanRule::Dose);
    let sent = model.sent();
    assert_eq!(sent.len(), 2, "exactly one validation round-trip");
    let retry = &sent[1].messages;
    assert_eq!(
        retry.len(),
        3,
        "append-only: first prompt, the answer, the reasons"
    );
    assert_eq!(plan.usage.tokens_in, 200);
}

#[tokio::test]
async fn an_unparseable_answer_is_retried_once() {
    let model = Arc::new(ScriptedModel::completions(vec![
        ("not json".to_owned(), StopReason::EndTurn),
        answer(&[step_json("s1", "Remove the lowest infected leaves")]),
    ]));
    let loaded = Loaded::new();
    let plan = writer(&model).write(&loaded.input()).await.unwrap();
    assert_eq!(plan.steps.len(), 1);

    let model = Arc::new(ScriptedModel::completions(vec![
        ("not json".to_owned(), StopReason::EndTurn),
        ("still not".to_owned(), StopReason::EndTurn),
    ]));
    let err = writer(&model).write(&loaded.input()).await.unwrap_err();
    assert!(matches!(err, PlanWriterError::Output(_)));
}

#[tokio::test]
async fn a_refusal_is_an_error() {
    let model = Arc::new(ScriptedModel::completing("", StopReason::Refusal));
    let loaded = Loaded::new();
    let err = writer(&model).write(&loaded.input()).await.unwrap_err();
    assert!(matches!(err, PlanWriterError::Refused));
}

#[test]
fn the_prompt_wraps_data_and_names_what_the_plan_may_use() {
    let loaded = Loaded::new();
    let model = Arc::new(ScriptedModel::default());
    let request = writer(&model).request(&loaded.input());
    let ContentPart::Text(context) = &request.messages[0].content[0] else {
        panic!("context first");
    };
    assert!(context.starts_with("<data kind=\"context\">"));
    assert!(context.contains("remove_lower_infected_leaves"));
    assert!(context.contains("\"role\":\"beneficial\""));
    let ContentPart::Text(memories) = &request.messages[0].content[1] else {
        panic!("memories second");
    };
    assert!(memories.contains("I garden organically"));
    let ContentPart::Text(beliefs) = &request.messages[0].content[2] else {
        panic!("beliefs third");
    };
    assert!(beliefs.starts_with("<data kind=\"beliefs\">"));
    assert!(beliefs.contains(
        r#"{"key":"container","value":"The tomato is in a 30 cm pot.","about_plant":true,"date":"2026-10-16"}"#
    ));
    assert!(
        !beliefs.contains("learned_at") && !beliefs.contains("memory_id"),
        "key, value, about_plant and the day only"
    );
    assert_eq!(PLAN_SCHEMA["additionalProperties"], false);
}

#[test]
fn no_beliefs_means_no_beliefs_block() {
    let mut loaded = Loaded::new();
    loaded.beliefs.clear();
    let model = Arc::new(ScriptedModel::default());
    let request = writer(&model).request(&loaded.input());
    assert!(
        request.messages[0].content.iter().all(
            |c| !matches!(c, ContentPart::Text(t) if t.starts_with("<data kind=\"beliefs\">"))
        )
    );
}

// ---------------------------------------------------------------------------------------------
// Farm records (EON E5.4)
// ---------------------------------------------------------------------------------------------

/// A Slack message (`system: "slack"`) or Drive document hit with this vendor id and similarity.
fn farm_match(vendor: &str, system: &str, similarity: f32) -> FarmRecordMatch {
    FarmRecordMatch {
        hit: serde_json::from_value(json!({
            "text": format!("record {vendor}"),
            "citation": {
                "record_id": format!("01JAX3K9T6R2B8V0QZ4M7D1E5C.0.{vendor}"),
                "system": system, "title": "#field-reports", "author": "Riley",
                "at": "2026-08-31T14:00:00Z", "link": "http://localhost:4002/archives/C1/p1"
            }
        }))
        .unwrap(),
        similarity,
    }
}

fn vendor_ids(hits: &[FarmRecordHit]) -> Vec<&str> {
    hits.iter()
        .map(|h| h.citation.record_id.vendor_id.as_ref())
        .collect()
}

/// A garden with 5 matching Slack messages and 1 Drive document.
fn farm_with_records() -> MockFarmRecordStore {
    let mut farm = MockFarmRecordStore::new();
    farm.expect_garden_has_records().returning(|_| Ok(true));
    farm.expect_search_records_for_plan()
        .withf(|garden, _, filter, k| {
            garden.to_string() == crate::testing::GARDEN
                && *k == PLAN_FARM_HITS
                && filter.since.is_none()
                && filter.until.is_none()
                && filter.author.is_none()
        })
        .times(2)
        .returning(|_, _, filter, _| {
            Ok(match filter.kind {
                Some(FarmRecordKind::Message) => ["m1", "m2", "m3", "m4", "m5"]
                    .iter()
                    .zip([0.9, 0.8, 0.7, 0.6, 0.5])
                    .map(|(v, s)| farm_match(v, "slack", s))
                    .collect(),
                Some(FarmRecordKind::Document) => vec![farm_match("D_SPRAY", "google_drive", 0.3)],
                None => panic!("each kind is searched apart"),
            })
        });
    farm
}

fn query_embedder(expected: &'static str) -> MockTextEmbedder {
    let mut embedder = MockTextEmbedder::new();
    embedder
        .expect_embed()
        .withf(move |mode, texts| *mode == EmbedMode::Query && texts == [expected])
        .times(1)
        .returning(|_, _| Ok(vec![domain::ports::Embedding(vec![1.0, 0.0])]));
    embedder
}

fn garden() -> GardenId {
    crate::testing::GARDEN.parse().unwrap()
}

#[test]
fn the_farm_query_is_the_plant_and_the_leading_candidate() {
    let mut d = fixtures::diagnosis();
    assert_eq!(farm_query("Bed 3", &d), "Bed 3 Early blight");
    // Uncertain: the top candidate.
    d.status = DiagnosisStatus::Uncertain;
    d.primary = None;
    assert_eq!(farm_query("Bed 3", &d), "Bed 3 Early blight");
    d.status = DiagnosisStatus::Healthy;
    assert_eq!(farm_query("Bed 3", &d), "Bed 3");
}

/// One query, embedded once, searched per kind: the top 4, each kind keeping 2 when it has
/// them, so the spray log isn't crowded out by Slack messages.
#[tokio::test]
async fn the_plan_gets_the_top_four_farm_records() {
    let farm = farm_with_records();
    let embedder = query_embedder("Tomato by the fence Early blight");
    let query = farm_query("Tomato by the fence", &fixtures::diagnosis());
    let hits = plan_farm_records(&farm, &embedder, garden(), &query, FARM_SEARCH_TIMEOUT).await;
    assert_eq!(vendor_ids(&hits), ["m1", "m2", "m3", "D_SPRAY"]);
}

/// A garden without records is never embedded or searched.
#[tokio::test]
async fn a_garden_without_farm_records_is_not_searched() {
    let mut farm = MockFarmRecordStore::new();
    farm.expect_garden_has_records().returning(|_| Ok(false));
    farm.expect_search_records_for_plan().never();
    let mut embedder = MockTextEmbedder::new();
    embedder.expect_embed().never();
    let hits = plan_farm_records(&farm, &embedder, garden(), "Bed 3", FARM_SEARCH_TIMEOUT).await;
    assert_eq!(hits, Vec::new());
}

/// An embedder that answers after a minute.
struct SlowEmbedder;

#[async_trait::async_trait]
impl TextEmbedder for SlowEmbedder {
    fn model(&self) -> domain::ports::EmbeddingModel {
        unreachable!("not asked")
    }
    async fn embed(
        &self,
        _: EmbedMode,
        _: &[String],
    ) -> Result<Vec<domain::ports::Embedding>, domain::ports::ModelError> {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(Vec::new())
    }
}

/// A failed or slow search gives no farm records rather than failing the plan.
#[tokio::test(start_paused = true)]
async fn a_failed_or_slow_search_gives_no_farm_records() {
    let mut farm = MockFarmRecordStore::new();
    farm.expect_garden_has_records().returning(|_| Ok(true));
    farm.expect_search_records_for_plan()
        .returning(|_, _, _, _| Err(StoreError::Unavailable("offline".into())));
    let embedder = query_embedder("Bed 3");
    let hits = plan_farm_records(&farm, &embedder, garden(), "Bed 3", FARM_SEARCH_TIMEOUT).await;
    assert_eq!(hits, Vec::new());

    let mut farm = MockFarmRecordStore::new();
    farm.expect_garden_has_records().returning(|_| Ok(true));
    farm.expect_search_records_for_plan().never();
    let hits =
        plan_farm_records(&farm, &SlowEmbedder, garden(), "Bed 3", FARM_SEARCH_TIMEOUT).await;
    assert_eq!(hits, Vec::new());
}

/// The hits reach the model in a `farm_records` data block, after the beliefs; without them
/// there is no block and the request is the one written before connectors.
#[tokio::test]
async fn the_farm_records_block_holds_the_hits_and_is_absent_without_them() {
    let farm = farm_with_records();
    let embedder = query_embedder("Tomato by the fence Early blight");
    let mut loaded = Loaded::new();
    loaded.farm_records = plan_farm_records(
        &farm,
        &embedder,
        garden(),
        "Tomato by the fence Early blight",
        FARM_SEARCH_TIMEOUT,
    )
    .await;
    let model = Arc::new(ScriptedModel::default());
    let request = writer(&model).request(&loaded.input());
    let content = &request.messages[0].content;
    assert_eq!(
        content.len(),
        5,
        "context, memories, beliefs, farm records, the ask"
    );
    let ContentPart::Text(block) = &content[3] else {
        panic!("farm records fourth");
    };
    assert!(block.starts_with("<data kind=\"farm_records\">\n"));
    for id in ["m1", "m2", "m3", "D_SPRAY"] {
        assert!(
            block.contains(&format!(
                "\"record_id\":\"01JAX3K9T6R2B8V0QZ4M7D1E5C.0.{id}\""
            )),
            "{id}"
        );
    }
    assert!(
        !block.contains(".0.m4") && !block.contains(".0.m5"),
        "the top 4 only"
    );

    let without = Loaded::new();
    let plain = writer(&model).request(&without.input());
    assert_eq!(plain.messages[0].content.len(), 4);
    assert!(
        plain.messages[0].content.iter().all(
            |c| !matches!(c, ContentPart::Text(t) if t.contains("<data kind=\"farm_records\">"))
        )
    );
    assert_eq!(plain.system, request.system, "one system prompt for both");
}

/// A step citing a farm record it wasn't given goes back once; the plan stores the records
/// the passing steps cite, resolved.
#[tokio::test]
async fn cited_farm_records_are_resolved_and_unknown_ones_go_back() {
    let mut loaded = Loaded::new();
    loaded.farm_records = vec![crate::testing::spray_log_hit()];
    let spray_log = loaded.farm_records[0].citation.clone();
    let cites = |ids: &[&str]| {
        let mut s = step_json("s1", "Remove the lowest infected leaves");
        s["farm_citations"] = json!(ids);
        s
    };
    let unknown = cites(&["01JAX3K9T6R2B8V0QZ4M7D1E5C.0.C1/1.000100"]);
    let known = cites(&["01JAX3K9T6R2B8V0QZ4M7D1E5C.0.D_SPRAY"]);
    let model = Arc::new(ScriptedModel::completions(vec![
        answer(&[unknown]),
        answer(&[known]),
    ]));
    let plan = writer(&model).write(&loaded.input()).await.unwrap();
    assert_eq!(plan.dropped, Vec::new());
    assert_eq!(
        plan.steps[0].farm_citations,
        vec![spray_log.record_id.clone()]
    );
    assert_eq!(plan.farm_records, vec![spray_log]);
    let sent = model.sent();
    assert_eq!(sent.len(), 2, "one validation round-trip");
    let ContentPart::Text(reasons) = &sent[1].messages[2].content[0] else {
        panic!("the reasons");
    };
    assert!(reasons.contains(
        "farm record 01JAX3K9T6R2B8V0QZ4M7D1E5C.0.C1/1.000100 is not one you were given"
    ));
}

#[test]
fn the_plan_schema_offers_farm_citations() {
    let step = &PLAN_SCHEMA["$defs"]["PlanStepDraft"];
    let farm = &step["properties"]["farm_citations"];
    assert_eq!(farm["type"], "array");
    // Plain strings, with no pattern: the pattern made revise_plan's strict input schema too
    // complex for Anthropic's API, which refused every chat turn that offered the tool.
    assert_eq!(farm["items"], json!({ "type": "string" }));
    assert!(
        !step["required"]
            .as_array()
            .unwrap()
            .contains(&json!("farm_citations")),
        "optional: a garden without farm records writes the plan as before"
    );
}

/// When the second answer doesn't parse, the first validation stands, with its own farm
/// records: those its passing steps cite.
#[tokio::test]
async fn an_unparseable_second_answer_keeps_the_first_rounds_farm_records() {
    let mut loaded = Loaded::new();
    loaded.farm_records = vec![crate::testing::spray_log_hit()];
    let spray_log = loaded.farm_records[0].citation.clone();
    let mut cited = step_json("s1", "Remove the lowest infected leaves");
    cited["farm_citations"] = json!([spray_log.record_id]);
    let mut dosed = step_json("s2", "Spray 2 tbsp of soap");
    dosed["farm_citations"] = json!([spray_log.record_id]);
    let model = Arc::new(ScriptedModel::completions(vec![
        answer(&[cited, dosed]),
        ("not json".to_owned(), StopReason::EndTurn),
    ]));
    let plan = writer(&model).write(&loaded.input()).await.unwrap();
    assert_eq!(model.sent().len(), 2);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.dropped[0].rule, PlanRule::Dose);
    assert_eq!(
        plan.steps[0].farm_citations,
        vec![spray_log.record_id.clone()]
    );
    assert_eq!(plan.farm_records, vec![spray_log]);
}
