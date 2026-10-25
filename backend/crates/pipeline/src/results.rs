//! Turning a worker's result into what the store keeps ([contracts 5], results and partial
//! results; [contracts 7], observation): observations per pipeline stage, and the VLM's
//! indicator answers as evidence. Pure, except for the lookups passed in.

use std::collections::BTreeMap;

use domain::{
    events::Stage,
    id::{ImageId, ScanId, SpeciesId},
    id::{IndicatorId, ObservationId, ProblemOrHealthy},
    jobs::{PerceptionRegion, PerceptionResult, SpeciesResult},
    knowledge::{ClassifierLabels, IndicatorSeed, OrganismSeed},
    ports::{NewMask, ObservationWrite},
    records::{
        diagnosis::{EvidenceSource, IndicatorEvidence},
        observation::{
            KnowledgeRef, Observation, ObservationDetail, ObservationSource, ObservationStage,
            OrganismRole,
        },
    },
    scan::{CandidateSource, SpeciesCandidate},
    url::Stored,
    vlm::{VlmFindings, VlmIndicatorAnswers},
    wire::{BBox, Confidence, Rated, Region, Run, Timestamp},
};

use crate::findings::{NO_PLANT, PLANT_FOUND};

/// What every observation of one result shares.
#[derive(Debug, Clone, Copy)]
pub struct ObsContext<'a> {
    /// The scan.
    pub scan: ScanId,
    /// The run.
    pub run: Run,
    /// `vlm` or `llm_fallback` for VLM findings, by the worker's `model_versions`. On a fallback
    /// perception result it also marks the disease stage `llm_fallback`, not `classifier`: the
    /// hosted LLM made that call, and only the disease head's adds `classifier_weight`.
    pub vlm_source: ObservationSource,
    /// The model version recorded on each observation.
    pub model_version: &'a str,
    /// Now.
    pub now: Timestamp,
}

/// Lookups the conversion needs, borrowed for the call: problem and species names, organism
/// names and roles, and the disease classes the knowledge base maps.
#[derive(Debug, Clone, Copy)]
pub struct Lookups<'a> {
    /// The disease classes kept as observations.
    pub classifier: &'a MappedClasses,
    /// Problem names, for classifier labels.
    pub problem_names: &'a BTreeMap<String, String>,
    /// Species' common names, for `BioCLIP`'s labels: the worker sends the label matrix's
    /// text ("Solanum lycopersicum with common name tomato"), and an observation's `label` is
    /// display text ([contracts 7], observation).
    pub species_names: &'a BTreeMap<String, String>,
    /// Organism profiles, for names and roles.
    pub organisms: &'a [OrganismSeed],
}

/// The knowledge a result's observations are named from, owned, so a partial result and the
/// job's `complete` build their observations from the same names ([`keep_partial_ids`]).
#[derive(Debug, Clone, Default)]
pub struct Names {
    /// The disease classes kept as observations.
    pub classifier: MappedClasses,
    /// Problem names by id.
    pub problems: BTreeMap<String, String>,
    /// Species' common names by id.
    pub species: BTreeMap<String, String>,
    /// The organisms the VLM named.
    pub organisms: Vec<OrganismSeed>,
}

impl Names {
    /// The names as the conversion borrows them.
    pub fn lookups(&self) -> Lookups<'_> {
        Lookups {
            classifier: &self.classifier,
            problem_names: &self.problems,
            species_names: &self.species,
            organisms: &self.organisms,
        }
    }
}

/// The disease classes the knowledge base maps for the species the worker classified
/// ([contracts 5], classifier candidates): the `classifier_label` rows of that species. The
/// disease head can return classes without a row, for a species whose problems aren't in the
/// knowledge base yet; those never become observations, so they name no candidate and count in
/// no status rule, and a species without rows gets no classifier result at all, `healthy`
/// included (#144).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MappedClasses {
    classes: Vec<ProblemOrHealthy>,
}

impl MappedClasses {
    /// The rows of `species` in `labels`; none without a species.
    pub fn new(species: Option<&SpeciesId>, labels: &ClassifierLabels) -> Self {
        let classes = labels
            .labels
            .iter()
            .filter(|l| species == Some(&l.species_id))
            .map(|l| l.problem_id.clone())
            .collect();
        Self { classes }
    }

