//! Scan events ([contracts 4]): the shared envelope, the event catalogue as one internally
//! tagged enum, the pipeline stages and problem kinds.
//!
//! A stored event is `ScanEvent<Stored>`: image and mask IDs only, and no
//! `SpeciesCandidate.reference_image`. The API signs it with [`Sign`] each time it sends or
//! replays it, so a replay hours later carries fresh 15-minute URLs.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::{ObservationId, PlanId, ProblemId, QuestionId, ScanId, SpeciesId},
    records::{diagnosis::Diagnosis, observation::Observation, weather::WeatherSummary},
    scan::{GuidedCheck, Question, ScanFailure, ScanStatus, SpeciesCandidate},
    url::{Sign, Signed, Signer, Stored, UrlState},
    wire::{EventSeq, Run, Timestamp, V1},
};

/// `Stage` ([contracts 4]): every pipeline stage, in pipeline order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Perception: is a plant present?
    PlantCheck,
    /// Perception: leaf (and plant, soil, surroundings) regions, with masks when available.
    Segmentation,
    /// Perception: `BioCLIP` species identification.
    Species,
    /// Perception: disease classification per leaf region.
    Disease,
    /// Perception: image vectors for similar cases.
    ImageEmbedding,
    /// `api`: `LiDAR` measurement, only when a photo has depth.
    Measurement,
    /// `api`: weather and soil context.
    Context,
    /// `api`: the pre-VLM candidate set and its indicators.
    Retrieve,
    /// VLM: findings call.
    VlmFindings,
    /// VLM: indicators call.
    VlmIndicators,
    /// VLM: a guided soil check.
    Guided,
    /// VLM: a `verify` job.
    Verify,
    /// `api`: deterministic candidate scoring.
    Scoring,
    /// `api`: fusion risk rules.
    Fusion,
    /// `api` → agent core: the plan writer.
    Plan,
}

impl Stage {
    /// Every stage, in pipeline order.
    pub const ALL: [Self; 15] = [
        Self::PlantCheck,
        Self::Segmentation,
        Self::Species,
        Self::Disease,
        Self::ImageEmbedding,
        Self::Measurement,
        Self::Context,
        Self::Retrieve,
        Self::VlmFindings,
        Self::VlmIndicators,
        Self::Guided,
        Self::Verify,
        Self::Scoring,
        Self::Fusion,
        Self::Plan,
    ];
}

/// `ProblemKind` ([contracts 4]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// Fungal disease.
    Fungal,
    /// Bacterial disease.
    Bacterial,
    /// Viral disease.
    Viral,
    /// Insect and other pests.
    Pest,
    /// Larger animals.
    Wildlife,
    /// Non-living causes: nutrients, water, sun, cold.
    Abiotic,
}

impl ProblemKind {
    /// Kinds that spread to neighbours ([contracts 7], neighbours for the plan writer).
    pub const fn spreads(self) -> bool {
        matches!(
            self,
            Self::Fungal | Self::Bacterial | Self::Viral | Self::Pest
        )
    }
}

/// `run_started.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunReason {
    /// Run 1.
    Submit,
    /// A retryable failure was resubmitted.
    Resubmit,
    /// The user picked a different species.
    SpeciesChanged,
}

/// `stage_completed.skipped` and the worker's `stage_completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Skipped {
    /// Such as `"species_not_supported"`, `"nothing_measurable"`, `"error"`, `"all_measured"`.
    pub reason: String,
}

impl Skipped {
    /// `measurement` with depth but nothing measurable ([contracts 4]).
    pub const NOTHING_MEASURABLE: &'static str = "nothing_measurable";
    /// `measurement` failed; the run continues without measurements.
    pub const ERROR: &'static str = "error";
    /// `vlm_indicators` when measurements answered every indicator.
    pub const ALL_MEASURED: &'static str = "all_measured";
}

/// `candidates_selected.candidates[].source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    /// The disease classifier's top 3.
    Classifier,
    /// `susceptible_to` the working species.
    Species,
    /// A look-alike of a top-ranked candidate.
    Lookalike,
    /// An organism the VLM found, added by a `verify` job.
    Organism,
    /// A damage pattern the VLM found, added by a `verify` job.
    Damage,
}

