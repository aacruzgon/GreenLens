//! One scan through the real pipeline and the real workers (task P8 of
//! [docs/vision/dev-roadmap.md]): `api`'s own router, served on localhost over the embedded
//! `SurrealDB` with the schema and the committed `knowledge.surql`, and the `worker` binary
//! claiming its jobs over HTTP, as on the rig. The photos are uploaded through the upload route
//! (EXIF, face blurring), the scan is submitted, and the harness waits for its resting point: a
//! diagnosis, a question, or a failure. It prints the scan's events and writes them, with the
//! scan and its indicator evidence, to `SCAN_HARNESS_OUT`.
//!
//! What it leaves out of production: Clerk (any bearer is one user), `RustFS` (photos and masks
//! in memory), the geocoder (the garden is at `SCAN_HARNESS_AT`, in Toronto's time zone),
//! `SoilGrids` (no soil profile), push notifications and the chat agent. The weather is
//! Open-Meteo's. The plan is written only when `LLM_API_KEY` is set; without it the plan step
//! waits until the harness ends, so the scan rests at `diagnosed`.
//!
//! [docs/vision/dev-roadmap.md]: ../../../../../docs/vision/dev-roadmap.md
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a manual harness: it reports the scan on the terminal"
)]

use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt as _;
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower::ServiceExt as _;

use domain::{
    authz::{Viewer, check_membership_of},
    events::{ScanEvent, ScanEventBody},
    garden::{gardens::TimeZone, push::PushMessage},
    id::{AuthId, GardenId, ScanId},
    knowledge::SoilProfile,
    ports::{
        AuthError, AuthVerifier, Clock, GeocodeQuery, Geocoded, Geocoder, KnowledgeRepo,
        KnowledgeStore, MockAuthAdmin, MockWebhookVerifier, NoBeliefs, Notifier, ProviderError,
        PushReport, SoilProvider, Store, VerifiedToken,
    },
    records::{diagnosis::IndicatorEvidence, weather::Coordinates},
    scan::ScanStatus,
    url::Stored,
    wire::{EventSeq, Timestamp},
};

use crate::{
    broker::JobBroker,
    config::Config,
    deletion::DeletionRunner,
    events::{hub::EventHub, writer::ScanWriters},
    limits::RateLimits,
    push::Pusher,
    router,
    signing::{HmacSigner, PhotoIndex},
    state::{AppState, Ports},
    wiring::{SystemClock, UlidGen},
    workers::WorkerTokens,
};

use super::NoChat;

/// The bearer every app request carries; [`OneUser`] signs it in.
const BEARER: &str = "Bearer scan-harness";
/// Toronto, where the team's gardens are.
const TORONTO: (f64, f64) = (43.6532, -79.3832);
/// The capabilities the harness starts a worker for, unless `SCAN_HARNESS_SPAWN` says otherwise.
const CAPS: &str = "perception,vlm";

/// Signs in any bearer as one user.
struct OneUser;

#[async_trait::async_trait]
impl AuthVerifier for OneUser {
    async fn verify(&self, _bearer: &str, now: Timestamp) -> Result<VerifiedToken, AuthError> {
        Ok(VerifiedToken {
            auth_id: auth_id(),
            display_name: Some("Scan harness".into()),
            expires_at: now.plus(Duration::from_hours(1)),
        })
    }
}

fn auth_id() -> AuthId {
    "user_scanharness".parse().expect("an auth id")
}

/// Every place is the harness's garden point, in Toronto's time zone.
struct OnePlace(Coordinates);

#[async_trait::async_trait]
impl Geocoder for OnePlace {
    async fn geocode<'a>(&self, _query: GeocodeQuery<'a>) -> Result<Geocoded, ProviderError> {
        Ok(Geocoded {
            coordinates: self.0,
            label: "Scan harness".into(),
            time_zone: TimeZone("America/Toronto".into()),
        })
    }
}

/// No soil profile: the diagnosis's evidence basis goes without `SoilGrids`' pH.
struct NoSoil;

#[async_trait::async_trait]
impl SoilProvider for NoSoil {
    async fn soil_profile(&self, _at: Coordinates) -> Result<SoilProfile, ProviderError> {
        Err(ProviderError::Unavailable {
            provider: "soilgrids",
            source: "the scan harness has no soil data".into(),
        })
    }
}