    /// Whether a class of the head's result is kept.
    pub fn keeps(&self, class: &ProblemOrHealthy) -> bool {
        self.classes.contains(class)
    }
}

/// The species the worker ran the disease head for, by the worker's rule (`vision`'s
/// perception, [contracts 5]): the run's working species, which a run starts with when the
/// species is confirmed or chosen, else `BioCLIP`'s first candidate, from the result or, for a
/// partial result, from the run's stored species observations. `None` when that candidate
/// names no species.
pub fn disease_species(
    working: Option<&SpeciesId>,
    result: &PerceptionResult,
    stored: &[Observation<Stored>],
    run: Run,
) -> Option<SpeciesId> {
    if let Some(id) = working {
        return Some(id.clone());
    }
    if let Some(species) = &result.species {
        return species
            .candidates
            .first()
            .and_then(|c| c.species_id.clone());
    }
    stored
        .iter()
        .filter(|o| {
            o.stage == ObservationStage::Species
                && o.source == ObservationSource::Bioclip
                && o.run == run
        })
        .max_by(|a, b| {
            a.confidence
                .confidence()
                .get()
                .total_cmp(&b.confidence.confidence().get())
        })
        .and_then(|o| match &o.knowledge_ref {
            Some(KnowledgeRef::Species(id)) => Some(id.clone()),
            _ => None,
        })
}

fn obs(
    ctx: &ObsContext<'_>,
    id: ObservationId,
    stage: ObservationStage,
    pipeline_stage: Stage,
    source: ObservationSource,
    label: String,
    confidence: Confidence,
) -> Observation<Stored> {
    Observation {
        id,
        scan_id: ctx.scan,
        run: ctx.run,
        stage,
        pipeline_stage,
        label,
        knowledge_ref: None,
        role: None,
        detail: None,
        confidence: Rated(confidence),
        region: None,
        source,
        model_version: ctx.model_version.to_owned(),
        created_at: ctx.now,
    }
}

/// The perception result's observations, one write per stage it ran, with no masks: a job's
/// completion writes each region's RLE as a PNG to object storage first and then adds the
/// records with [`attach_masks`]; a partial result stores none. Disease results keep only the
/// classes [`Lookups::classifier`] maps ([`MappedClasses`]).
pub fn perception_writes(
    ctx: &ObsContext<'_>,
    result: &PerceptionResult,
    images: &[ImageId],
    lookups: &Lookups<'_>,
    mut new_id: impl FnMut() -> ObservationId,
) -> Vec<ObservationWrite> {
    let write = |stage: Stage, observations: Vec<Observation<Stored>>| ObservationWrite {
        stage,
        run: ctx.run,
        images: images.to_vec(),
        replace_imageless: true,
        observations,
        masks: Vec::new(),
    };
    let mut writes = Vec::new();
    if let Some(present) = result.plant_present {
        let label = if present { PLANT_FOUND } else { NO_PLANT };
        let o = obs(
            ctx,
            new_id(),
            ObservationStage::PlantCheck,
            Stage::PlantCheck,
            ObservationSource::Segmentation,
            label.to_owned(),
            Confidence::HIGH,
        );
        writes.push(write(Stage::PlantCheck, vec![o]));
    }
    if !result.regions.is_empty() {
        let regions = result
            .regions
            .iter()
            .map(|r| region_obs(ctx, r, new_id()))
            .collect();
        writes.push(write(Stage::Segmentation, regions));
    }
    if let Some(species) = &result.species {
        writes.push(write(
            Stage::Species,
            species_obs(ctx, species, lookups, &mut new_id),
        ));
    }
    if let Some(disease) = &result.disease {
        let disease_source = if ctx.vlm_source == ObservationSource::LlmFallback {
            ObservationSource::LlmFallback
        } else {
            ObservationSource::Classifier
        };
        let mut list = Vec::new();
        for d in disease {
            for top in d
                .top
                .iter()
                .filter(|t| lookups.classifier.keeps(&t.problem_id))
            {
                let (label, knowledge_ref) = match &top.problem_id {
                    ProblemOrHealthy::Healthy => ("Healthy".to_owned(), None),
                    ProblemOrHealthy::Problem(p) => (
                        lookups
                            .problem_names
                            .get(p.as_str())
                            .cloned()
                            .unwrap_or_else(|| p.to_string()),
                        Some(KnowledgeRef::Disease(p.clone())),
                    ),
                };
                let mut o = obs(
                    ctx,
                    new_id(),
                    ObservationStage::Disease,
                    Stage::Disease,
                    disease_source,
                    label,
                    top.confidence,
                );
                o.knowledge_ref = knowledge_ref;
                o.region = Some(Region::unmasked(d.image_id, BBox::FULL, d.region_id));
                list.push(o);
            }
        }
        writes.push(write(Stage::Disease, list));
    }
    writes
}