/// `candidates_selected.candidates[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SelectedCandidate {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
    /// Why it is in the set.
    pub source: SelectionSource,
}

/// `question_resolved.by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedBy {
    /// The user answered.
    User,
    /// A photo answered it.
    Photo,
    /// A new run, or a failed scan, superseded it.
    Superseded,
    /// The photo sent for it couldn't be analysed; the plan stays.
    Dropped,
}

/// `plan_superseded.reason` ([contracts 7], re-planning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupersedeReason {
    /// An answer or photo after the scheduler continued without it.
    LateAnswer,
    /// A species-change run reached `planned` again.
    SpeciesChanged,
    /// A confirmed `revise_plan` card.
    ChatRevision,
    /// A guided check changed the texture.
    GuidedCheck,
}

/// The event catalogue ([contracts 4]), tagged by `type` and flattened into [`ScanEvent`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", bound = "")]
#[schema(as = ScanEventBody)]
pub enum ScanEventBody<U: UrlState = Signed> {
    /// Reset the listed stages; clear their findings.
    RunStarted {
        /// Why the run started.
        reason: RunReason,
        /// The stages the run repeats.
        rerun_stages: Vec<Stage>,
    },
    /// Update the status chip.
    StatusChanged {
        /// The old status.
        from: ScanStatus,
        /// The new status.
        to: ScanStatus,
    },
    /// Show the stage as in progress.
    StageStarted {
        /// The stage.
        stage: Stage,
    },
    /// Tick the stage.
    StageCompleted {
        /// The stage.
        stage: Stage,
        /// How long it took.
        duration_ms: u64,
        /// Why it was skipped, if it was.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        skipped: Option<Skipped>,
    },
    /// Add the finding; draw its region and mask.
    Finding {
        /// The finding.
        #[schema(value_type = Observation)]
        observation: Observation<U>,
    },
    /// Remove a finding the job's final result didn't keep.
    FindingRetracted {
        /// The retracted observation.
        observation_id: ObservationId,
    },
    /// Show the weather strip.
    WeatherLoaded {
        /// The run's weather.
        weather: WeatherSummary,
    },
    /// "Checking 9 possible problems".
    CandidatesSelected {
        /// The candidate set.
        candidates: Vec<SelectedCandidate>,
    },
    /// Show the confirmation card when `needs_confirmation`.
    SpeciesSuggested {
        /// `BioCLIP` and VLM candidates.
        #[schema(value_type = Vec<SpeciesCandidate>)]
        candidates: Vec<SpeciesCandidate<U>>,
        /// The species the run continues with.
        #[schema(required = true)]
        working_species_id: Option<SpeciesId>,
        /// Whether to ask the user.
        needs_confirmation: bool,
    },
    /// Hide the card.
    SpeciesConfirmed {
        /// `null` for "something else".
        #[schema(required = true)]
        species_id: Option<SpeciesId>,
        /// "Something else".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        custom_name: Option<String>,
        /// A new run follows (announced by `run_started`).
        rerun: bool,
    },
    /// Show the question card.
    Question {
        /// The question.
        question: Question,
    },
    /// Hide the card.
    QuestionResolved {
        /// The question.
        question_id: QuestionId,
        /// How it resolved.
        by: ResolvedBy,
    },
    /// Show the texture result, or the failure.
    GuidedCheckDone {
        /// The check.
        #[schema(value_type = GuidedCheck)]
        check: GuidedCheck<U>,
    },
    /// Show the result.
    Diagnosis {
        /// The diagnosis.
        #[schema(value_type = Diagnosis)]
        diagnosis: Diagnosis<U>,
    },
    /// Fetch `GET /scans/{id}` and show the plan.
    PlanReady {
        /// The plan.
        plan_id: PlanId,
        /// Written from an `uncertain` diagnosis.
        provisional: bool,
    },
    /// Replace the plan, show a notice.
    PlanSuperseded {
        /// The old plan.
        old_plan_id: PlanId,
        /// The new plan.
        new_plan_id: PlanId,
        /// Why.
        reason: SupersedeReason,
    },
    /// Show the error; offer "retry" when retryable, otherwise "new scan".
    Failed {
        /// The failure.
        failure: ScanFailure,
    },
}

