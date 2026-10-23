//! Turning a worker's result into what the store keeps ([contracts 5], results and partial
//! results; [contracts 7], observation): observations per pipeline stage, and the VLM's
//! indicator answers as evidence. Pure, except for the lookups passed in.

use std::collections::BTreeMap;

use domain::{
    events::Stage,
    id::{ImageId, ScanId},
    id::{IndicatorId, ObservationId, ProblemOrHealthy},
    jobs::{PerceptionRegion, PerceptionResult, SpeciesResult},
    knowledge::{IndicatorSeed, OrganismSeed},
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
    /// `vlm` or `llm_fallback` for VLM findings, by the worker's `model_versions`.
    pub vlm_source: ObservationSource,
    /// The model version recorded on each observation.
    pub model_version: &'a str,
    /// Now.
    pub now: Timestamp,
}

/// Lookups the conversion needs, borrowed for the call: problem names, and organism names and
/// roles.
#[derive(Debug, Clone, Copy)]
pub struct Lookups<'a> {
    /// Problem names, for classifier labels.
    pub problem_names: &'a BTreeMap<String, String>,
    /// Organism profiles, for names and roles.
    pub organisms: &'a [OrganismSeed],
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
/// records with [`attach_masks`]; a partial result stores none.
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
            species_obs(ctx, species, &mut new_id),
        ));
    }
    if let Some(disease) = &result.disease {
        let mut list = Vec::new();
        for d in disease {
            for top in &d.top {
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
                    ObservationSource::Classifier,
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
    new_id: &mut impl FnMut() -> ObservationId,
) -> Vec<Observation<Stored>> {
    species
        .candidates
        .iter()
        .map(|c| {
            let mut o = obs(
                ctx,
                new_id(),
                ObservationStage::Species,
                Stage::Species,
                ObservationSource::Bioclip,
                c.label.clone(),
                c.score,
            );
            o.knowledge_ref = c.species_id.clone().map(KnowledgeRef::Species);
            o
        })
        .collect()
}

/// The VLM findings' observations (`pipeline_stage: vlm_findings`): the species guess, soil,
/// organisms (role from the knowledge base, `unknown` for "other") and damage.
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
        g.other_name
            .clone()
            .or_else(|| g.species_id.as_ref().map(ToString::to_string))
            .unwrap_or_default(),
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
