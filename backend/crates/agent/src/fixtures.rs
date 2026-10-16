//! Knowledge-base fixtures shared by the crate's unit tests: a tomato with early blight and
//! aphids, ladybirds seen, and one neighbour.

use domain::{
    id::{PlantId, SourceId},
    knowledge::{OrganismSeed, ProblemSeed, ProductSeed, ProductsSeed, TreatmentSeed},
    records::plan::{PlanNeighbour, PlanStepDraft, PlanTarget, PlanWhen, StepCategory},
};
use serde_json::json;

pub const NEIGHBOUR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5G";
pub const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5F";

fn from<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> T {
    serde_json::from_value(v).unwrap()
}

pub fn problems() -> Vec<ProblemSeed> {
    let base = |id: &str, kind: &str, caused_by: &[&str], treatments: &[&str]| {
        from::<ProblemSeed>(json!({
            "id": id, "name": id, "kind": kind, "affects": ["tomato"], "caused_by": caused_by,
            "damage_patterns": [], "prevalence": "common", "indicators": [],
            "conditions": {}, "looks_like": [], "treatments": treatments,
            "sources": ["ext_tomato"], "reviewed_by": "teammate", "reviewed_at": "2026-10-20"
        }))
    };
    vec![
        base(
            "tomato_early_blight",
            "fungal",
            &[],
            &["remove_lower_infected_leaves", "copper_fungicide"],
        ),
        base(
            "aphid_infestation",
            "pest",
            &["green_peach_aphid"],
            &["water_spray"],
        ),
    ]
}

pub fn treatments() -> Vec<TreatmentSeed> {
    let t = |id: &str, kind: &str| {
        from::<TreatmentSeed>(
            json!({"id": id, "kind": kind, "text": id, "sources": ["ext_tomato"]}),
        )
    };
    vec![
        t("remove_lower_infected_leaves", "cultural"),
        t("copper_fungicide", "chemical"),
        t("water_spray", "mechanical"),
    ]
}

pub fn organisms() -> Vec<OrganismSeed> {
    let o = |id: &str, role: &str| {
        from::<OrganismSeed>(json!({
            "id": id, "name": id, "scientific_name": id, "taxon_group": "insect", "role": role,
            "recognition": "small", "bioclip_label": id, "sources": ["ext_tomato"],
            "reviewed_by": "teammate"
        }))
    };
    vec![
        o("green_peach_aphid", "pest"),
        o("seven_spot_ladybird", "beneficial"),
    ]
}

pub fn products() -> ProductsSeed {
    ProductsSeed {
        products: vec![ProductSeed {
            name: "malathion".to_owned(),
            broad_spectrum: true,
        }],
    }
}

pub fn neighbours() -> Vec<PlanNeighbour> {
    vec![PlanNeighbour {
        plant_id: NEIGHBOUR.parse::<PlantId>().unwrap(),
        name: "Pepper".to_owned(),
        species_name: "Pepper".to_owned(),
        distance_m: 0.6,
        problems: vec!["aphid_infestation".parse().unwrap()],
    }]
}

pub fn sources() -> Vec<SourceId> {
    vec!["ext_tomato".parse().unwrap()]
}

/// A clean cultural step.
pub fn step(id: &str, order: u32) -> PlanStepDraft {
    PlanStepDraft {
        id: id.parse().unwrap(),
        order,
        title: "Remove the lowest infected leaves".to_owned(),
        detail: "Bag them; don't compost them.".to_owned(),
        treatment_id: Some("remove_lower_infected_leaves".parse().unwrap()),
        category: StepCategory::Cultural,
        when: PlanWhen::Now,
        targets: vec![PlanTarget::Disease("tomato_early_blight".parse().unwrap())],
        plant_id: None,
        citations: sources(),
    }
}

pub const SCAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5H";
pub const DIAGNOSIS: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5J";
pub const OBSERVATION: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5K";

/// A confirmed early-blight diagnosis with ladybirds seen.
pub fn diagnosis() -> domain::records::diagnosis::Diagnosis<domain::url::Stored> {
    let candidate = json!({
        "problem_id": "tomato_early_blight", "name": "Early blight", "kind": "fungal",
        "score": 5, "matched": [], "contradicted": [], "unsure": [], "weather_fit": "favourable"
    });
    from(json!({
        "id": DIAGNOSIS, "scan_id": SCAN, "run": 1, "status": "confirmed", "low_evidence": false,
        "primary": candidate, "candidates": [candidate],
        "organisms": [{"observation_id": OBSERVATION, "organism_id": "seven_spot_ladybird",
                       "name": "Seven-spot ladybird", "role": "beneficial"}],
        "soil": [], "risks": [],
        "evidence_basis": {"user_input": [], "photo": [], "modelled": []},
        "citations": [], "model_versions": {}, "created_at": "2026-10-16T10:00:00Z"
    }))
}

/// A memory of the given source.
pub fn memory(id: &str, source: &str, shared: bool, text: &str) -> domain::chat::Memory {
    from(json!({
        "id": id, "garden_id": NEIGHBOUR, "kind": "fact", "text": text, "source": source,
        "shared": shared, "created_at": "2026-10-16T10:00:00Z"
    }))
}