/// `ScanEventType`: the `type` values, for the `scan_event.type` column and the SSE `event:`
/// line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanEventType {
    /// `run_started`
    RunStarted,
    /// `status_changed`
    StatusChanged,
    /// `stage_started`
    StageStarted,
    /// `stage_completed`
    StageCompleted,
    /// `finding`
    Finding,
    /// `finding_retracted`
    FindingRetracted,
    /// `weather_loaded`
    WeatherLoaded,
    /// `candidates_selected`
    CandidatesSelected,
    /// `species_suggested`
    SpeciesSuggested,
    /// `species_confirmed`
    SpeciesConfirmed,
    /// `question`
    Question,
    /// `question_resolved`
    QuestionResolved,
    /// `guided_check_done`
    GuidedCheckDone,
    /// `diagnosis`
    Diagnosis,
    /// `plan_ready`
    PlanReady,
    /// `plan_superseded`
    PlanSuperseded,
    /// `failed`
    Failed,
}

impl ScanEventType {
    /// The wire name: the SSE `event:` line and the stored `type`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "run_started",
            Self::StatusChanged => "status_changed",
            Self::StageStarted => "stage_started",
            Self::StageCompleted => "stage_completed",
            Self::Finding => "finding",
            Self::FindingRetracted => "finding_retracted",
            Self::WeatherLoaded => "weather_loaded",
            Self::CandidatesSelected => "candidates_selected",
            Self::SpeciesSuggested => "species_suggested",
            Self::SpeciesConfirmed => "species_confirmed",
            Self::Question => "question",
            Self::QuestionResolved => "question_resolved",
            Self::GuidedCheckDone => "guided_check_done",
            Self::Diagnosis => "diagnosis",
            Self::PlanReady => "plan_ready",
            Self::PlanSuperseded => "plan_superseded",
            Self::Failed => "failed",
        }
    }
}

impl<U: UrlState> ScanEventBody<U> {
    /// The event's `type`.
    pub fn event_type(&self) -> ScanEventType {
        match self {
            Self::RunStarted { .. } => ScanEventType::RunStarted,
            Self::StatusChanged { .. } => ScanEventType::StatusChanged,
            Self::StageStarted { .. } => ScanEventType::StageStarted,
            Self::StageCompleted { .. } => ScanEventType::StageCompleted,
            Self::Finding { .. } => ScanEventType::Finding,
            Self::FindingRetracted { .. } => ScanEventType::FindingRetracted,
            Self::WeatherLoaded { .. } => ScanEventType::WeatherLoaded,
            Self::CandidatesSelected { .. } => ScanEventType::CandidatesSelected,
            Self::SpeciesSuggested { .. } => ScanEventType::SpeciesSuggested,
            Self::SpeciesConfirmed { .. } => ScanEventType::SpeciesConfirmed,
            Self::Question { .. } => ScanEventType::Question,
            Self::QuestionResolved { .. } => ScanEventType::QuestionResolved,
            Self::GuidedCheckDone { .. } => ScanEventType::GuidedCheckDone,
            Self::Diagnosis { .. } => ScanEventType::Diagnosis,
            Self::PlanReady { .. } => ScanEventType::PlanReady,
            Self::PlanSuperseded { .. } => ScanEventType::PlanSuperseded,
            Self::Failed { .. } => ScanEventType::Failed,
        }
    }
}

