//! A driver for the worker's own VLM requests against a live OpenAI-compatible server: the seed
//! of the `vlm` job harness, task P3 of [docs/vision/dev-roadmap.md].
//!
//! [docs/vision/dev-roadmap.md]: ../../../../../../docs/vision/dev-roadmap.md

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::Bytes;
use clap::ValueEnum as _;
use domain::{
    jobs::{GuidedPayload, JobImage, VerifyPayload, VerifyReason, VlmPayload},
    ports::{JobContext, LoadedImage, ModelError, ProgressError, ProgressSink, VisionModel},
    scan::ImageKind,
    vlm::{BoxFormat, GuidedTexture, VlmFindings, VlmIndicatorAnswers},
};
use integrations::{
    anthropic::{AnthropicClient, AnthropicConfig},
    http::{HttpConfig, RetryPolicy, SecretString},
    openai_compat::{OpenAiCompatClient, OpenAiCompatConfig, ServerFlavour},
};
use tokio::sync::Semaphore;
use vision::{
    imaging::{self, DecodeLimits, ImageView},
    upload::UploadConfig,
};

use crate::config::VlmServerKind;
use crate::vlm::{
    VlmAdapter,
    backend::{AnthropicBackend, OpenAiBackend, Stop, Tokens, VlmBackend, VlmCall, VlmReply},
    profile::{AdapterProfile, CallBudget, CertaintyMap, VlmIdentity},
};

/// The tomato case: whole plant, leaf close-up and soil (so the soil readings are asked), three
/// candidates of the disease head's tomato rows.
const TOMATO: &str = include_str!("fixtures/tomato_leaf.json");
/// The corn case: whole plant and leaf close-up, no soil image, three corn candidates.
const CORN: &str = include_str!("fixtures/corn_leaf.json");

/// The image ids of the fixtures, in the order the photos of `VLM_RECORD_IMAGES` are assigned
/// to them (cycling when fewer are given): tomato whole plant, tomato leaf, tomato soil, corn
/// whole plant, corn leaf, squeeze test, jar test.
const SLOTS: [&str; 7] = [
    "01JAX6AAAAAAAAAAAAAAAAAAA1",
    "01JAX6AAAAAAAAAAAAAAAAAAA2",
    "01JAX6AAAAAAAAAAAAAAAAAAA3",
    "01JAX6AAAAAAAAAAAAAAAAAAA4",
    "01JAX6AAAAAAAAAAAAAAAAAAA5",
    "01JAX6AAAAAAAAAAAAAAAAAAA6",
    "01JAX6AAAAAAAAAAAAAAAAAAA7",
];
const JOB: &str = "01JAX5AAAAAAAAAAAAAAAAAAAA";

/// One backend call as the adapter made it.
pub(super) struct Attempt {
    pub(super) schema: String,
    pub(super) images: usize,
    pub(super) budget: u32,
    pub(super) ms: u128,
    pub(super) outcome: Result<(Stop, usize), String>,
    /// The tokens used, when the backend reports them.
    pub(super) tokens: Option<Tokens>,
    /// The answer as sent, before decoding: its boxes in the model's own coordinates.
    pub(super) text: Option<String>,
    /// Each image's size as sent (width, height), which `abs_px_prepared` boxes are in.
    pub(super) sizes: Vec<(u32, u32)>,
}

/// The real backend, unchanged, with a log of each call: the adapter retries inside, so this
/// is the only place a cut-off first answer shows.
pub(super) struct Recording<B> {
    inner: B,
    pub(super) log: Mutex<Vec<Attempt>>,
}

impl<B> Recording<B> {
    const fn new(inner: B) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
        }
    }
}

/// A JPEG's size from its header.
fn jpeg_size(jpeg: &[u8]) -> (u32, u32) {
    image::ImageReader::with_format(std::io::Cursor::new(jpeg), image::ImageFormat::Jpeg)
        .into_dimensions()
        .unwrap_or((0, 0))
}