/// Adds stored masks to the segmentation write of [`perception_writes`]: each mask whose region
/// has an observation there is recorded with it, and that observation's region gets its
/// `mask_id`, so a response or an event signs a `mask_url` ([contracts 1], regions). A mask
/// whose region isn't in the write is left out, so no record outlines a region that isn't stored.
pub fn attach_masks(writes: &mut [ObservationWrite], masks: Vec<NewMask>) {
    let Some(write) = writes.iter_mut().find(|w| w.stage == Stage::Segmentation) else {
        return;
    };
    for mask in masks {
        let region = write
            .observations
            .iter_mut()
            .filter_map(|o| o.region.as_mut())
            .find(|r| r.region_id == Some(mask.region_id));
        if let Some(region) = region {
            region.mask.mask_id = Some(mask.id);
            write.masks.push(mask);
        }
    }
}

/// Keeps the ids of the findings a partial result stored ([contracts 5], partial results: "if
/// `complete` matches the partial result, nothing changes"). An observation of `writes` equal to
/// one in `stored` that the write replaces (same pipeline stage, on one of the write's images or
/// imageless when it replaces those), apart from its id, model version and time, takes that
/// one's id, so the store's diff neither retracts it nor sends it again. A finding that changed,
/// such as a region that gained its mask, keeps its new id and is replaced.
pub fn keep_partial_ids(writes: &mut [ObservationWrite], stored: &[Observation<Stored>]) {
    let mut taken: Vec<ObservationId> = Vec::new();
    for write in writes.iter_mut() {
        let replaced = |s: &&Observation<Stored>| {
            s.pipeline_stage == write.stage
                && match &s.region {
                    Some(r) => write.images.contains(&r.image_id),
                    None => write.replace_imageless,
                }
        };
        let candidates: Vec<&Observation<Stored>> = stored.iter().filter(replaced).collect();
        for o in &mut write.observations {
            let same = candidates
                .iter()
                .find(|s| !taken.contains(&s.id) && same_finding(s, o));
            if let Some(s) = same {
                taken.push(s.id);
                o.id = s.id;
            }
        }
    }
}

/// `a` and `b` are the same finding: equal but for the id, the model version (a partial
/// result's is `partial`) and the time.
fn same_finding(a: &Observation<Stored>, b: &Observation<Stored>) -> bool {
    let mut b = b.clone();
    b.id = a.id;
    b.model_version.clone_from(&a.model_version);
    b.created_at = a.created_at;
    *a == b
}

fn region_obs(
    ctx: &ObsContext<'_>,
    r: &PerceptionRegion,
    id: ObservationId,
) -> Observation<Stored> {
    let label = match r.class {
        domain::jobs::RegionClass::Plant => "Plant",
        domain::jobs::RegionClass::Leaf => "Leaf",
        domain::jobs::RegionClass::Soil => "Soil",
        domain::jobs::RegionClass::Surroundings => "Surroundings",
    };
    let mut o = obs(
        ctx,
        id,
        ObservationStage::Segmentation,
        Stage::Segmentation,
        ObservationSource::Segmentation,
        label.to_owned(),
        r.confidence,
    );
    o.detail = Some(ObservationDetail {
        region_class: Some(r.class),
        ..ObservationDetail::default()
    });
    o.region = Some(Region::unmasked(r.image_id, r.bbox, Some(r.region_id)));
    o
}

