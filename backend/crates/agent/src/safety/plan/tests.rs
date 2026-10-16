use domain::records::plan::{PlanTarget, PlanWhen, SAFETY_NOTE, StepCategory};

use super::*;
use crate::fixtures::{self, step};

struct Kb {
    treatments: Vec<TreatmentSeed>,
    organisms: Vec<OrganismSeed>,
    problems: Vec<ProblemSeed>,
    neighbours: Vec<PlanNeighbour>,
    sources: Vec<SourceId>,
    products: ProductCatalogue,
}

impl Kb {
    fn new() -> Self {
        Self {
            treatments: fixtures::treatments(),
            organisms: fixtures::organisms(),
            problems: fixtures::problems(),
            neighbours: fixtures::neighbours(),
            sources: fixtures::sources(),
            products: ProductCatalogue::new(&fixtures::products()),
        }
    }

    fn ctx(&self, beneficials_seen: bool) -> ValidationContext<'_> {
        ValidationContext {
            treatments: &self.treatments,
            products: &self.products,
            organisms: &self.organisms,
            problems: &self.problems,
            neighbours: &self.neighbours,
            beneficials_seen,
            known_sources: &self.sources,
        }
    }
}

fn draft(steps: Vec<PlanStepDraft>) -> PlanDraft {
    PlanDraft {
        summary: "Early blight on the lower leaves.".to_owned(),
        steps,
        watch_for: vec!["New spots on upper leaves within 5 days".to_owned()],
    }
}

fn rules(v: &PlanValidation) -> Vec<PlanRule> {
    v.failures.iter().map(|f| f.rule).collect()
}

#[test]
fn a_clean_plan_passes_unchanged() {
    let kb = Kb::new();
    let v = validate_plan(draft(vec![step("s1", 1)]), &kb.ctx(false));
    assert!(v.is_clean(), "{:?}", v.failures);
    assert_eq!(v.steps.len(), 1);
    assert_eq!(v.steps[0].category, StepCategory::Cultural);
    assert_eq!(v.steps[0].safety_note, None);
    assert_eq!(v.citations, fixtures::sources());
}

#[test]
fn doses_are_checked_in_every_text() {
    let kb = Kb::new();
    let mut s = step("s1", 1);
    s.detail = "Then spray 2 tbsp of soap.".to_owned();
    let mut d = draft(vec![s, step("s2", 2)]);
    d.summary = "Use a 1:10 mix.".to_owned();
    d.watch_for.push("Spray 5 ml again".to_owned());
    let v = validate_plan(d, &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::Dose, PlanRule::Dose, PlanRule::Dose]);
    assert_eq!(v.summary, FALLBACK_SUMMARY);
    assert_eq!(v.watch_for.len(), 1);
    assert_eq!(v.steps.len(), 1, "the failing step is dropped");
    assert_eq!(v.steps[0].order, 1, "renumbered");
}

#[test]
fn the_category_comes_from_the_treatment() {
    let kb = Kb::new();
    let mut s = step("s1", 1);
    s.category = StepCategory::Monitoring;
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert_eq!(v.steps[0].category, StepCategory::Cultural);

    let mut s = step("s1", 1);
    s.treatment_id = Some("made_up".parse().unwrap());
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::UnknownTreatment]);
}

#[test]
fn products_make_a_step_chemical_with_the_safety_note() {
    let kb = Kb::new();
    let mut s = step("s1", 1);
    s.treatment_id = None;
    s.title = "Spray neem oil on the leaves".to_owned();
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert!(v.is_clean());
    assert_eq!(v.steps[0].category, StepCategory::Chemical);
    assert_eq!(v.steps[0].safety_note.as_deref(), Some(SAFETY_NOTE));

    let mut s = step("s1", 1);
    s.treatment_id = Some("copper_fungicide".parse().unwrap());
    s.title = "Apply a fungicide".to_owned();
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert_eq!(v.steps[0].category, StepCategory::Chemical);
    assert_eq!(v.steps[0].safety_note.as_deref(), Some(SAFETY_NOTE));
}