/// No phone to notify.
struct NoPush;

#[async_trait::async_trait]
impl Notifier for NoPush {
    async fn push(&self, _messages: Vec<PushMessage>) -> Result<PushReport, ProviderError> {
        Ok(PushReport::default())
    }
}

/// Without `LLM_API_KEY`: the plan step waits until the harness ends, then fails.
struct HeldPlans(CancellationToken);

#[async_trait::async_trait]
impl pipeline::PlanAuthor for HeldPlans {
    async fn write_plan(
        &self,
        _input: &agent::plan_writer::PlanInput<'_>,
    ) -> Result<agent::plan_writer::WrittenPlan, agent::PlanWriterError> {
        self.0.cancelled().await;
        Err(agent::PlanWriterError::Refused)
    }
}

/// The harness's settings, from the environment.
struct Settings {
    photos: Vec<(String, PathBuf)>,
    manifest: PathBuf,
    species: Option<String>,
    at: Coordinates,
    worker: PathBuf,
    spawn: Vec<String>,
    out: Option<PathBuf>,
    timeout: Duration,
    port: u16,
    llm_key: Option<String>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Settings {
    fn from_env() -> Self {
        let photos = var("SCAN_HARNESS_PHOTOS")
            .expect("SCAN_HARNESS_PHOTOS: kind=path pairs, comma-separated, in upload order")
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|item| {
                let (kind, path) = item
                    .split_once('=')
                    .unwrap_or_else(|| panic!("{item}: expected kind=path"));
                (kind.trim().to_owned(), PathBuf::from(path.trim()))
            })
            .collect();
        let at = var("SCAN_HARNESS_AT").map_or(TORONTO, |s| {
            let (lat, lon) = s.split_once(',').expect("SCAN_HARNESS_AT: lat,lon");
            (
                lat.trim().parse().expect("a latitude"),
                lon.trim().parse().expect("a longitude"),
            )
        });
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Self {
            photos,
            manifest: PathBuf::from(
                var("MODEL_MANIFEST").expect("MODEL_MANIFEST: the manifest the workers load"),
            ),
            species: var("SCAN_HARNESS_SPECIES"),
            at: Coordinates {
                lat: at.0,
                lon: at.1,
            },
            worker: var("SCAN_HARNESS_WORKER")
                .map_or_else(|| workspace.join("target/release/worker"), PathBuf::from),
            spawn: std::env::var("SCAN_HARNESS_SPAWN")
                .unwrap_or_else(|_| CAPS.to_owned())
                .split(',')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_owned)
                .collect(),
            out: var("SCAN_HARNESS_OUT").map(PathBuf::from),
            timeout: Duration::from_secs(
                var("SCAN_HARNESS_TIMEOUT").map_or(900, |s| s.parse().expect("seconds")),
            ),
            port: var("SCAN_HARNESS_PORT").map_or(0, |s| s.parse().expect("a port")),
            llm_key: var("LLM_API_KEY"),
        }
    }
}

/// The worker id and token of each capability.
fn worker_id(cap: &str) -> String {
    format!("harness-{cap}-1")
}

fn worker_token(cap: &str) -> String {
    format!("scan-harness-token-for-the-{cap}-worker-0000")
}

fn config(base: &str, llm_key: Option<&str>) -> Config {
    let long = "h".repeat(40);
    let tokens = CAPS
        .split(',')
        .map(|cap| format!("{}={}", worker_id(cap), worker_token(cap)))
        .collect::<Vec<_>>()
        .join(",");
    let mut vars = BTreeMap::from([
        ("CLERK_ISSUER", "https://clerk.example.org".to_owned()),
        ("API_URL_SIGNING_KEY", long.clone()),
        ("API_PUBLIC_BASE", base.to_owned()),
        ("WORKER_TOKENS", tokens),
        ("SURREAL_PASS", "unused".to_owned()),
        ("SURREAL_ROOT_PASS", "unused".to_owned()),
        ("RUSTFS_ACCESS_KEY", "unused".to_owned()),
        ("RUSTFS_SECRET_KEY", "unused".to_owned()),
        ("CLERK_SECRET_KEY", "unused".to_owned()),
        ("CLERK_WEBHOOK_SECRET", "whsec_unused".to_owned()),
        ("LLM_API_KEY", llm_key.unwrap_or("unused").to_owned()),
    ]);
    if let Some(key) = var("OPEN_METEO_API_KEY") {
        vars.insert("OPEN_METEO_API_KEY", key);
    }
    Config::from_lookup(|k| vars.get(k).cloned()).expect("the harness's configuration")
}

