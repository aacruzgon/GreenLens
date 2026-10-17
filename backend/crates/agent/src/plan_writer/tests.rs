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
        r#"{"key":"container","value":"The tomato is in a 30 cm pot.","about_plant":true}"#
    ));
    assert!(
        !beliefs.contains("learned_at"),
        "only key, value and about_plant"
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
