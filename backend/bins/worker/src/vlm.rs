//! The VLM adapter ([contracts 6]): `vlm`, `verify` and `guided` jobs on one backend. Generic
//! over the backend (`VlmAdapter<OpenAiBackend>` on the rig, `VlmAdapter<AnthropicBackend>` on
//! the fallback worker), wired as `Arc<dyn VisionModel>`.

use std::{collections::BTreeMap, sync::Arc, time::Instant};

use async_trait::async_trait;
use domain::{
    events::{Skipped, Stage},
    jobs::{
        CandidatePrompt, GuidedPayload, GuidedResult, Indicator, PartialResult, PerceptionPayload,
        PerceptionResult, VerifyPayload, VerifyReason, VerifyResult, VlmPayload, VlmResult,
        WorkerEvent,
    },
    ports::{JobContext, LoadedImage, ModelError, ProgressSink, VisionModel},
    rules::limits::MAX_VLM_IMAGES,
    vlm::{VlmFindings, VlmIndicatorAnswers},
};
use serde_json::Value;
use tokio::sync::Semaphore;

pub mod backend;
pub mod decode;
pub mod prepare;
pub mod profile;
pub mod prompt;
pub mod schema;

use backend::{Stop, VlmBackend, VlmCall, VlmImage};
use decode::{CallImages, Expected, Invalid};
use prepare::PreparedImage;
use profile::AdapterProfile;
use schema::{BoxRange, Limits};

/// The adapter. `B` is chosen at compile time per worker kind; nothing here is `dyn` except
/// the `VisionModel` it implements.
pub struct VlmAdapter<B: VlmBackend> {
    backend: B,
    profile: AdapterProfile,
    system: String,
    limits: Limits,
    cpu: Arc<Semaphore>,
}

impl<B: VlmBackend> VlmAdapter<B> {
    /// An adapter on `backend` with the instance's profile.
    pub fn new(backend: B, profile: AdapterProfile, cpu: Arc<Semaphore>) -> Self {
        Self {
            system: prompt::system(profile.box_format),
            backend,
            profile,
            limits: Limits::CONTRACT,
            cpu,
        }
    }

    /// The profile.
    pub const fn profile(&self) -> &AdapterProfile {
        &self.profile
    }

    /// Prepares the job's images (each once, even when both calls use it).
    pub async fn prepare(&self, images: &[LoadedImage]) -> Result<Vec<PreparedImage>, ModelError> {
        prepare::prepare_all(images, &self.profile, &self.cpu)
            .await
            .map_err(|e| e.into_model_error(self.backend.name()))
    }

