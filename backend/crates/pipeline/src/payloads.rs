//! Job payloads ([contracts 5], payloads and photo routing): the `perception`, `vlm` and
//! `verify` jobs the steps queue. Pure: the steps pass in what they read.
//!
//! **Lifetimes.** `'s` borrows the scan's images from the step's snapshot while routing picks
//! them; every payload is owned, because it is stored on the job row.

use domain::{
    id::{ImageId, JobId, QuestionId, SpeciesId},
    jobs::{
        CandidatePrompt, Indicator, JobImage, PerceptionPayload, PerceptionStage, PriorRegion,
        VerifyPayload, VerifyReason, VlmPayload, WorkerImagePath, WorkingSpecies,
    },
    ports::VlmOptions,
    rules::{
        limits::{MAX_CANDIDATES, MAX_VLM_IMAGES},
        routing::{RoutableImage, findings_images, indicator_images},
    },
    scan::ScanImage,
    url::Stored,
};
use knowledge::{candidates::Candidate, scoring::AnswerSet};

/// A stored image as a job image: its URL is the worker route
/// `/internal/jobs/{job_id}/images/{image_id}`.
pub fn job_image(job: JobId, image: &ScanImage<Stored>) -> JobImage {
    JobImage {
        image_id: image.id,
        kind: image.kind,
        width: image.width,
        height: image.height,
        url: WorkerImagePath::new(job, image.id),
    }
}

/// The run's plant and soil shots, in upload order, numbered within their kind for the
/// routing table. Question photos and guided checks are left out.
pub fn run_images(images: &[ScanImage<Stored>]) -> Vec<RoutableImage<&ScanImage<Stored>>> {
    let mut out: Vec<RoutableImage<&ScanImage<Stored>>> = Vec::new();
    for img in images
        .iter()
        .filter(|i| i.kind.is_plant_or_soil() && i.answers_question_id.is_none())
    {
        let nth = out.iter().filter(|r| r.kind == img.kind).count();
        out.push(RoutableImage {
            kind: img.kind,
            nth_of_kind: u32::try_from(nth).unwrap_or(u32::MAX),
            item: img,
        });
    }
    out
}

/// The indicators a job asks: each candidate's 3 discriminating ones, minus those the user or
/// a measurement settled ([contracts 7]); a candidate with none left is not sent; at most
/// [`MAX_CANDIDATES`].
pub fn prompts(candidates: &[Candidate<'_>], answers: &AnswerSet<'_>) -> Vec<CandidatePrompt> {
    candidates
        .iter()
        .filter_map(|c| {
            let indicators: Vec<Indicator> = c
                .indicators()
                .filter(|i| !answers.settled(&i.id))
                .map(|i| Indicator {
                    indicator_id: i.id.clone(),
                    question: i.question.clone(),
                    r#where: i.r#where,
                    image_kinds: i.image_kinds.clone(),
                })
                .collect();
            (!indicators.is_empty()).then(|| CandidatePrompt {
                problem_id: c.profile.id.clone(),
                name: c.profile.name.clone(),
                indicators,
            })
        })
        .take(MAX_CANDIDATES)
        .collect()
}

/// The `vlm` job ([contracts 5]): findings images in the fixed order, indicator images chosen
/// by the questions. With no candidate left (measurements answered everything) `candidates`
/// and `indicator_images` are empty and the worker skips the indicators call.
pub fn vlm_payload(
    job: JobId,
    images: &[ScanImage<Stored>],
    candidates: Vec<CandidatePrompt>,
    working_species: Option<WorkingSpecies>,
    options: &VlmOptions,
) -> VlmPayload {
    let routable = run_images(images);
    let findings = findings_images(&routable)
        .into_iter()
        .map(|i| job_image(job, i))
        .collect();
    let indicators: Vec<&Indicator> = candidates.iter().flat_map(|c| &c.indicators).collect();
    let indicator_imgs = if indicators.is_empty() {
        Vec::new()
    } else {
        indicator_images(&routable, &indicators, None)
            .into_iter()
            .map(|i| job_image(job, i))
            .collect()
    };
    VlmPayload {
        findings_images: findings,
        indicator_images: indicator_imgs,
        working_species,
        species_options: options.species.clone(),
        organism_options: options.organisms.clone(),
        candidates,
    }
}

/// A `verify` job ([contracts 5], verify routing): the answering photo first, then the
/// indicator-call rule for the rest; at most 4 images.
pub fn verify_payload(
    job: JobId,
    reason: VerifyReason,
    question: Option<QuestionId>,
    images: &[ScanImage<Stored>],
    new_photo: Option<ImageId>,
    candidates: Vec<CandidatePrompt>,
) -> VerifyPayload {
    let routable = run_images(images);
    let first = new_photo
        .and_then(|id| images.iter().find(|i| i.id == id))
        .map(|img| RoutableImage {
            kind: img.kind,
            nth_of_kind: 0,
            item: img,
        });
    let indicators: Vec<&Indicator> = candidates.iter().flat_map(|c| &c.indicators).collect();
    let mut chosen: Vec<JobImage> = indicator_images(&routable, &indicators, first.as_ref())
        .into_iter()
        .map(|i| job_image(job, i))
        .collect();
    chosen.truncate(MAX_VLM_IMAGES);
    VerifyPayload {
        reason,
        question_id: question,
        images: chosen,
        candidates,
    }
}

/// What a run's `perception` job needs besides the images.
#[derive(Debug, Clone, PartialEq)]
pub struct PerceptionSpec {
    /// The stages to run.
    pub stages: Vec<PerceptionStage>,
    /// A confirmed or user-chosen species: skip identification.
    pub species_id: Option<SpeciesId>,
    /// "Something else" was chosen.
    pub skip_species_identification: bool,
    /// Regions of an earlier run, when segmentation isn't re-run.
    pub regions: Option<Vec<PriorRegion>>,
    /// `label_set_version`.
    pub label_set_version: String,
    /// `classifier_labels_version`.
    pub classifier_labels_version: String,
}

/// A run's `perception` job: every plant and soil image, never guided checks or depth.
pub fn perception_payload(
    job: JobId,
    images: &[ScanImage<Stored>],
    spec: PerceptionSpec,
) -> PerceptionPayload {
    PerceptionPayload {
        images: images
            .iter()
            .filter(|i| i.kind.is_plant_or_soil() && i.answers_question_id.is_none())
            .map(|i| job_image(job, i))
            .collect(),
        stages: spec.stages,
        species_id: spec.species_id,
        skip_species_identification: spec.skip_species_identification,
        regions: spec.regions,
        label_set_version: spec.label_set_version,
        classifier_labels_version: spec.classifier_labels_version,
        question_id: None,
    }
}

/// A question photo's segmentation-only `perception` job ([contracts 7], photos sent for a
/// question): `stages: ["segmentation"]`, that photo alone, `question_id` set.
pub fn question_segmentation_payload(
    job: JobId,
    photo: &ScanImage<Stored>,
    question: QuestionId,
    label_set_version: String,
    classifier_labels_version: String,
) -> PerceptionPayload {
    PerceptionPayload {
        images: vec![job_image(job, photo)],
        stages: vec![PerceptionStage::Segmentation],
        species_id: None,
        skip_species_identification: false,
        regions: None,
        label_set_version,
        classifier_labels_version,
        question_id: Some(question),
    }
}