/// The CPU models `api` loads, from the workers' manifest.
struct Models {
    embedder: Arc<dyn domain::ports::TextEmbedder>,
    images: Arc<vision::upload::UploadProcessor>,
    measurer: Arc<dyn pipeline::ImageMeasurer>,
}

async fn load_models(manifest: PathBuf) -> Models {
    tokio::task::spawn_blocking(move || {
        let runtime =
            vision::runtime::OrtRuntime::init(&vision::runtime::RuntimeConfig::cpu("api"))
                .expect("ONNX Runtime (ORT_DYLIB_PATH)");
        let manifest = vision::manifest::Manifest::load(&manifest).expect("the manifest");
        let embedder = vision::embed::ArcticEmbedder::load(
            &runtime,
            &manifest,
            vision::embed::EmbedderConfig::default(),
        )
        .expect("the text embedder");
        let images = vision::upload::UploadProcessor::load(
            &runtime,
            &manifest,
            vision::upload::UploadConfig::default(),
        )
        .expect("the face detector");
        let spots = Arc::new(vision::measure::spots::ColourSpotSegmenter::default());
        let measurer =
            vision::measure::Measurer::new(vision::measure::MeasurementConfig::default(), spots);
        Models {
            embedder: Arc::new(embedder),
            images: Arc::new(images),
            measurer: Arc::new(pipeline::measure::VisionMeasurer::new(Arc::new(measurer))),
        }
    })
    .await
    .expect("the model loader")
}

/// The plan writer on Anthropic's API, as `wiring` builds it, or [`HeldPlans`].
async fn plan_author(
    llm_key: Option<&str>,
    knowledge: &dyn KnowledgeStore,
    shutdown: &CancellationToken,
) -> Arc<dyn pipeline::PlanAuthor> {
    let Some(key) = llm_key else {
        return Arc::new(HeldPlans(shutdown.clone()));
    };
    let http = integrations::http::client(&integrations::http::HttpConfig::default())
        .expect("an HTTP client");
    let llm = integrations::anthropic::AnthropicClient::new(
        http,
        integrations::anthropic::AnthropicConfig::new(SecretString::from(key.to_owned())),
    );
    let products = Arc::new(agent::safety::ProductCatalogue::new(
        &knowledge.products().await.expect("the product catalogue"),
    ));
    Arc::new(agent::PlanWriter::new(
        Arc::new(agent::model::AnthropicChat::new(llm)),
        products,
    ))
}

/// The embedded store with the schema and the committed migrations, `knowledge.surql`
/// included, and the knowledge base over it.
async fn databases() -> (
    Arc<store::SurrealStore>,
    Arc<dyn KnowledgeStore>,
    PhotoIndex,
) {
    let db = surrealdb::engine::any::connect("mem://")
        .await
        .expect("the embedded engine");
    let applied = store::migrate::migrate_with(&db, store::migrate::MIGRATIONS)
        .await
        .expect("the migrations");
    assert!(applied.knowledge, "knowledge.surql applied");
    let surreal = Arc::new(
        store::SurrealStore::from_client(db)
            .await
            .expect("the store"),
    );
    let repo: Arc<dyn KnowledgeRepo> = surreal.clone();
    let knowledge: Arc<dyn KnowledgeStore> = Arc::new(
        knowledge::SurrealKnowledgeStore::load(Arc::clone(&repo))
            .await
            .expect("the knowledge base"),
    );
    let photos = PhotoIndex::new(
        repo.load_knowledge()
            .await
            .expect("the knowledge snapshot")
            .species_photos,
    );
    (surreal, knowledge, photos)
}

