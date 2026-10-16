//! The PDM worker: `worker --caps perception|vlm|fallback`. It pulls jobs from the API, runs
//! them on its model, and reports back ([contracts 5]; [architecture 8.4]).

use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser as _;
use domain::{id::WorkerId, ports::VisionModel};
use integrations::{
    anthropic::{AnthropicClient, AnthropicConfig},
    http::{HttpConfig, RetryPolicy},
    openai_compat::{OpenAiCompatClient, OpenAiCompatConfig},
};
use tokio::sync::Semaphore;
use vision::{
    manifest::Manifest,
    perception::{BatchCaps, PerceptionConfig, PerceptionPipeline},
    runtime::{OrtRuntime, RuntimeConfig},
};

mod api;
mod config;
mod fallback;
mod job;
mod lifecycle;
mod vlm;

use api::HttpWorkerApi;
use config::{Caps, Cli, Timings};
use lifecycle::{Identity, Worker, run_until_sigterm};
use vlm::{
    VlmAdapter,
    backend::{AnthropicBackend, OpenAiBackend},
    profile::{AdapterProfile, CallBudget, CertaintyMap, VlmIdentity},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let worker_id: WorkerId = cli.worker_id.parse().context("WORKER_ID")?;
    let timings = Timings::new(Duration::from_secs(cli.drain_seconds));
    // One reqwest client per process. Its read gap is long: a non-streamed VLM call can wait
    // a minute or more for its response headers, which a 60 s read timeout would cut.
    let http = integrations::http::client(&HttpConfig {
        read_timeout: Duration::from_secs(600),
        ..HttpConfig::default()
    })
    .context("building the HTTP client")?;
    let cpu = Arc::new(Semaphore::new(cli.cpu_permits.max(1)));
    let model = build_model(&cli, http.clone(), Arc::clone(&cpu)).await?;
    let api = HttpWorkerApi::new(
        http,
        &cli.api_base_url,
        cli.worker_token.clone(),
        timings.claim_wait_s,
    );
    let identity = Identity {
        worker_id,
        caps: vec![cli.caps.capability()],
        slots: cli.slots(),
    };
    tracing::info!(caps = ?cli.caps, slots = identity.slots, "worker starting");
    let worker = Arc::new(Worker::new(api, model, identity, timings));
    run_until_sigterm(worker).await?;
    Ok(())
}

/// The model for `--caps`, loaded before the first claim so a missing driver, file or key fails
/// start-up instead of a job.
async fn build_model(
    cli: &Cli,
    http: reqwest::Client,
    cpu: Arc<Semaphore>,
) -> anyhow::Result<Arc<dyn VisionModel>> {
    Ok(match cli.caps {
        Caps::Perception => {
            let p = &cli.perception;
            let manifest = Manifest::load(&p.manifest).context("MODEL_MANIFEST")?;
            let runtime = OrtRuntime::init(&RuntimeConfig {
                ep: p.ort_ep,
                device_id: p.ort_device_id,
                dylib_path: p.ort_dylib_path.clone(),
                intra_threads: p.ort_intra_threads,
                name: "perception-worker",
            })?;
            let config = PerceptionConfig {
                caps: BatchCaps {
                    job: p.max_batch,
                    detector: p.detector_max_batch,
                    sam_encoder: p.sam_encoder_max_batch,
                    sam_boxes: p.sam_max_boxes,
                    bioclip: p.bioclip_max_batch,
                },
                ..PerceptionConfig::default()
            };
            Arc::new(PerceptionPipeline::load(runtime, &manifest, config, cli.cpu_permits).await?)
        }
        Caps::Vlm => {
            let v = &cli.vlm;
            let client = OpenAiCompatClient::new(
                http,
                OpenAiCompatConfig {
                    base_url: v.base_url.clone(),
                    api_key: v.api_key.clone(),
                    timeout: Duration::from_secs(240),
                    flavour: v.adapter.flavour(),
                    retry: RetryPolicy::NONE,
                },
            );
            let served = client
                .models()
                .await
                .context("the VLM server's /v1/models")?;
            anyhow::ensure!(
                served.iter().any(|m| m == &v.served_name),
                "the VLM server serves {served:?}, not {}",
                v.served_name
            );
            let profile = AdapterProfile {
                box_format: v.box_format.format(),
                cell_px: v.cell_px,
                max_pixels: v.max_pixels,
                max_long_side: None,
                jpeg_quality: 90,
                budget: CallBudget::LOCAL,
                certainty: CertaintyMap::default(),
                identity: VlmIdentity {
                    model: v.model.clone(),
                    quant: Some(v.quant.clone()),
                    server: v.server.clone(),
                },
            };
            Arc::new(VlmAdapter::new(
                OpenAiBackend::new(client, v.served_name.clone()),
                profile,
                cpu,
            ))
        }
        Caps::Fallback => {
            let f = &cli.fallback;
            let key = f.api_key.clone().context("LLM_API_KEY")?;
            let mut config = AnthropicConfig::new(key);
            config.retry = RetryPolicy::NONE;
            config.request_timeout = Duration::from_secs(300);
            config.models.vision_fallback.clone_from(&f.model);
            let client = AnthropicClient::new(http, config);
            let profile = AdapterProfile {
                box_format: domain::vlm::BoxFormat::AbsPxPrepared,
                cell_px: 1,
                max_pixels: f.max_pixels,
                max_long_side: Some(f.max_long_side),
                jpeg_quality: 90,
                budget: CallBudget::HOSTED,
                certainty: CertaintyMap::default(),
                identity: VlmIdentity {
                    model: f.model.clone(),
                    quant: None,
                    server: "anthropic".to_owned(),
                },
            };
            let manifest = Manifest::load(&cli.perception.manifest).context("MODEL_MANIFEST")?;
            let labels = fallback::FallbackLabels::load(&manifest)?;
            let adapter =
                VlmAdapter::new(AnthropicBackend::new(client, f.model.clone()), profile, cpu);
            Arc::new(fallback::FallbackModel::new(adapter, labels))
        }
    })
}