impl Sign for ScanEventBody<Stored> {
    type Signed = ScanEventBody<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> ScanEventBody<Signed> {
        match self {
            Self::RunStarted {
                reason,
                rerun_stages,
            } => ScanEventBody::RunStarted {
                reason,
                rerun_stages,
            },
            Self::StatusChanged { from, to } => ScanEventBody::StatusChanged { from, to },
            Self::StageStarted { stage } => ScanEventBody::StageStarted { stage },
            Self::StageCompleted {
                stage,
                duration_ms,
                skipped,
            } => ScanEventBody::StageCompleted {
                stage,
                duration_ms,
                skipped,
            },
            Self::Finding { observation } => ScanEventBody::Finding {
                observation: observation.sign((), signer),
            },
            Self::FindingRetracted { observation_id } => {
                ScanEventBody::FindingRetracted { observation_id }
            }
            Self::WeatherLoaded { weather } => ScanEventBody::WeatherLoaded { weather },
            Self::CandidatesSelected { candidates } => {
                ScanEventBody::CandidatesSelected { candidates }
            }
            Self::SpeciesSuggested {
                candidates,
                working_species_id,
                needs_confirmation,
            } => ScanEventBody::SpeciesSuggested {
                candidates: candidates.sign((), signer),
                working_species_id,
                needs_confirmation,
            },
            Self::SpeciesConfirmed {
                species_id,
                custom_name,
                rerun,
            } => ScanEventBody::SpeciesConfirmed {
                species_id,
                custom_name,
                rerun,
            },
            Self::Question { question } => ScanEventBody::Question { question },
            Self::QuestionResolved { question_id, by } => {
                ScanEventBody::QuestionResolved { question_id, by }
            }
            Self::GuidedCheckDone { check } => ScanEventBody::GuidedCheckDone {
                check: check.sign((), signer),
            },
            Self::Diagnosis { diagnosis } => ScanEventBody::Diagnosis {
                diagnosis: diagnosis.sign((), signer),
            },
            Self::PlanReady {
                plan_id,
                provisional,
            } => ScanEventBody::PlanReady {
                plan_id,
                provisional,
            },
            Self::PlanSuperseded {
                old_plan_id,
                new_plan_id,
                reason,
            } => ScanEventBody::PlanSuperseded {
                old_plan_id,
                new_plan_id,
                reason,
            },
            Self::Failed { failure } => ScanEventBody::Failed { failure },
        }
    }
}

/// `ScanEventBase` plus its body ([contracts 4], envelope): what the SSE `data:` line carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ScanEvent)]
pub struct ScanEvent<U: UrlState = Signed> {
    /// `1`.
    pub v: V1,
    /// The SSE `id`.
    pub seq: EventSeq,
    /// The run the event belongs to.
    pub run: Run,
    /// The scan.
    pub scan_id: ScanId,
    /// When it was stored.
    pub ts: Timestamp,
    /// `type` and the extra fields.
    #[serde(flatten)]
    #[schema(value_type = ScanEventBody)]
    pub body: ScanEventBody<U>,
}

impl<U: UrlState> ScanEvent<U> {
    /// The SSE `event:` name.
    pub fn event_name(&self) -> &'static str {
        self.body.event_type().as_str()
    }
}