/// `api`'s state as `wiring::build` assembles it, over the harness's ports.
#[expect(
    clippy::too_many_lines,
    reason = "every port and in-process piece is built here, in start-up order, as in wiring"
)]
async fn state(
    settings: &Settings,
    base: &str,
    shutdown: &CancellationToken,
    tasks: &TaskTracker,
) -> Arc<AppState> {
    let config = Arc::new(config(base, settings.llm_key.as_deref()));
    let (surreal, knowledge, photos) = databases().await;
    let models = load_models(settings.manifest.clone()).await;
    let meta = surreal
        .knowledge_meta()
        .await
        .expect("knowledge_meta")
        .expect("a knowledge set");
    let model = models.embedder.model();
    assert!(
        meta.matches_embedder(&model.name, &model.version, model.dimension),
        "the knowledge set was embedded with {}, not {}@{}",
        meta.embedding_model,
        model.name,
        model.version
    );
    let store: Arc<dyn Store> = surreal.clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let blob: Arc<dyn domain::ports::BlobStore> =
        Arc::new(blob::MemoryBlobStore::new(Arc::clone(&clock)));
    let http = integrations::http::client(&integrations::http::HttpConfig::default())
        .expect("an HTTP client");
    let weather = Arc::new(integrations::open_meteo::OpenMeteo::new(
        http,
        integrations::open_meteo::OpenMeteoConfig {
            api_key: config.providers.open_meteo_api_key.clone(),
            thresholds: config.weather,
            ..integrations::open_meteo::OpenMeteoConfig::default()
        },
        Arc::clone(&clock),
    ));
    let notifier: Arc<dyn Notifier> = Arc::new(NoPush);
    let ports = Ports {
        store: Arc::clone(&store),
        memory: surreal.clone(),
        beliefs: Arc::new(NoBeliefs),
        knowledge: Arc::clone(&knowledge),
        blob: Arc::clone(&blob),
        auth: Arc::new(OneUser),
        webhooks: Arc::new(MockWebhookVerifier::new()),
        weather,
        soil: Arc::new(NoSoil),
        geocoder: Arc::new(OnePlace(settings.at)),
        notifier: Arc::clone(&notifier),
        embedder: Arc::clone(&models.embedder),
        signer: Arc::new(HmacSigner::new(
            config.url_signing_key.clone(),
            config.public_base.clone(),
        )),
        photos: Arc::new(photos),
        clock: Arc::clone(&clock),
        ids: Arc::new(UlidGen),
    };
    let hub = Arc::new(EventHub::default());
    let broker = Arc::new(JobBroker::new(
        surreal.clone(),
        Arc::clone(&clock),
        surreal.jobs_queued(),
    ));
    let pusher = Pusher::new(Arc::clone(&store), notifier);
    let writers = Arc::new(ScanWriters::new(
        Arc::clone(&store),
        Arc::clone(&clock),
        Arc::clone(&hub),
        pusher.clone(),
        tasks.clone(),
        shutdown.clone(),
    ));
    let deletes = blob::DeleteQueue::new(Arc::clone(&blob));
    let pipeline = Arc::new(pipeline::Pipeline::new(pipeline::PipelineDeps {
        store: Arc::clone(&store),
        writer: writers.clone(),
        knowledge: Arc::clone(&knowledge),
        weather: Arc::clone(&ports.weather),
        memory: Arc::clone(&ports.memory),
        beliefs: Arc::clone(&ports.beliefs),
        embedder: Arc::clone(&ports.embedder),
        blobs: deletes.queued_store(),
        measurer: models.measurer,
        plan_author: plan_author(settings.llm_key.as_deref(), knowledge.as_ref(), shutdown).await,
        clock: Arc::clone(&clock),
        ids: Arc::clone(&ports.ids),
        config: pipeline::PipelineConfig {
            diagnosis_margin: config.diagnosis_margin,
            weather: config.weather,
            cpu_permits: Arc::new(Semaphore::new(config.measure_permits.max(1))),
            beliefs_read_timeout: agent::config::BELIEFS_READ_TIMEOUT,
        },
    }));
    let deletions = DeletionRunner::new(
        Arc::clone(&store),
        deletes.clone(),
        Arc::new(MockAuthAdmin::new()),
        pusher,
        clock,
        tasks.clone(),
        shutdown.clone(),
    );
    Arc::new(AppState {
        limits: RateLimits::new(config.rate_per_minute),
        upload_permits: Arc::new(Semaphore::new(config.upload_permits.max(1))),
        workers: WorkerTokens::new(config.worker_tokens.clone()),
        config,
        ports,
        pipeline,
        chat: Arc::new(NoChat),
        images: models.images,
        hub,
        writers,
        broker,
        deletions,
        deletes,
        connect: Arc::new(crate::connect::MockConnectVendors::new()),
        farm_sync: None,
        shutdown: shutdown.clone(),
        tasks: tasks.clone(),
    })
}