#[test]
fn beneficials_are_never_targeted() {
    let kb = Kb::new();
    let mut s = step("s1", 1);
    s.targets = vec![PlanTarget::Organism("seven_spot_ladybird".parse().unwrap())];
    let v = validate_plan(draft(vec![s]), &kb.ctx(true));
    assert_eq!(rules(&v), [PlanRule::TargetsBeneficial]);

    let mut s = step("s1", 1);
    s.targets = vec![PlanTarget::Organism("unknown_beetle".parse().unwrap())];
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::UnknownTarget]);
}

#[test]
fn broad_spectrum_products_are_rejected_when_beneficials_were_seen() {
    let kb = Kb::new();
    let mk = || {
        let mut s = step("s1", 1);
        s.treatment_id = None;
        s.title = "Spray malathion on the aphids".to_owned();
        s.targets = vec![PlanTarget::Organism("green_peach_aphid".parse().unwrap())];
        s
    };
    let v = validate_plan(draft(vec![mk()]), &kb.ctx(true));
    assert_eq!(rules(&v), [PlanRule::BroadSpectrum]);
    let v = validate_plan(draft(vec![mk()]), &kb.ctx(false));
    assert!(v.is_clean());
}

#[test]
fn every_step_is_cited_from_known_sources() {
    let kb = Kb::new();
    let mut s = step("s1", 1);
    s.citations.clear();
    let mut t = step("s2", 2);
    t.citations = vec!["blog_post".parse().unwrap()];
    let v = validate_plan(draft(vec![s, t]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::Uncited, PlanRule::UnknownSource]);
    assert_eq!(v.steps.len(), 0);
}

#[test]
fn neighbour_steps_are_monitoring_steps_about_a_given_neighbour_and_its_problems() {
    let kb = Kb::new();
    let neighbour = fixtures::NEIGHBOUR.parse().unwrap();
    let monitor = |targets: Vec<PlanTarget>| {
        let mut s = step("s9", 9);
        s.treatment_id = None;
        s.title = "Check the pepper for aphids".to_owned();
        s.category = StepCategory::Monitoring;
        s.when = PlanWhen::Recurring {
            every_days: 3,
            until: None,
        };
        s.plant_id = Some(neighbour);
        s.targets = targets;
        s
    };
    let aphid = PlanTarget::Organism("green_peach_aphid".parse().unwrap());
    let v = validate_plan(draft(vec![monitor(vec![aphid.clone()])]), &kb.ctx(false));
    assert!(
        v.is_clean(),
        "an organism the problem is caused_by: {:?}",
        v.failures
    );

    let blight = PlanTarget::Disease("tomato_early_blight".parse().unwrap());
    let v = validate_plan(draft(vec![monitor(vec![blight])]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::NeighbourTarget]);

    let mut s = monitor(vec![aphid.clone()]);
    s.category = StepCategory::Mechanical;
    s.title = "Spray the pepper with water".to_owned();
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::NeighbourNotMonitoring]);

    let mut s = monitor(vec![aphid]);
    s.plant_id = Some(fixtures::PLANT.parse().unwrap());
    let v = validate_plan(draft(vec![s]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::UnknownNeighbour]);
}

#[test]
fn structural_checks() {
    let kb = Kb::new();
    let mut s = step("s2", 2);
    s.when = PlanWhen::Recurring {
        every_days: 0,
        until: None,
    };
    let v = validate_plan(draft(vec![step("s1", 1), step("s1", 2), s]), &kb.ctx(false));
    assert_eq!(rules(&v), [PlanRule::DuplicateStep, PlanRule::InvalidWhen]);
    assert!(
        v.retry_message()
            .contains("step s1: another step has the same id")
    );
}
