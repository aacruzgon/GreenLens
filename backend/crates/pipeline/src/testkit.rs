//! Test builders for the pipeline's records.

use domain::{
    events::Stage,
    id::{ImageId, ScanId},
    jobs::RegionClass,
    records::{
        diagnosis::{Diagnosis, DiagnosisStatus, EvidenceBasis},
        observation::{
            KnowledgeRef, Observation, ObservationDetail, ObservationSource, ObservationStage,
        },
        plan::ActionPlan,
    },
    scan::{GuidedKind, Question, QuestionKind, QuestionStatus},
    url::Stored,
    vlm::SoilAspect,
    wire::{BBox, Confidence, Region, Run, Timestamp},
};

pub fn scan_id() -> ScanId {
    "01JAX3K9T6R2B8V0QZ4M7D1E50".parse().unwrap()
}

pub fn image_id() -> ImageId {
    "01JAX3K9T6R2B8V0QZ4M7D1E51".parse().unwrap()
}

pub fn now() -> Timestamp {
    "2026-10-18T14:00:00Z".parse().unwrap()
}

pub fn obs(
    stage: ObservationStage,
    pipeline_stage: Stage,
    source: ObservationSource,
    confidence: f64,
) -> Observation<Stored> {
    Observation {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E52".parse().unwrap(),
        scan_id: scan_id(),
        run: Run(1),
        stage,
        pipeline_stage,
        label: String::new(),
        knowledge_ref: None,
        role: None,
        detail: None,
        confidence: Confidence::new(confidence).unwrap().into(),
        region: Some(Region::unmasked(image_id(), BBox::FULL, None)),
        source,
        model_version: "test".into(),
        created_at: now(),
    }
}

pub fn segmentation_leaf() -> Observation<Stored> {
    let mut o = obs(
        ObservationStage::Segmentation,
        Stage::Segmentation,
        ObservationSource::Segmentation,
        0.8,
    );
    o.detail = Some(ObservationDetail {
        region_class: Some(RegionClass::Leaf),
        ..ObservationDetail::default()
    });
    o
}

pub fn species_obs(confidence: f64) -> Observation<Stored> {
    obs(
        ObservationStage::Species,
        Stage::Species,
        ObservationSource::Bioclip,
        confidence,
    )
}

pub fn disease_obs(problem: Option<&str>, confidence: f64) -> Observation<Stored> {
    let mut o = obs(
        ObservationStage::Disease,
        Stage::Disease,
        ObservationSource::Classifier,
        confidence,
    );
    o.knowledge_ref = problem.map(|p| KnowledgeRef::Disease(p.parse().unwrap()));
    o
}

pub fn texture_obs(value: &str) -> Observation<Stored> {
    let mut o = obs(
        ObservationStage::Soil,
        Stage::Guided,
        ObservationSource::Vlm,
        0.65,
    );
    o.detail = Some(ObservationDetail {
        aspect: Some(SoilAspect::Texture),
        value: Some(value.into()),
        guided_kind: Some(GuidedKind::JarTest),
        ..ObservationDetail::default()
    });
    o
}

pub fn diagnosis(run: u32, status: DiagnosisStatus) -> Diagnosis<Stored> {
    Diagnosis {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E60".parse().unwrap(),
        scan_id: scan_id(),
        run: Run(run),
        supersedes: None,
        status,
        low_evidence: false,
        primary: None,
        candidates: Vec::new(),
        organisms: Vec::new(),
        soil: Vec::new(),
        risks: Vec::new(),
        evidence_basis: EvidenceBasis::default(),
        citations: Vec::new(),
        model_versions: std::collections::BTreeMap::new(),
        created_at: now(),
    }
}

pub fn plan(d: &Diagnosis<Stored>) -> ActionPlan {
    ActionPlan {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E70".parse().unwrap(),
        scan_id: scan_id(),
        plant_id: "01JAX3K9T6R2B8V0QZ4M7D1E71".parse().unwrap(),
        version: 1,
        supersedes: None,
        provisional: d.status == DiagnosisStatus::Uncertain,
        diagnosis_id: d.id,
        open_question: None,
        constraints: Vec::new(),
        summary: String::new(),
        steps: Vec::new(),
        watch_for: Vec::new(),
        citations: Vec::new(),
        farm_records: Vec::new(),
        task_ids: Vec::new(),
        llm_model: "m".into(),
        prompt_version: "p".into(),
        created_at: now(),
    }
}

pub fn question() -> Question {
    Question {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E80".parse().unwrap(),
        run: Run(1),
        kind: QuestionKind::Indicator,
        text: "Rings?".into(),
        options: None,
        indicator_id: None,
        photo_kind: None,
        photo_hint: None,
        candidates: Vec::new(),
        asked_at: now(),
        continue_at: now(),
        status: QuestionStatus::Open,
    }
}