fn species_obs(
    ctx: &ObsContext<'_>,
    species: &SpeciesResult,
    lookups: &Lookups<'_>,
    new_id: &mut impl FnMut() -> ObservationId,
) -> Vec<Observation<Stored>> {
    species
        .candidates
        .iter()
        .map(|c| {
            let label = c
                .species_id
                .as_ref()
                .and_then(|id| lookups.species_names.get(id.as_str()))
                .cloned()
                .unwrap_or_else(|| c.label.clone());
            let mut o = obs(
                ctx,
                new_id(),
                ObservationStage::Species,
                Stage::Species,
                ObservationSource::Bioclip,
                label,
                c.score,
            );
            o.knowledge_ref = c.species_id.clone().map(KnowledgeRef::Species);
            o
        })
        .collect()
}

/// The VLM findings' observations (`pipeline_stage: vlm_findings`): the species guess, soil,
/// organisms (role from the knowledge base, `unknown` for "other") and damage.
/// The VLM's species guess as display text: the name it gave a species outside the list, or the
/// listed species' common name.
fn guess_label(g: &domain::vlm::SpeciesGuess, lookups: &Lookups<'_>) -> String {
    g.other_name
        .clone()
        .or_else(|| {
            g.species_id.as_ref().map(|id| {
                lookups
                    .species_names
                    .get(id.as_str())
                    .cloned()
                    .unwrap_or_else(|| id.to_string())
            })
        })
        .unwrap_or_default()
}

pub fn findings_write(
    ctx: &ObsContext<'_>,
    findings: &VlmFindings,
    images: &[ImageId],
    lookups: &Lookups<'_>,
    mut new_id: impl FnMut() -> ObservationId,
) -> ObservationWrite {
    let src = ctx.vlm_source;
    let mut list = Vec::new();
    let g = &findings.species_guess;
    let mut guess = obs(
        ctx,
        new_id(),
        ObservationStage::Species,
        Stage::VlmFindings,
        src,
        guess_label(g, lookups),
        g.confidence,
    );
    guess.knowledge_ref = g.species_id.clone().map(KnowledgeRef::Species);
    list.push(guess);
    for s in &findings.soil {
        let mut o = obs(
            ctx,
            new_id(),
            ObservationStage::Soil,
            Stage::VlmFindings,
            src,
            format!("Soil: {}", s.value),
            s.confidence,
        );
        o.detail = Some(ObservationDetail {
            aspect: Some(s.aspect),
            value: Some(s.value.clone()),
            ..ObservationDetail::default()
        });
        o.region = Some(Region::unmasked(s.image_id, BBox::FULL, None));
        list.push(o);
    }
    for f in &findings.organisms {
        let seed = f
            .organism_id
            .as_ref()
            .and_then(|id| lookups.organisms.iter().find(|o| &o.id == id));
        let label = seed
            .map(|s| s.name.clone())
            .or_else(|| f.other_name.clone())
            .unwrap_or_else(|| "Unknown organism".to_owned());
        let mut o = obs(
            ctx,
            new_id(),
            ObservationStage::Organism,
            Stage::VlmFindings,
            src,
            label,
            f.confidence,
        );
        o.knowledge_ref = f.organism_id.clone().map(KnowledgeRef::Organism);
        o.role = Some(seed.map_or(OrganismRole::Unknown, |s| s.role.into()));
        o.detail = Some(ObservationDetail {
            count: Some(f.count),
            ..ObservationDetail::default()
        });
        o.region = Some(f.region.clone());
        list.push(o);
    }
    for d in &findings.damage {
        let label = d
            .other_description
            .clone()
            .unwrap_or_else(|| format!("{:?}", d.pattern));
        let mut o = obs(
            ctx,
            new_id(),
            ObservationStage::Damage,
            Stage::VlmFindings,
            src,
            label,
            d.confidence,
        );
        o.detail = Some(ObservationDetail {
            pattern: Some(d.pattern),
            ..ObservationDetail::default()
        });
        o.region = Some(
            d.region
                .clone()
                .unwrap_or_else(|| Region::unmasked(d.image_id, BBox::FULL, None)),
        );
        list.push(o);
    }
    ObservationWrite {
        stage: Stage::VlmFindings,
        run: ctx.run,
        images: images.to_vec(),
        replace_imageless: true,
        observations: list,
        masks: Vec::new(),
    }
}