impl<B: VlmBackend> VlmBackend for Recording<B> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn max_questions(&self) -> Option<usize> {
        self.inner.max_questions()
    }

    async fn call<'a>(&'a self, call: &'a VlmCall<'a>) -> Result<VlmReply, ModelError> {
        let started = Instant::now();
        let reply = self.inner.call(call).await;
        self.log.lock().unwrap().push(Attempt {
            schema: call.schema_name.to_owned(),
            images: call.images.len(),
            budget: call.max_output_tokens,
            ms: started.elapsed().as_millis(),
            outcome: reply
                .as_ref()
                .map(|r| (r.stop, r.text.len()))
                .map_err(ToString::to_string),
            tokens: reply.as_ref().ok().and_then(|r| r.tokens),
            text: reply.as_ref().ok().map(|r| r.text.clone()),
            sizes: call.images.iter().map(|i| jpeg_size(i.jpeg)).collect(),
        });
        reply
    }
}

pub(super) struct Discard;

#[async_trait]
impl ProgressSink for Discard {
    async fn send(&self, _events: Vec<domain::jobs::WorkerEvent>) -> Result<(), ProgressError> {
        Ok(())
    }
}

/// `VLM_ADAPTER` as `main.rs` reads it, `vllm` when unset: llama.cpp takes `max_tokens` and
/// `repeat_penalty` where vLLM takes `max_completion_tokens` and `repetition_penalty`.
fn flavour() -> ServerFlavour {
    std::env::var("VLM_ADAPTER").map_or(ServerFlavour::Vllm, |v| {
        VlmServerKind::from_str(&v, false)
            .unwrap_or_else(|e| panic!("VLM_ADAPTER={v}: {e}"))
            .flavour()
    })
}

/// The adapter `main.rs` builds for `--caps vlm` with its defaults (`VLM_ADAPTER=vllm`,
/// `VLM_SERVED_NAME=vlm`, `norm1000`, 32 px cells, 1,003,520 pixels, the local budgets), with
/// the backend wrapped in [`Recording`]; `VLM_ADAPTER=llamacpp` sends llama.cpp's fields.
async fn adapter(base_url: &str) -> VlmAdapter<Recording<OpenAiBackend>> {
    adapter_with(base_url, 1_003_520, Arc::new(Semaphore::new(1))).await
}

/// [`adapter`] with another `VLM_MAX_PIXELS` and a shared CPU semaphore, for the benchmark.
pub(super) async fn adapter_with(
    base_url: &str,
    max_pixels: u32,
    cpu: Arc<Semaphore>,
) -> VlmAdapter<Recording<OpenAiBackend>> {
    let http = integrations::http::client(&HttpConfig {
        read_timeout: Duration::from_secs(600),
        ..HttpConfig::default()
    })
    .unwrap();
    let client = OpenAiCompatClient::new(
        http,
        OpenAiCompatConfig {
            base_url: base_url.to_owned(),
            api_key: std::env::var("VLM_API_KEY").ok().map(SecretString::from),
            timeout: Duration::from_secs(240),
            flavour: flavour(),
            retry: RetryPolicy::NONE,
        },
    );
    let served = client.models().await.unwrap();
    assert!(
        served.iter().any(|m| m == "vlm"),
        "the server serves {served:?}, not vlm"
    );
    let profile = AdapterProfile {
        box_format: BoxFormat::Norm1000,
        cell_px: 32,
        max_pixels,
        max_long_side: None,
        jpeg_quality: 90,
        budget: CallBudget::LOCAL,
        certainty: CertaintyMap::default(),
        // Only the label the adapter stores with its results: it never reaches the server or the
        // recording, so it stays FP8 when another model is recorded. Each recording's `models`
        // field names the checkpoint that answered.
        identity: VlmIdentity {
            model: "Qwen/Qwen3-VL-8B-Instruct-FP8".to_owned(),
            quant: Some("fp8_block".to_owned()),
            server: "vllm@v0.31.0".to_owned(),
        },
    };
    VlmAdapter::new(
        Recording::new(OpenAiBackend::new(client, "vlm".to_owned())),
        profile,
        cpu,
    )
}

