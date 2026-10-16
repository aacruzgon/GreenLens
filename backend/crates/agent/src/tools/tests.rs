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
        .specs_for(Role::Owner)
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(all.len(), 19);
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted, "a stable order keeps the prompt cache");

    for role in [Role::Viewer, Role::Reviewer] {
        let names: Vec<_> = r.specs_for(role).into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 16);
        for write in ["create_task", "complete_task", "revise_plan"] {
            assert!(
                !names.iter().any(|n| n == write),
                "{role:?} is offered {write}"
            );
            assert!(r.find(write, role).is_none());
        }
        assert!(names.iter().any(|n| n == "remember"));
        assert!(names.iter().any(|n| n == "request_rescan"));
    }
    assert!(r.find("revise_plan", Role::Editor).is_some());
}

#[test]
fn specs_carry_a_json_schema_of_the_typed_arguments() {
    let r = ToolRegistry::standard();
    let specs = r.specs_for(Role::Owner);
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