/// A worker process, killed when dropped.
struct Worker {
    cap: String,
    child: Child,
    log: PathBuf,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the `worker` binary for `cap` against the harness's API. Everything else (the
/// manifest, `ORT_*`, `VLM_*`) comes from the harness's own environment. Its log goes to a
/// file beside `SCAN_HARNESS_OUT`, or in the temporary directory.
fn spawn_worker(settings: &Settings, cap: &str, base: &str) -> Worker {
    assert!(
        settings.worker.exists(),
        "{}: no worker binary; build it with `cargo build -p worker --release`, or name one \
         with SCAN_HARNESS_WORKER",
        settings.worker.display()
    );
    let dir = settings
        .out
        .as_ref()
        .and_then(|o| o.parent().map(Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir);
    let log = dir.join(format!("scan-harness-{cap}.log"));
    let file = File::create(&log).expect("the worker's log file");
    let child = Command::new(&settings.worker)
        .env("WORKER_CAPS", cap)
        .env("WORKER_ID", worker_id(cap))
        .env("WORKER_TOKEN", worker_token(cap))
        .env("API_BASE_URL", base)
        .stdout(file.try_clone().expect("the log file"))
        .stderr(file)
        .stdin(Stdio::null())
        .spawn()
        .expect("the worker binary");
    eprintln!("worker {cap}: pid {}, log {}", child.id(), log.display());
    Worker {
        cap: cap.to_owned(),
        child,
        log,
    }
}

/// One app request through the router, as the app sends it.
async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("the router");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

fn json_request(method: &str, uri: &str, body: Option<&Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, BEARER);
    match body {
        Some(b) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string())),
        None => builder.body(Body::empty()),
    }
    .expect("a request")
}

/// The media type the upload route checks, from the file's extension.
fn media_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("heic" | "heif") => "image/heic",
        _ => "image/jpeg",
    }
}

/// `POST /scans/{id}/images`: a `multipart/form-data` body with `kind` and `file`.
fn upload_request(scan: &str, kind: &str, path: &Path) -> Request<Body> {
    const BOUNDARY: &str = "scan-harness-boundary-7f3a";
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("photo");
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\n{kind}\r\n\
             --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
             Content-Type: {}\r\n\r\n",
            media_type(path)
        )
        .as_bytes(),
    );
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri(format!("/scans/{scan}/images"))
        .header(header::AUTHORIZATION, BEARER)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .expect("an upload request")
}

fn id_of(body: &Value) -> String {
    body["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no id in {body}"))
        .to_owned()
}