/// `FALLBACK_MAX_PIXELS`'s default: the most pixels the hosted API takes without resizing.
pub(super) const FALLBACK_MAX_PIXELS: u32 = 1_150_000;

/// The adapter `main.rs` builds for `--caps fallback` with its defaults (`abs_px_prepared`,
/// 1 px cells, at most 1,150,000 pixels and 1568 on the long side, the hosted budgets, no
/// retries and 300 s a call), calling `model` with `LLM_API_KEY`, with the backend wrapped in
/// [`Recording`] (task P9).
pub(super) fn hosted_adapter(
    model: &str,
    cpu: Arc<Semaphore>,
) -> VlmAdapter<Recording<AnthropicBackend>> {
    let http = integrations::http::client(&HttpConfig {
        read_timeout: Duration::from_secs(600),
        ..HttpConfig::default()
    })
    .unwrap();
    let key = std::env::var("LLM_API_KEY").expect("LLM_API_KEY is not set");
    let mut config = AnthropicConfig::new(SecretString::from(key));
    config.retry = RetryPolicy::NONE;
    config.request_timeout = Duration::from_secs(300);
    model.clone_into(&mut config.models.vision_fallback);
    let profile = AdapterProfile {
        box_format: BoxFormat::AbsPxPrepared,
        cell_px: 1,
        max_pixels: FALLBACK_MAX_PIXELS,
        max_long_side: Some(1568),
        jpeg_quality: 90,
        budget: CallBudget::HOSTED,
        certainty: CertaintyMap::default(),
        identity: VlmIdentity {
            model: model.to_owned(),
            quant: None,
            server: "anthropic".to_owned(),
        },
    };
    VlmAdapter::new(
        Recording::new(AnthropicBackend::new(
            AnthropicClient::new(http, config),
            model.to_owned(),
        )),
        profile,
        cpu,
    )
}