fn wire_name<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// A guided check's texture observation (`pipeline_stage: guided`, aspect `texture`); `None`
/// for a jar that hadn't settled ([contracts 6], after decoding).
pub fn guided_observation(
    ctx: &ObsContext<'_>,
    texture: &domain::vlm::GuidedTexture,
    image: ImageId,
    id: ObservationId,
) -> Option<Observation<Stored>> {
    use domain::{scan::GuidedKind, vlm::GuidedTexture as T};
    let (kind, value, confidence) = match texture {
        T::SqueezeTest { result, confidence } => {
            (GuidedKind::SqueezeTest, wire_name(result), *confidence)
        }
        T::JarTest { settled: false, .. } => return None,
        T::JarTest {
            texture,
            confidence,
            ..
        } => (GuidedKind::JarTest, wire_name(texture), *confidence),
    };
    let mut o = obs(
        ctx,
        id,
        ObservationStage::Soil,
        Stage::Guided,
        ctx.vlm_source,
        format!("Soil texture: {value}"),
        confidence,
    );
    o.detail = Some(ObservationDetail {
        aspect: Some(domain::vlm::SoilAspect::Texture),
        value: Some(value),
        guided_kind: Some(kind),
        ..ObservationDetail::default()
    });
    o.region = Some(Region::unmasked(image, BBox::FULL, None));
    Some(o)
}

/// The VLM's indicator answers as `vlm` evidence. An answer to an indicator the job didn't
/// ask is dropped.
pub fn vlm_evidence(
    answers: &VlmIndicatorAnswers,
    asked: &[&IndicatorSeed],
) -> Vec<IndicatorEvidence> {
    let by_id: BTreeMap<&IndicatorId, &IndicatorSeed> = asked.iter().map(|i| (&i.id, *i)).collect();
    answers
        .answers
        .iter()
        .filter_map(|a| {
            let ind = by_id.get(&a.indicator_id)?;
            Some(IndicatorEvidence {
                indicator_id: a.indicator_id.clone(),
                text: ind.text.clone(),
                weight: ind.weight,
                answer: a.answer,
                reason: a.reason.clone(),
                source: EvidenceSource::Vlm,
                measured: None,
            })
        })
        .collect()
}