impl Sign for ScanEvent<Stored> {
    type Signed = ScanEvent<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> ScanEvent<Signed> {
        ScanEvent {
            v: V1,
            seq: self.seq,
            run: self.run,
            scan_id: self.scan_id,
            ts: self.ts,
            body: self.body.sign((), signer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        records::observation::{KnowledgeRef, ObservationSource, ObservationStage},
        testing::{FakeSigner, FixedPhotos},
        url::StoredMask,
        wire::{BBox, Confidence, Rated, Region},
    };

    const SCAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5F";

    fn stored_finding() -> ScanEvent<Stored> {
        ScanEvent {
            v: V1,
            seq: EventSeq(17),
            run: Run(1),
            scan_id: SCAN.parse().unwrap(),
            ts: "2026-10-18T14:03:22.412Z".parse().unwrap(),
            body: ScanEventBody::Finding {
                observation: Observation {
                    id: "01JAX5OBS0000000000000000A"
                        .replace('O', "0")
                        .parse()
                        .unwrap(),
                    scan_id: SCAN.parse().unwrap(),
                    run: Run(1),
                    stage: ObservationStage::Disease,
                    pipeline_stage: Stage::Disease,
                    label: "Early blight".to_owned(),
                    knowledge_ref: Some(KnowledgeRef::Disease(
                        "tomato_early_blight".parse().unwrap(),
                    )),
                    role: None,
                    detail: None,
                    confidence: Rated(Confidence::new(0.71).unwrap()),
                    region: Some(Region {
                        region_id: Some("01JAX6BBBBBBBBBBBBBBBBBBBB.2".parse().unwrap()),
                        image_id: "01JAX6BBBBBBBBBBBBBBBBBBBB".parse().unwrap(),
                        bbox: BBox::new(0.1, 0.2, 0.3, 0.4).unwrap(),
                        mask: StoredMask {
                            mask_id: Some("01JAX7MMMMMMMMMMMMMMMMMMMM".parse().unwrap()),
                        },
                    }),
                    source: ObservationSource::Classifier,
                    model_version: "plantdoc-v3".to_owned(),
                    created_at: "2026-10-18T14:03:22Z".parse().unwrap(),
                },
            },
        }
    }

    #[test]
    fn stored_finding_keeps_ids_and_no_url() {
        let json = serde_json::to_value(stored_finding()).unwrap();
        let region = &json["observation"]["region"];
        assert_eq!(region["mask_id"], "01JAX7MMMMMMMMMMMMMMMMMMMM");
        assert!(region.get("mask_url").is_none());
        let back: ScanEvent<Stored> = serde_json::from_value(json).unwrap();
        assert_eq!(back.body, stored_finding().body);
    }

    #[test]
    fn finding_envelope_is_signed_when_sent() {
        let signer = FakeSigner;
        let photos = FixedPhotos::default();
        let now: crate::wire::Timestamp = "2026-10-18T14:03:22Z".parse().unwrap();
        let sent = stored_finding().sign((), &Signer::new(&signer, &photos, now));
        assert_eq!(sent.event_name(), "finding");
        let text = serde_json::to_string(&sent).unwrap();
        // The envelope of contracts 4, in order: v, seq, run, scan_id, ts, type, then the body.
        assert!(text.starts_with(
            r#"{"v":1,"seq":17,"run":1,"scan_id":"01JAX3K9T6R2B8V0QZ4M7D1E5F","ts":"2026-10-18T14:03:22Z","type":"finding","observation":{"#
        ));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let obs = &json["observation"];
        assert_eq!(obs["confidence"], 0.71);
        assert_eq!(obs["confidence_band"], "medium");
        assert_eq!(
            obs["ref"],
            serde_json::json!({"table": "disease", "id": "tomato_early_blight"})
        );
        let mask_url = obs["region"]["mask_url"].as_str().unwrap();
        let exp = now.unix_seconds() + 900;
        assert!(mask_url.starts_with(&format!(
            "https://api.test/scans/01JAX3K9T6R2B8V0QZ4M7D1E5F/masks/01JAX7MMMMMMMMMMMMMMMMMMMM?exp={exp}&sig="
        )));
        assert!(obs["region"].get("mask_id").is_none());
        let back: ScanEvent = serde_json::from_str(&text).unwrap();
        assert_eq!(back.body, sent.body);
        assert_eq!(back.ts.to_string(), "2026-10-18T14:03:22Z");
    }

    #[test]
    fn every_event_type_matches_its_tag() {
        let body: ScanEventBody<Stored> = ScanEventBody::PlanSuperseded {
            old_plan_id: SCAN.parse().unwrap(),
            new_plan_id: SCAN.parse().unwrap(),
            reason: SupersedeReason::LateAnswer,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["type"], body.event_type().as_str());
    }

    #[test]
    fn signed_events_have_a_schema() {
        fn keys(v: &serde_json::Value, out: &mut Vec<String>) {
            if let Some(props) = v["properties"].as_object() {
                out.extend(props.keys().cloned());
            }
            for part in v["allOf"].as_array().into_iter().flatten() {
                keys(part, out);
            }
        }
        let schema =
            serde_json::to_value(<ScanEventBody as utoipa::PartialSchema>::schema()).unwrap();
        let text = schema.to_string();
        assert!(text.contains("finding_retracted"));
        let region = serde_json::to_value(<Region as utoipa::PartialSchema>::schema()).unwrap();
        let mut props = Vec::new();
        keys(&region, &mut props);
        let props: Vec<&str> = props.iter().map(String::as_str).collect();
        assert!(props.contains(&"mask_url"));
        assert!(!props.contains(&"mask_id"));
        let observation =
            serde_json::to_value(<Observation as utoipa::PartialSchema>::schema()).unwrap();
        let mut obs_props = Vec::new();
        keys(&observation, &mut obs_props);
        assert!(obs_props.iter().any(|k| k == "ref"));
        assert!(observation.to_string().contains("confidence_band"));
        let indicator =
            serde_json::to_value(<crate::jobs::Indicator as utoipa::PartialSchema>::schema())
                .unwrap();
        assert!(indicator["properties"].get("where").is_some());
    }
}