/// A photo as the bucket would hold it: decoded with its EXIF orientation applied, resized to
/// the default stored long side and re-encoded as `WebP` (`vision::upload`, without the face
/// blur, which needs ONNX Runtime).
pub(super) fn stored(path: &str, image: &JobImage) -> LoadedImage {
    let file = std::fs::read(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let config = UploadConfig::default();
    let decoded = imaging::decode_upload(&file, DecodeLimits::default())
        .unwrap_or_else(|e| panic!("decoding {path}: {e}"));
    let resized =
        imaging::resize_long_side(ImageView::from(&decoded.image), config.default_long_side)
            .unwrap();
    let webp = imaging::encode_webp(ImageView::from(&resized), config.webp_quality);
    LoadedImage {
        image: JobImage {
            width: resized.width(),
            height: resized.height(),
            ..image.clone()
        },
        bytes: Bytes::from(webp),
    }
}

/// The photos of `VLM_RECORD_IMAGES`, assigned to the fixtures' images by [`SLOTS`].
struct Photos(Vec<String>);

impl Photos {
    fn load(&self, image: &JobImage) -> LoadedImage {
        let slot = SLOTS
            .iter()
            .position(|s| image.image_id.to_string() == *s)
            .unwrap_or_else(|| panic!("no slot for {}", image.image_id));
        stored(&self.0[slot % self.0.len()], image)
    }

    /// Every image of a payload, each once, with the payload's sizes set to the stored ones.
    fn payload(&self, payload: &mut VlmPayload) -> Vec<LoadedImage> {
        let loaded: Vec<LoadedImage> = payload
            .findings_images
            .iter()
            .chain(&payload.indicator_images)
            .fold(Vec::<&JobImage>::new(), |mut seen, i| {
                if !seen.iter().any(|s| s.image_id == i.image_id) {
                    seen.push(i);
                }
                seen
            })
            .into_iter()
            .map(|i| self.load(i))
            .collect();
        for image in payload
            .findings_images
            .iter_mut()
            .chain(&mut payload.indicator_images)
        {
            if let Some(l) = loaded.iter().find(|l| l.image.image_id == image.image_id) {
                image.clone_from(&l.image);
            }
        }
        loaded
    }
}

pub(super) fn ctx() -> JobContext {
    JobContext {
        job_id: JOB.parse().unwrap(),
        scan_id: JOB.parse().unwrap(),
        run: None,
        attempt: 1,
    }
}

fn guided_image(slot: usize, kind: ImageKind) -> JobImage {
    let image_id = SLOTS[slot].parse().unwrap();
    JobImage {
        image_id,
        kind,
        width: 1600,
        height: 1200,
        url: domain::jobs::WorkerImagePath::new(JOB.parse().unwrap(), image_id),
    }
}

fn findings_line(f: &VlmFindings) -> String {
    let species = f.species_guess.species_id.as_ref().map_or_else(
        || {
            format!(
                "other:{}",
                f.species_guess.other_name.as_deref().unwrap_or("?")
            )
        },
        ToString::to_string,
    );
    let top = f
        .organisms
        .iter()
        .map(|o| {
            o.organism_id
                .as_ref()
                .map_or_else(|| "other".to_owned(), ToString::to_string)
        })
        .chain(f.damage.iter().map(|d| format!("{:?}", d.pattern)))
        .next()
        .unwrap_or_else(|| "nothing".to_owned());
    let boxes = f.organisms.len() + f.damage.iter().filter(|d| d.region.is_some()).count();
    format!(
        "species {species} ({:.2}), top {top}, {boxes} boxes, {} soil readings",
        f.species_guess.confidence.get(),
        f.soil.len()
    )
}

fn answers_line(a: &VlmIndicatorAnswers) -> String {
    let count = |want| a.answers.iter().filter(|x| x.answer == want).count();
    format!(
        "{} answers: {} yes, {} no, {} unsure",
        a.answers.len(),
        count(domain::scan::Answer::Yes),
        count(domain::scan::Answer::No),
        count(domain::scan::Answer::Unsure)
    )
}

/// Prints what one entry point decoded and the backend calls behind it, then clears the log.
#[allow(
    clippy::print_stderr,
    reason = "the run's summary is the output of this manual test"
)]
fn report(case: &str, decoded: &str, log: &Mutex<Vec<Attempt>>) {
    eprintln!("{case}: {decoded}");
    for a in log.lock().unwrap().drain(..) {
        let outcome = match a.outcome {
            Ok((stop, chars)) => format!("finish {stop:?}, {chars} chars"),
            Err(e) => format!("error {e}"),
        };
        eprintln!(
            "  call {}: {} images, budget {} tokens, {} ms, {outcome}",
            a.schema, a.images, a.budget, a.ms
        );
    }
}