    /// One structured call, validated, retried once on any validation failure (including
    /// `finish_reason: length` and a refusal), then `InvalidOutput` (`model_error`, retryable).
    pub async fn call_validated<T>(
        &self,
        call: &VlmCall<'_>,
        decode: impl Fn(&str) -> Result<T, Invalid> + Send + Sync,
    ) -> Result<T, ModelError> {
        let mut last = None;
        for attempt in 1..=2u32 {
            let reply = self.backend.call(call).await?;
            let decoded = match reply.stop {
                Stop::Length => Err(Invalid::Truncated),
                Stop::Refusal => Err(Invalid::Refused),
                Stop::Complete => decode(&reply.text),
            };
            match decoded {
                Ok(value) => return Ok(value),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, schema = call.schema_name, "invalid VLM output");
                    last = Some(e);
                }
            }
        }
        Err(ModelError::InvalidOutput {
            model: self.backend.name(),
            source: Box::new(last.unwrap_or(Invalid::Truncated)),
        })
    }

    /// The call's labelled images; `'c` borrows the prepared images.
    fn labelled<'c>(
        images: &[&'c PreparedImage],
        first_is_new: bool,
    ) -> (Vec<String>, CallImages<'c>) {
        let labels = images
            .iter()
            .enumerate()
            .map(|(i, p)| prompt::image_label(i, p.kind, first_is_new && i == 0))
            .collect();
        (labels, CallImages::new(images.to_vec()))
    }

    fn box_range(&self, images: &[&PreparedImage]) -> BoxRange {
        let largest = images
            .iter()
            .map(|p| p.width.max(p.height))
            .max()
            .unwrap_or(1000);
        BoxRange::for_format(self.profile.box_format, largest)
    }

    /// The findings call on the payload's findings images, in the payload's order.
    pub async fn findings(
        &self,
        payload: &VlmPayload,
        prepared: &[PreparedImage],
    ) -> Result<VlmFindings, ModelError> {
        let images = pick(prepared, payload.findings_images.iter().map(|i| i.image_id));
        let (labels, call_images) = Self::labelled(&images, false);
        let schema: Value = schema::findings(
            images.len(),
            &payload.species_options,
            &payload.organism_options,
            self.box_range(&images),
            self.limits,
        );
        let instructions = prompt::findings(
            payload.working_species.as_ref(),
            &payload.species_options,
            &payload.organism_options,
        );
        let call = VlmCall {
            system: &self.system,
            images: vlm_images(&labels, &images),
            instructions: &instructions,
            schema_name: "findings",
            schema: &schema,
            max_output_tokens: self.profile.budget.findings,
        };
        self.call_validated(&call, |text| {
            decode::findings(
                text,
                &call_images,
                &payload.species_options,
                &payload.organism_options,
                self.profile.box_format,
                &self.profile.certainty,
                self.limits,
            )
        })
        .await
    }

    /// The indicators call (also the verify call) on the given images, for every indicator of
    /// every candidate, in payload order.
    pub async fn indicators(
        &self,
        candidates: &[CandidatePrompt],
        images: &[&PreparedImage],
        new_photo: bool,
    ) -> Result<VlmIndicatorAnswers, ModelError> {
        let all: Vec<&Indicator> = candidates.iter().flat_map(|c| &c.indicators).collect();
        let kinds: Vec<_> = images.iter().map(|p| p.kind).collect();
        let questions = schema::questions(all, &kinds);
        let expected: Vec<Expected> = questions
            .iter()
            .map(|q| Expected {
                key: q.key.clone(),
                indicator_id: q.indicator.indicator_id.clone(),
                unsure_only: q.images.is_empty(),
            })
            .collect();
        let (labels, call_images) = Self::labelled(images, new_photo);
        let schema = schema::indicators(images.len(), &questions, self.limits);
        let instructions = prompt::indicators(&questions, new_photo);
        let call = VlmCall {
            system: &self.system,
            images: vlm_images(&labels, images),
            instructions: &instructions,
            schema_name: "indicators",
            schema: &schema,
            max_output_tokens: self.profile.budget.indicators,
        };
        self.call_validated(&call, |text| {
            decode::indicators(text, &call_images, &expected, self.limits)
        })
        .await
    }
}

/// The prepared images with these ids, in that order, at most 4.
pub fn pick(
    prepared: &[PreparedImage],
    ids: impl IntoIterator<Item = domain::id::ImageId>,
) -> Vec<&PreparedImage> {
    ids.into_iter()
        .filter_map(|id| prepared.iter().find(|p| p.image_id == id))
        .take(MAX_VLM_IMAGES)
        .collect()
}