/// The species candidates and suggestion after the VLM's findings ([architecture 6.2]):
/// `BioCLIP`'s top 3 and the VLM's guess when it names a different species; the working
/// species is the VLM's guess when `BioCLIP` also lists it, otherwise `BioCLIP`'s best.
pub fn species_suggestion(
    bioclip: &[(Option<domain::id::SpeciesId>, String, Confidence)],
    vlm: Option<(Option<domain::id::SpeciesId>, String, Confidence)>,
) -> (Vec<SpeciesCandidate<Stored>>, Option<domain::id::SpeciesId>) {
    let mut candidates: Vec<SpeciesCandidate<Stored>> = bioclip
        .iter()
        .take(3)
        .map(|(id, name, score)| SpeciesCandidate {
            species_id: id.clone(),
            name: name.clone(),
            score: *score,
            source: CandidateSource::Bioclip,
            reference_image: None,
        })
        .collect();
    let mut working = bioclip.first().and_then(|b| b.0.clone());
    if let Some((id, name, score)) = vlm {
        if id.is_some() && bioclip.iter().any(|b| b.0 == id) {
            working.clone_from(&id);
        } else {
            candidates.push(SpeciesCandidate {
                species_id: id,
                name,
                score,
                source: CandidateSource::Vlm,
                reference_image: None,
            });
        }
    }
    (candidates, working)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlm_answers_become_evidence_for_asked_indicators_only() {
        let ind: IndicatorSeed = serde_json::from_value(serde_json::json!({
            "id": "p_one.rings", "text": "Rings", "question": "Rings?", "weight": 3,
            "discriminating": true, "where": "lower_leaves", "image_kinds": ["leaf_closeup"]
        }))
        .unwrap();
        let answers: VlmIndicatorAnswers = serde_json::from_value(serde_json::json!({"answers": [
            {"indicator_id": "p_one.rings", "answer": "yes", "reason": "clear rings"},
            {"indicator_id": "p_two.halo", "answer": "no", "reason": "-"}
        ]}))
        .unwrap();
        let ev = vlm_evidence(&answers, &[&ind]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].weight.get(), 3);
    }

    fn ctx(model_version: &str) -> ObsContext<'_> {
        ObsContext {
            scan: "01JAX3K9T6R2B8V0QZ4M7D1E50".parse().unwrap(),
            run: Run(1),
            vlm_source: ObservationSource::Vlm,
            model_version,
            now: "2026-10-18T14:00:00Z".parse().unwrap(),
        }
    }

    /// A segmentation region on the plant photo, the species top 2 and one disease top.
    fn result() -> PerceptionResult {
        serde_json::from_value(serde_json::json!({
            "plant_present": true,
            "regions": [{
                "region_id": "01JAX3K9T6R2B8V0QZ4M7D1E5A.1",
                "image_id": "01JAX3K9T6R2B8V0QZ4M7D1E5A",
                "class": "leaf",
                "bbox": [0.1, 0.1, 0.5, 0.5],
                "confidence": 0.9
            }],
            "species": {"candidates": [
                {"species_id": "tomato", "label": "Solanum lycopersicum with common name tomato", "score": 0.6},
                {"species_id": null, "label": "a weed", "score": 0.1}
            ]},
            "disease": [{
                "image_id": "01JAX3K9T6R2B8V0QZ4M7D1E5A",
                "region_id": "01JAX3K9T6R2B8V0QZ4M7D1E5A.1",
                "top": [{"problem_id": "tomato_late_blight", "confidence": 0.32}]
            }]
        }))
        .unwrap()
    }

    fn names() -> Names {
        Names {
            classifier: MappedClasses::new(Some(&"tomato".parse().unwrap()), &labels()),
            problems: BTreeMap::from([("tomato_late_blight".into(), "Late blight".into())]),
            species: BTreeMap::from([("tomato".into(), "Tomato".into())]),
            organisms: Vec::new(),
        }
    }

    /// `classifier_labels.yaml` as it stands after #134: tomato's classes only, here three of
    /// them.
    fn labels() -> ClassifierLabels {
        serde_json::from_value(serde_json::json!({
            "model_version": "disease-head-v3c-2026-10-20",
            "labels": [
                {"index": 11, "label": "tomato/healthy", "species_id": "tomato", "problem_id": "healthy"},
                {"index": 13, "label": "tomato/tomato_early_blight", "species_id": "tomato",
                 "problem_id": "tomato_early_blight"},
                {"index": 14, "label": "tomato/tomato_late_blight", "species_id": "tomato",
                 "problem_id": "tomato_late_blight"}
            ]
        }))
        .unwrap()
    }

    /// The disease observations a result's top 3 leaves, by label, for a species classified as
    /// `species`.
    fn kept(species: &str, top: &serde_json::Value) -> Vec<Observation<Stored>> {
        let result: PerceptionResult = serde_json::from_value(serde_json::json!({
            "regions": [],
            "disease": [{"image_id": "01JAX3K9T6R2B8V0QZ4M7D1E5A", "top": top}]
        }))
        .unwrap();
        let names = Names {
            classifier: MappedClasses::new(Some(&species.parse().unwrap()), &labels()),
            ..Names::default()
        };
        let mut n = 0;
        perception_writes(&ctx("v1"), &result, &[], &names.lookups(), || {
            n += 1;
            format!("01JAX3K9T6R2B8V0QZ4M7D1E{n:02}").parse().unwrap()
        })
        .into_iter()
        .filter(|w| w.stage == Stage::Disease)
        .flat_map(|w| w.observations)
        .collect()
    }

    /// #144: the disease head also returns apple's classes, which `classifier_labels.yaml`
    /// doesn't map yet, so an apple scan gets no classifier result: no candidate, no `healthy`.
    #[test]
    fn a_species_without_classifier_label_rows_gets_no_disease_results() {
        let apple = kept(
            "apple",
            &serde_json::json!([
                {"problem_id": "apple_scab", "confidence": 0.8},
                {"problem_id": "healthy", "confidence": 0.15},
                {"problem_id": "apple_cedar_apple_rust", "confidence": 0.05}
            ]),
        );
        assert!(apple.is_empty(), "{apple:?}");
        assert_eq!(
            crate::findings::classifier_hits(&apple),
            Vec::<knowledge::candidates::ClassifierHit>::new()
        );
    }

    /// #144: a species with rows keeps only its mapped classes, `healthy` included, so a class
    /// a later head adds before the knowledge base has it (tomato mosaic virus) names nothing.
    #[test]
    fn a_species_with_rows_keeps_only_its_mapped_classes() {
        let tomato = kept(
            "tomato",
            &serde_json::json!([
                {"problem_id": "tomato_mosaic_virus", "confidence": 0.6},
                {"problem_id": "tomato_late_blight", "confidence": 0.3},
                {"problem_id": "healthy", "confidence": 0.1}
            ]),
        );
        let labels: Vec<_> = tomato
            .iter()
            .map(|o| (o.label.as_str(), o.knowledge_ref.clone()))
            .collect();
        assert_eq!(
            labels,
            [
                (
                    "tomato_late_blight",
                    Some(KnowledgeRef::Disease("tomato_late_blight".parse().unwrap()))
                ),
                ("Healthy", None),
            ]
        );
        let hits = crate::findings::classifier_hits(&tomato);
        let problems: Vec<&str> = hits.iter().map(|h| h.problem.as_str()).collect();
        assert_eq!(problems, ["tomato_late_blight"]);
    }

    /// No species, no rows: a result the worker couldn't place keeps nothing.
    #[test]
    fn without_a_species_no_class_is_kept() {
        let none = MappedClasses::new(None, &labels());
        assert!(!none.keeps(&ProblemOrHealthy::Healthy));
        assert!(!MappedClasses::default().keeps(&ProblemOrHealthy::Healthy));
    }

    /// The worker's rule: the run's working species, else `BioCLIP`'s first candidate, from
    /// the result or, for a partial disease result, from the run's stored species observations.
    #[test]
    fn the_disease_species_follows_the_workers_rule() {
        let tomato: SpeciesId = "tomato".parse().unwrap();
        let grape: SpeciesId = "grape".parse().unwrap();
        let full = result();
        assert_eq!(
            disease_species(Some(&grape), &full, &[], Run(1)),
            Some(grape.clone())
        );
        assert_eq!(
            disease_species(None, &full, &[], Run(1)),
            Some(tomato.clone())
        );

        let mut weed_first = result();
        if let Some(s) = weed_first.species.as_mut() {
            s.candidates.reverse();
        }
        assert_eq!(disease_species(None, &weed_first, &[], Run(1)), None);

        // A partial disease result: the species partial of the same run is stored.
        let stored: Vec<Observation<Stored>> = writes("partial", 0)
            .into_iter()
            .flat_map(|w| w.observations)
            .collect();
        let partial = PerceptionResult {
            species: None,
            ..result()
        };
        assert_eq!(
            disease_species(None, &partial, &stored, Run(1)),
            Some(tomato)
        );
        assert_eq!(disease_species(None, &partial, &stored, Run(2)), None);
    }

    fn writes(model_version: &str, first: u32) -> Vec<ObservationWrite> {
        let image: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let mut n = first;
        perception_writes(
            &ctx(model_version),
            &result(),
            &[image],
            &names().lookups(),
            || {
                n += 1;
                format!("01JAX3K9T6R2B8V0QZ4M7D1E{n:02}").parse().unwrap()
            },
        )
    }

    #[test]
    fn findings_are_named_from_the_knowledge_base() {
        let all: Vec<_> = writes("v1", 0)
            .into_iter()
            .flat_map(|w| w.observations)
            .collect();
        let labels: Vec<&str> = all
            .iter()
            .filter(|o| {
                matches!(
                    o.stage,
                    ObservationStage::Species | ObservationStage::Disease
                )
            })
            .map(|o| o.label.as_str())
            .collect();
        // A species the knowledge base doesn't hold keeps the worker's label.
        assert_eq!(labels, ["Tomato", "a weed", "Late blight"]);
    }

    /// A fallback perception result's disease calls are the hosted LLM's, tagged `llm_fallback`
    /// so they add no `classifier_weight` (PR #212's review); the disease head's stay `classifier`.
    #[test]
    fn a_fallback_disease_call_is_tagged_llm_fallback() {
        let image: ImageId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let source_of = |vlm_source| {
            let mut c = ctx("v1");
            c.vlm_source = vlm_source;
            let mut n = 0;
            perception_writes(&c, &result(), &[image], &names().lookups(), || {
                n += 1;
                format!("01JAX3K9T6R2B8V0QZ4M7D1E{n:02}").parse().unwrap()
            })
            .into_iter()
            .flat_map(|w| w.observations)
            .find(|o| o.stage == ObservationStage::Disease)
            .unwrap()
            .source
        };
        assert_eq!(
            source_of(ObservationSource::Vlm),
            ObservationSource::Classifier
        );
        assert_eq!(
            source_of(ObservationSource::LlmFallback),
            ObservationSource::LlmFallback
        );
    }

    #[test]
    fn complete_keeps_the_ids_of_unchanged_partial_findings() {
        let stored: Vec<Observation<Stored>> = writes("partial", 0)
            .into_iter()
            .flat_map(|w| w.observations)
            .collect();
        let mut complete = writes("v1", 50);
        // The region gained its mask in `complete`: it changed, so it is replaced.
        attach_masks(
            &mut complete,
            vec![NewMask {
                id: "01JAX3K9T6R2B8V0QZ4M7D1E99".parse().unwrap(),
                image: "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap(),
                region_id: "01JAX3K9T6R2B8V0QZ4M7D1E5A.1".parse().unwrap(),
                storage_key: domain::ports::ObjectKey::mask(
                    "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap(),
                    "01JAX3K9T6R2B8V0QZ4M7D1E50".parse().unwrap(),
                    "01JAX3K9T6R2B8V0QZ4M7D1E99".parse().unwrap(),
                ),
                rle: domain::jobs::RleMask {
                    size: [10, 10],
                    counts: "0".into(),
                },
            }],
        );
        keep_partial_ids(&mut complete, &stored);
        let id_of = |stage: Stage| -> Vec<ObservationId> {
            complete
                .iter()
                .filter(|w| w.stage == stage)
                .flat_map(|w| w.observations.iter().map(|o| o.id))
                .collect()
        };
        let stored_of = |stage: Stage| -> Vec<ObservationId> {
            stored
                .iter()
                .filter(|o| o.pipeline_stage == stage)
                .map(|o| o.id)
                .collect()
        };
        for stage in [Stage::PlantCheck, Stage::Species, Stage::Disease] {
            assert_eq!(id_of(stage), stored_of(stage), "{stage:?} keeps its ids");
        }
        assert!(
            id_of(Stage::Segmentation)
                .iter()
                .all(|id| !stored_of(Stage::Segmentation).contains(id)),
            "the masked region is new"
        );
        // Nothing is matched twice, and a finding on a photo the write doesn't cover is never
        // taken: its record isn't replaced.
        let mut other = writes("v1", 70);
        for w in &mut other {
            w.images = vec!["01JAX3K9T6R2B8V0QZ4M7D1E5B".parse().unwrap()];
            w.replace_imageless = false;
        }
        keep_partial_ids(&mut other, &stored);
        let kept = other
            .iter()
            .flat_map(|w| &w.observations)
            .filter(|o| stored.iter().any(|s| s.id == o.id))
            .count();
        assert_eq!(kept, 0);
    }

    #[test]
    fn species_suggestion_prefers_agreement() {
        let tomato: domain::id::SpeciesId = "tomato".parse().unwrap();
        let pepper: domain::id::SpeciesId = "pepper".parse().unwrap();
        let c = |v| Confidence::new(v).unwrap();
        let bioclip = vec![
            (Some(pepper.clone()), "Pepper".into(), c(0.5)),
            (Some(tomato.clone()), "Tomato".into(), c(0.4)),
        ];
        let (cands, working) = species_suggestion(
            &bioclip,
            Some((Some(tomato.clone()), "Tomato".into(), c(0.85))),
        );
        assert_eq!(working, Some(tomato));
        assert_eq!(cands.len(), 2);
        let (cands, working) = species_suggestion(&bioclip, Some((None, "Basil".into(), c(0.65))));
        assert_eq!(working, Some(pepper));
        assert_eq!(cands.len(), 3);
    }
}