/// Sends the worker's own VLM requests (its prompts, per-call schemas, image preparation and
/// validation) to a live OpenAI-compatible server, through the same entry points a job takes:
/// `analyse` (findings, then indicators) for each fixture case, `verify` with a new photo, and
/// `read_guided_check` for a squeeze test and a jar test. Every call must decode and validate.
/// Point it at a recording proxy in front of vLLM to record Qwen3-VL-8B's answers, or at the
/// fake server (`ml/training/eval/fake_vlm.py`) to check that the replays decode. It is the
/// seed of the `vlm` job harness, task P3 of `docs/vision/dev-roadmap.md`.
///
/// ```text
/// cd backend
/// VLM_RECORD_BASE_URL=http://127.0.0.1:8001 VLM_RECORD_IMAGES=a.jpg,b.jpg \
///   cargo nextest run -p worker --run-ignored only record_against_live_server --no-capture
/// # or: cargo test -p worker record_against_live_server -- --ignored --nocapture
/// ```
///
/// - `VLM_RECORD_BASE_URL`: the server, without `/v1`; it must serve the model as `vlm`.
///   Unset, the test returns at once.
/// - `VLM_API_KEY`: the server's `--api-key`, if it has one.
/// - `VLM_ADAPTER`: `llamacpp` for llama.cpp's `llama-server`; `vllm` when unset.
/// - `VLM_RECORD_IMAGES`: at least 2 photo paths (JPEG, PNG or `WebP`), comma- or
///   colon-separated, assigned in [`SLOTS`] order and reused cyclically: give 7 to have each
///   shot (tomato plant, leaf, soil; corn plant, leaf; squeeze test; jar test) its own photo.
#[tokio::test]
#[ignore = "needs a VLM server"]
async fn record_against_live_server() {
    let Ok(base_url) = std::env::var("VLM_RECORD_BASE_URL") else {
        #[allow(clippy::print_stderr, reason = "says why the manual test did nothing")]
        {
            eprintln!("VLM_RECORD_BASE_URL is unset: nothing to record");
        }
        return;
    };
    let paths: Vec<String> = std::env::var("VLM_RECORD_IMAGES")
        .unwrap_or_default()
        .split([',', ':'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(
        paths.len() >= 2,
        "VLM_RECORD_IMAGES needs at least 2 photo paths"
    );
    let photos = Photos(paths);
    let a = adapter(&base_url).await;
    let log = &a.backend.log;

    for (case, fixture) in [("tomato", TOMATO), ("corn", CORN)] {
        let mut payload: VlmPayload = serde_json::from_str(fixture).unwrap();
        let images = photos.payload(&mut payload);
        let result = a.analyse(ctx(), &payload, &images, Arc::new(Discard)).await;
        let result = match result {
            Ok(r) => r,
            Err(e) => {
                report(case, "failed", log);
                panic!("{case}: analyse failed: {e}");
            }
        };
        let expected: usize = payload.candidates.iter().map(|c| c.indicators.len()).sum();
        assert_eq!(result.indicators.answers.len(), expected, "{case}");
        report(
            case,
            &format!(
                "{}; {}",
                findings_line(&result.findings),
                answers_line(&result.indicators)
            ),
            log,
        );

        if case == "tomato" {
            // the leaf close-up as a photo sent for a question: first, and labelled as new
            let verify = VerifyPayload {
                reason: VerifyReason::NewPhoto,
                question_id: None,
                images: payload.indicator_images.clone(),
                candidates: payload.candidates.clone(),
            };
            let result = a.verify(ctx(), &verify, &images, Arc::new(Discard)).await;
            let result = match result {
                Ok(r) => r,
                Err(e) => {
                    report("tomato verify", "failed", log);
                    panic!("tomato verify failed: {e}");
                }
            };
            assert_eq!(result.indicators.answers.len(), expected);
            report("tomato verify", &answers_line(&result.indicators), log);
        }
    }

    for (slot, kind) in [(5, ImageKind::SqueezeTest), (6, ImageKind::JarTest)] {
        let payload = GuidedPayload {
            image: guided_image(slot, kind),
        };
        let image = photos.load(&payload.image);
        let name = format!("guided {}", kind.as_str());
        let result = a.read_guided_check(ctx(), &payload, &image).await;
        let texture = match result {
            Ok(r) => r.texture,
            Err(e) => {
                report(&name, "failed", log);
                panic!("{name} failed: {e}");
            }
        };
        let kind_matches = matches!(
            (kind, texture),
            (ImageKind::SqueezeTest, GuidedTexture::SqueezeTest { .. })
                | (ImageKind::JarTest, GuidedTexture::JarTest { .. })
        );
        assert!(kind_matches, "{name}: {texture:?}");
        report(&name, &format!("{texture:?}"), log);
    }
}