/// The garden, a scan of a new plant, the photos, then `submit`. Returns the scan and garden.
async fn start_scan(app: &axum::Router, settings: &Settings) -> (ScanId, GardenId) {
    let (status, garden) = call(
        app,
        json_request(
            "POST",
            "/gardens",
            Some(&json!({
                "name": "Scan harness",
                "location": { "lat": settings.at.lat, "lon": settings.at.lon },
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "POST /gardens: {garden}");
    let garden_id = id_of(&garden);
    let mut new_plant = json!({ "name": "Harness plant" });
    if let Some(species) = &settings.species {
        new_plant["species_id"] = json!(species);
    }
    let (status, scan) = call(
        app,
        json_request(
            "POST",
            "/scans",
            Some(&json!({ "garden_id": garden_id, "new_plant": new_plant })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "POST /scans: {scan}");
    let scan_id = id_of(&scan);
    for (kind, path) in &settings.photos {
        let (status, image) = call(app, upload_request(&scan_id, kind, path)).await;
        assert!(
            status.is_success(),
            "upload of {} as {kind}: {status} {image}",
            path.display()
        );
        eprintln!(
            "uploaded {kind} {}: {} (quality passed: {})",
            path.display(),
            id_of(&image["image"]),
            image["quality"]["passed"]
        );
    }
    let (status, submitted) = call(
        app,
        json_request("POST", &format!("/scans/{scan_id}/submit"), None),
    )
    .await;
    assert!(status.is_success(), "submit: {status} {submitted}");
    (
        scan_id.parse().expect("a scan id"),
        garden_id.parse().expect("a garden id"),
    )
}

/// Whether the scan has reached the point the harness waits for.
fn at_rest(status: ScanStatus, plans: bool) -> bool {
    match status {
        ScanStatus::AwaitingAnswer | ScanStatus::Failed | ScanStatus::Planned => true,
        ScanStatus::Diagnosed => !plans,
        _ => false,
    }
}

/// Polls `GET /scans/{id}` until [`at_rest`], printing each status; the last scan body.
async fn wait_for_rest(
    app: &axum::Router,
    scan: ScanId,
    settings: &Settings,
    workers: &mut [Worker],
) -> Value {
    let started = Instant::now();
    let mut last: Option<ScanStatus> = None;
    loop {
        let (status, body) = call(app, json_request("GET", &format!("/scans/{scan}"), None)).await;
        assert_eq!(status, StatusCode::OK, "GET /scans/{scan}: {body}");
        let scan_status: ScanStatus =
            serde_json::from_value(body["status"].clone()).expect("a scan status");
        if last != Some(scan_status) {
            eprintln!(
                "{:>6.1} s  {}",
                started.elapsed().as_secs_f64(),
                body["status"]
            );
            last = Some(scan_status);
        }
        if at_rest(scan_status, settings.llm_key.is_some()) {
            return body;
        }
        for w in workers.iter_mut() {
            if let Ok(Some(exit)) = w.child.try_wait() {
                panic!(
                    "the {} worker exited ({exit}); its log is {}",
                    w.cap,
                    w.log.display()
                );
            }
        }
        assert!(
            started.elapsed() < settings.timeout,
            "the scan is still {scan_status:?} after {:?}",
            settings.timeout
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// One line per event: what a reader needs to follow the scan.
fn describe(event: &ScanEvent<Stored>) -> String {
    let body = match &event.body {
        ScanEventBody::RunStarted { reason, .. } => format!("run started ({reason:?})"),
        ScanEventBody::StatusChanged { from, to } => format!("status {from:?} -> {to:?}"),
        ScanEventBody::StageStarted { stage } => format!("{stage:?} started"),
        ScanEventBody::StageCompleted {
            stage,
            duration_ms,
            skipped,
        } => match skipped {
            Some(s) => format!("{stage:?} skipped ({}) in {duration_ms} ms", s.reason),
            None => format!("{stage:?} done in {duration_ms} ms"),
        },
        ScanEventBody::Finding { observation: o } => format!(
            "finding {:?}/{:?}: {} ({:?}, {:?}){}",
            o.stage,
            o.source,
            o.label,
            o.confidence.confidence(),
            o.knowledge_ref,
            o.region
                .as_ref()
                .map(|r| format!(" on {}", r.image_id))
                .unwrap_or_default()
        ),
        ScanEventBody::FindingRetracted { observation_id } => {
            format!("finding retracted: {observation_id}")
        }
        ScanEventBody::WeatherLoaded { weather } => format!(
            "weather: {:.0} °C, {:.0}% humidity, {} hours over 90% in 7 days",
            weather.current.temp_c, weather.current.rh_pct, weather.past_7_days.hours_rh_over_90
        ),
        ScanEventBody::CandidatesSelected { candidates } => format!(
            "candidates: {}",
            candidates
                .iter()
                .map(|c| format!("{} ({:?})", c.problem_id, c.source))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ScanEventBody::SpeciesSuggested {
            candidates,
            working_species_id,
            needs_confirmation,
        } => format!(
            "species suggested: {} (working {working_species_id:?}, confirm {needs_confirmation})",
            candidates
                .iter()
                .map(|c| format!("{} {:?}", c.name, c.score))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ScanEventBody::SpeciesConfirmed { species_id, .. } => {
            format!("species confirmed: {species_id:?}")
        }
        ScanEventBody::Question { question } => format!("question: {question:?}"),
        ScanEventBody::QuestionResolved { question_id, by } => {
            format!("question {question_id} resolved by {by:?}")
        }
        ScanEventBody::GuidedCheckDone { check } => {
            format!("guided check done: {:?} on {}", check.kind, check.image_id)
        }
        ScanEventBody::Diagnosis { diagnosis: d } => format!(
            "diagnosis {:?}{}: {}",
            d.status,
            if d.low_evidence {
                " (low evidence)"
            } else {
                ""
            },
            d.candidates
                .iter()
                .map(|c| format!(
                    "{} {} (+{} -{} ?{})",
                    c.problem_id,
                    c.score,
                    c.matched.len(),
                    c.contradicted.len(),
                    c.unsure.len()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ScanEventBody::PlanReady { plan_id, .. } => format!("plan ready: {plan_id}"),
        ScanEventBody::PlanSuperseded { new_plan_id, .. } => {
            format!("plan superseded by {new_plan_id}")
        }
        ScanEventBody::Failed { failure } => {
            format!("failed: {:?} {}", failure.code, failure.message)
        }
    };
    format!("{:>3} run {} {body}", event.seq.0, event.run.0)
}

fn describe_evidence(e: &IndicatorEvidence) -> String {
    format!(
        "{} {:?} by {:?} (weight {:?}): {}",
        e.indicator_id, e.answer, e.source, e.weight, e.reason
    )
}

/// One scan through the real models. Run it as section 5 of docs/vision/dev-roadmap.md says:
///
/// ```text
/// cargo build -p worker --release
/// MODEL_MANIFEST=… ORT_EP=cpu VLM_BASE_URL=… \
/// SCAN_HARNESS_PHOTOS='whole_plant=/p/plant.jpg,leaf_closeup=/p/leaf.jpg' \
///   cargo test -p api --release scan_harness -- --ignored --nocapture
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs a scan through the real models and workers (docs/vision/dev-roadmap.md, P8)"]
async fn scan_harness() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .try_init();
    let settings = Settings::from_env();
    let shutdown = CancellationToken::new();
    let tasks = TaskTracker::new();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", settings.port))
        .await
        .expect("a local port");
    let base = format!("http://{}", listener.local_addr().expect("the address"));
    let state = state(&settings, &base, &shutdown, &tasks).await;
    let app = router::app(Arc::clone(&state));
    let server = tokio::spawn(
        axum::serve(listener, app.clone())
            .with_graceful_shutdown(shutdown.clone().cancelled_owned())
            .into_future(),
    );
    eprintln!("api: {base}");
    let mut workers: Vec<Worker> = settings
        .spawn
        .iter()
        .map(|cap| spawn_worker(&settings, cap, &base))
        .collect();
    for cap in CAPS
        .split(',')
        .filter(|c| !settings.spawn.iter().any(|s| s == c))
    {
        eprintln!(
            "start the {cap} worker yourself: WORKER_CAPS={cap} WORKER_ID={} WORKER_TOKEN={} \
             API_BASE_URL={base}",
            worker_id(cap),
            worker_token(cap)
        );
    }

    let started = Instant::now();
    let (scan, garden) = start_scan(&app, &settings).await;
    eprintln!("scan {scan} submitted");
    let final_scan = wait_for_rest(&app, scan, &settings, &mut workers).await;
    let elapsed = started.elapsed();

    let store = &state.ports.store;
    let user = store
        .upsert_user(&auth_id(), None)
        .await
        .expect("the harness's user");
    let viewer = check_membership_of::<Viewer>(store.as_ref(), user, garden)
        .await
        .expect("a member");
    let events = store
        .events_after(viewer, scan, EventSeq(0))
        .await
        .expect("the scan's events");
    let snapshot = store.snapshot(scan).await.expect("the scan's snapshot");
    println!("\nscan {scan}, {:.1} s:", elapsed.as_secs_f64());
    for event in &events {
        println!("{}", describe(event));
    }
    println!("\nindicator evidence ({}):", snapshot.evidence.len());
    for e in &snapshot.evidence {
        println!("  {}", describe_evidence(e));
    }
    if let Some(out) = &settings.out {
        let report = json!({
            "scan": final_scan,
            "seconds": elapsed.as_secs_f64(),
            "events": events,
            "evidence": snapshot.evidence,
        });
        std::fs::write(out, serde_json::to_vec_pretty(&report).expect("the report"))
            .expect("SCAN_HARNESS_OUT");
        println!("\nreport: {}", out.display());
    }

    shutdown.cancel();
    drop(workers);
    tasks.close();
    let _ = tokio::time::timeout(Duration::from_secs(10), tasks.wait()).await;
    let _ = tokio::time::timeout(Duration::from_secs(10), server).await;
    let status: ScanStatus =
        serde_json::from_value(final_scan["status"].clone()).expect("a scan status");
    assert_ne!(
        status,
        ScanStatus::Failed,
        "the scan failed: {}",
        final_scan["failure"]
    );
}
