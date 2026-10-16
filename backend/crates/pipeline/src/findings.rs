//! Reading the step's observations ([contracts 7], observation): what the readable rule, the
//! status rule, the measurable indicators and the diagnosis's `soil` and `organisms` need.
//! Pure functions over borrowed observations.
//!
//! **Lifetimes.** `'o` is the lifetime of the snapshot's observations; every view returned
//! here points into them and lives no longer than the step.

use std::collections::BTreeMap;

use domain::{
    events::Stage,
    id::{ImageId, ProblemOrHealthy},
    jobs::RegionClass,
    ports::StoredDepth,
    records::observation::{
        KnowledgeRef, Observation, ObservationSource, ObservationStage, OrganismRole,
    },
    scan::{GuidedKind, ImageKind, ScanImage},
    url::Stored,
    vlm::SoilAspect,
    wire::{Confidence, Run},
};
use knowledge::{candidates::ClassifierHit, status::FindingSummary};

use crate::measure::MeasuredObs;

fn medium(o: &Observation<Stored>) -> bool {
    o.confidence.confidence().at_least_medium()
}

fn image_of(o: &Observation<Stored>) -> Option<ImageId> {
    o.region.as_ref().map(|r| r.image_id)
}

/// The plant check observation's label when a plant was found.
pub const PLANT_FOUND: &str = "Plant found";
/// Its label when none was.
pub const NO_PLANT: &str = "No plant found";

/// The plant check's result, when it ran: `false` fails the scan with `no_plant_found`.
pub fn plant_present(observations: &[Observation<Stored>]) -> Option<bool> {
    observations
        .iter()
        .find(|o| o.stage == ObservationStage::PlantCheck)
        .map(|o| o.label != NO_PLANT)
}

/// The classifier's results across the leaf regions: (problem or healthy, confidence).
pub fn classifier_results(
    observations: &[Observation<Stored>],
) -> Vec<(ProblemOrHealthy, Confidence)> {
    observations
        .iter()
        .filter(|o| {
            o.stage == ObservationStage::Disease && o.source == ObservationSource::Classifier
        })
        .map(|o| {
            let p = match &o.knowledge_ref {
                Some(KnowledgeRef::Disease(id)) => ProblemOrHealthy::Problem(id.clone()),
                _ => ProblemOrHealthy::Healthy,
            };
            (p, o.confidence.confidence())
        })
        .collect()
}

/// The scan's classifier candidates ([contracts 5]): 3 distinct problems, healthy ignored.
pub fn classifier_hits(observations: &[Observation<Stored>]) -> Vec<ClassifierHit> {
    knowledge::candidates::classifier_hits(classifier_results(observations))
}

/// A plant is **readable** ([contracts 3]) when any of these holds: a leaf region was found;
/// the disease stage produced any result of medium confidence or more, `healthy` included;
/// `BioCLIP` or the VLM named a species with medium confidence or more; or the VLM findings
/// contain any finding of medium confidence or more.
pub fn readable(observations: &[Observation<Stored>]) -> bool {
    observations.iter().any(|o| match o.stage {
        ObservationStage::Segmentation => {
            o.detail.as_ref().and_then(|d| d.region_class) == Some(RegionClass::Leaf)
        }
        ObservationStage::Disease | ObservationStage::Species => medium(o),
        ObservationStage::Soil | ObservationStage::Organism | ObservationStage::Damage => {
            o.pipeline_stage == Stage::VlmFindings && medium(o)
        }
        ObservationStage::PlantCheck | ObservationStage::Measurement => false,
    })
}

/// What the status rule reads besides the scores ([contracts 7], diagnosis).
pub fn finding_summary(observations: &[Observation<Stored>]) -> FindingSummary {
    let pest_or_damage = observations.iter().any(|o| {
        medium(o)
            && match o.stage {
                ObservationStage::Organism => {
                    matches!(o.role, Some(OrganismRole::Pest | OrganismRole::Invasive))
                }
                ObservationStage::Damage => true,
                _ => false,
            }
    });
    let results = classifier_results(observations);
    let classifier_problem = results
        .iter()
        .any(|(p, c)| matches!(p, ProblemOrHealthy::Problem(_)) && c.at_least_medium());
    let classifier_healthy = results
        .iter()
        .max_by(|a, b| a.1.get().total_cmp(&b.1.get()))
        .is_some_and(|(p, c)| *p == ProblemOrHealthy::Healthy && c.at_least_medium());
    FindingSummary {
        pest_or_damage,
        classifier_problem,
        classifier_healthy,
    }
}

/// The run's VLM stage has stored its findings: `score` may diagnose.
pub fn vlm_done(observations: &[Observation<Stored>], run: Run) -> bool {
    observations
        .iter()
        .any(|o| o.pipeline_stage == Stage::VlmFindings && o.run == run)
}