fn vlm_images<'a>(labels: &'a [String], images: &[&'a PreparedImage]) -> Vec<VlmImage<'a>> {
    labels
        .iter()
        .zip(images)
        .map(|(label, p)| VlmImage {
            label,
            jpeg: &p.jpeg,
        })
        .collect()
}

async fn send(progress: &dyn ProgressSink, events: Vec<WorkerEvent>) {
    if let Err(e) = progress.send(events).await {
        tracing::debug!(error = %e, "progress not delivered");
    }
}

fn ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[async_trait]
impl<B: VlmBackend> VisionModel for VlmAdapter<B> {
    fn model_versions(&self) -> BTreeMap<String, String> {
        self.profile.model_versions()
    }

    async fn perceive(
        &self,
        _job: JobContext,
        _payload: &PerceptionPayload,
        _images: &[LoadedImage],
        _progress: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError> {
        Err(ModelError::Unsupported {
            model: self.backend.name(),
            what: "perception jobs",
        })
    }

    /// The findings call, its partial result, then the indicators call unless every indicator
    /// was measured (`all_measured`: no candidates and no indicator images).
    async fn analyse(
        &self,
        _job: JobContext,
        payload: &VlmPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError> {
        let prepared = self.prepare(images).await?;
        send(
            progress.as_ref(),
            vec![WorkerEvent::StageStarted {
                stage: Stage::VlmFindings,
            }],
        )
        .await;
        let started = Instant::now();
        let findings = self.findings(payload, &prepared).await?;
        send(
            progress.as_ref(),
            vec![
                WorkerEvent::PartialResult {
                    stage: Stage::VlmFindings,
                    result: PartialResult::VlmFindings {
                        findings: findings.clone(),
                    },
                },
                WorkerEvent::StageCompleted {
                    stage: Stage::VlmFindings,
                    duration_ms: ms(started),
                    skipped: None,
                },
            ],
        )
        .await;
        let started = Instant::now();
        let indicators = if payload.candidates.iter().all(|c| c.indicators.is_empty()) {
            send(
                progress.as_ref(),
                vec![WorkerEvent::StageCompleted {
                    stage: Stage::VlmIndicators,
                    duration_ms: 0,
                    skipped: Some(Skipped {
                        reason: Skipped::ALL_MEASURED.to_owned(),
                    }),
                }],
            )
            .await;
            VlmIndicatorAnswers::default()
        } else {
            send(
                progress.as_ref(),
                vec![WorkerEvent::StageStarted {
                    stage: Stage::VlmIndicators,
                }],
            )
            .await;
            let chosen = pick(
                &prepared,
                payload.indicator_images.iter().map(|i| i.image_id),
            );
            let answers = self.indicators(&payload.candidates, &chosen, false).await?;
            send(
                progress.as_ref(),
                vec![WorkerEvent::StageCompleted {
                    stage: Stage::VlmIndicators,
                    duration_ms: ms(started),
                    skipped: None,
                }],
            )
            .await;
            answers
        };
        Ok(VlmResult {
            findings,
            indicators,
        })
    }

    /// The indicators call only; for a new photo it comes first and is labelled as such.
    async fn verify(
        &self,
        _job: JobContext,
        payload: &VerifyPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError> {
        let prepared = self.prepare(images).await?;
        send(
            progress.as_ref(),
            vec![WorkerEvent::StageStarted {
                stage: Stage::Verify,
            }],
        )
        .await;
        let started = Instant::now();
        let chosen = pick(&prepared, payload.images.iter().map(|i| i.image_id));
        let new_photo = payload.reason == VerifyReason::NewPhoto;
        let indicators = self
            .indicators(&payload.candidates, &chosen, new_photo)
            .await?;
        send(
            progress.as_ref(),
            vec![WorkerEvent::StageCompleted {
                stage: Stage::Verify,
                duration_ms: ms(started),
                skipped: None,
            }],
        )
        .await;
        Ok(VerifyResult { indicators })
    }

    /// One squeeze or jar test image, alone, with the schema branch of its kind.
    async fn read_guided_check(
        &self,
        _job: JobContext,
        payload: &GuidedPayload,
        image: &LoadedImage,
    ) -> Result<GuidedResult, ModelError> {
        let kind = payload.image.kind.guided().ok_or(ModelError::Unsupported {
            model: self.backend.name(),
            what: "a guided job whose image isn't a squeeze or jar test",
        })?;
        let prepared = self.prepare(std::slice::from_ref(image)).await?;
        let images: Vec<&PreparedImage> = prepared.iter().collect();
        let (labels, _) = Self::labelled(&images, false);
        let schema = schema::guided(kind);
        let call = VlmCall {
            system: &self.system,
            images: vlm_images(&labels, &images),
            instructions: prompt::guided(kind),
            schema_name: "guided",
            schema: &schema,
            max_output_tokens: self.profile.budget.guided,
        };
        let texture = self
            .call_validated(&call, |text| {
                decode::guided(text, kind, &self.profile.certainty)
            })
            .await?;
        Ok(GuidedResult { texture })
    }
}

#[cfg(test)]
mod tests;