/// `fx` in stored-image pixels, from the depth metadata (scaled if its reference width isn't
/// the stored width).
pub fn stored_fx(depth: &StoredDepth, stored_width: u32) -> f64 {
    let i = &depth.meta.intrinsics;
    if i.ref_width == 0 || i.ref_width == stored_width {
        i.fx
    } else {
        i.fx * f64::from(stored_width) / f64::from(i.ref_width)
    }
}

/// The measurement observations as the margin rule reads them.
pub fn measured<'o>(
    observations: &'o [Observation<Stored>],
    images: &[ScanImage<Stored>],
    depth: &[(ImageId, StoredDepth)],
) -> Vec<MeasuredObs<'o>> {
    observations
        .iter()
        .filter(|o| o.stage == ObservationStage::Measurement)
        .filter_map(|o| {
            let m = o.detail.as_ref()?.measurement.as_ref()?;
            let image_id = image_of(o)?;
            let image = images.iter().find(|i| i.id == image_id)?;
            let fx = depth
                .iter()
                .find(|(id, _)| *id == image_id)
                .map(|(_, d)| stored_fx(d, image.width));
            Some(MeasuredObs {
                image_id,
                kind: image.kind,
                measurement: m,
                confidence: o.confidence.confidence(),
                fx,
            })
        })
        .collect()
}

/// The soil observations a diagnosis uses: the VLM's, plus the latest guided check of each
/// kind ([contracts 3]: the diagnosis uses the latest check of each kind).
pub fn soil(observations: &[Observation<Stored>]) -> Vec<Observation<Stored>> {
    let mut latest: BTreeMap<u8, &Observation<Stored>> = BTreeMap::new();
    let mut out: Vec<Observation<Stored>> = Vec::new();
    for o in observations
        .iter()
        .filter(|o| o.stage == ObservationStage::Soil)
    {
        if o.pipeline_stage == Stage::Guided {
            let key = match o.detail.as_ref().and_then(|d| d.guided_kind) {
                Some(GuidedKind::SqueezeTest) => 0,
                Some(GuidedKind::JarTest) => 1,
                None => 2,
            };
            match latest.get(&key) {
                Some(kept) if kept.created_at >= o.created_at => {}
                _ => {
                    latest.insert(key, o);
                }
            }
        } else {
            out.push(o.clone());
        }
    }
    out.extend(latest.into_values().cloned());
    out
}

/// The soil texture a diagnosis records: each guided kind's texture value. Two diagnoses
/// differ in texture when these differ, or one has none ([contracts 7], re-planning).
pub fn texture(soil: &[Observation<Stored>]) -> Vec<(Option<GuidedKind>, String)> {
    let mut t: Vec<(Option<GuidedKind>, String)> = soil
        .iter()
        .filter_map(|o| {
            let d = o.detail.as_ref()?;
            (d.aspect == Some(SoilAspect::Texture))
                .then(|| (d.guided_kind, d.value.clone().unwrap_or_default()))
        })
        .collect();
    t.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    t
}

/// The images of the run with depth: the `measurement` stage runs only when there is one.
pub fn has_depth(images: &[ScanImage<Stored>]) -> bool {
    images
        .iter()
        .any(|i| i.depth.is_some() && i.kind.is_plant_or_soil() && i.answers_question_id.is_none())
}

/// The kinds of the scan's run images, for routing and question choice.
pub fn kinds(images: &[ScanImage<Stored>]) -> Vec<ImageKind> {
    images.iter().map(|i| i.kind).collect()
}

/// `model_versions` for a diagnosis: each source's model version from the observations used.
pub fn model_versions(observations: &[Observation<Stored>]) -> BTreeMap<String, String> {
    observations
        .iter()
        .map(|o| {
            let key = match o.source {
                ObservationSource::Segmentation => "segmentation",
                ObservationSource::Bioclip => "bioclip",
                ObservationSource::Classifier => "classifier",
                ObservationSource::Vlm => "vlm_model",
                ObservationSource::LlmFallback => "llm_fallback",
                ObservationSource::User => "user",
                ObservationSource::Measurement => "measurement",
            };
            (key.to_owned(), o.model_version.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    #[test]
    fn readable_rule() {
        let leaf = testkit::segmentation_leaf();
        assert!(readable(&[leaf]));
        let mut low_species = testkit::species_obs(0.3);
        assert!(!readable(std::slice::from_ref(&low_species)));
        low_species.confidence = Confidence::new(0.65).unwrap().into();
        assert!(readable(&[low_species]));
        assert!(!readable(&[]));
    }

    #[test]
    fn early_blight_at_071_is_a_classifier_problem() {
        let obs = [
            testkit::disease_obs(Some("tomato_early_blight"), 0.71),
            testkit::disease_obs(None, 0.2),
        ];
        let f = finding_summary(&obs);
        assert!(f.classifier_problem);
        assert!(!f.classifier_healthy);
        let healthy = [
            testkit::disease_obs(None, 0.9),
            testkit::disease_obs(Some("tomato_early_blight"), 0.3),
        ];
        let f = finding_summary(&healthy);
        assert!(f.classifier_healthy && !f.classifier_problem);
    }
}
