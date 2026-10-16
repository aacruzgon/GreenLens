//! The only module that names concrete adapter types: it builds every port from the
//! configuration and assembles `AppState`, in the start-up order of api.md section 4, and it
//! implements `api`'s seam traits for the library crates' types.

use std::{path::Path, sync::Arc, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use futures::StreamExt;
use secrecy::ExposeSecret;
use tokio::sync::Semaphore;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use domain::{
    authz::{Editor, Membership},
    chat::Preview,
    id::{PendingActionId, PlantId, QuestionId, ScanId},
    jobs::{CompleteJob, WorkerEvent},
    ports::{
        AuthAdmin, AuthVerifier, BlobStore, ChatModel, Clock, CompletionOutcome, Geocoder, IdGen,
        KnowledgeRepo, KnowledgeStore, Lease, LeasedJob, MemoryStore, Notifier, PhotoAnswerStart,
        PlanWrite, QuestionStepClaim, SoilProvider, Store, TextClassifier, TextEmbedder,
        WeatherProvider, WebhookVerifier,
    },
    records::plan::ActionPlan,
    scan::{AnswerRequest, Plant, Scan, SpeciesChoice},
    url::Stored,
    wire::Timestamp,
};
use integrations::{
    anthropic::{AnthropicClient, AnthropicConfig, ModelIds},
    clerk::{ClerkAdmin, ClerkAdminConfig, ClerkVerifier, ClerkVerifierConfig},
    expo_push::{ExpoPush, ExpoPushConfig},
    geocode::{CompositeGeocoder, GeocoderConfig, TimeZoneFinder},
    http::{HttpConfig, client},
    jev::{JevClassifier, JevConfig},
    open_meteo::{OpenMeteo, OpenMeteoConfig},
    soilgrids::{SoilGrids, SoilGridsConfig},
    svix::SvixVerifier,
};

use crate::{
    broker::JobBroker,
    config::Config,
    deletion::DeletionRunner,
    events::{hub::EventHub, writer::ScanWriters},
    limits::RateLimits,
    push::Pusher,
    seams::{
        ChatTurns, FollowUp, Handled, ImageProcessing, PhotoInfo, PipelineApi, PipelineError,
        PreviewError, ProcessedImage, ProcessedUpload, RevisedPlan, SubmitOutcome, TurnError,
        TurnParts, TurnRequest, UploadError, UploadInput,
    },
    signing::{HmacSigner, PhotoIndex},
    state::{AppState, Ports},
    workers::WorkerTokens,
};

/// The system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        chrono::Utc::now().into()
    }
}

/// ULIDs from `ulid` 3 (`Ulid::generate`).
pub struct UlidGen;

impl IdGen for UlidGen {
    fn ulid(&self) -> ulid::Ulid {
        ulid::Ulid::generate()
    }
}

// ---------------------------------------------------------------------------------------------
// The seams, implemented for the library crates' types
// ---------------------------------------------------------------------------------------------

#[async_trait]
impl PipelineApi for pipeline::Pipeline {
    async fn follow_up(&self, follow_up: FollowUp) -> Result<(), PipelineError> {
        pipeline::Pipeline::follow_up(self, follow_up).await
    }
    async fn submit(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
    ) -> Result<SubmitOutcome, PipelineError> {
        pipeline::Pipeline::submit(self, editor, scan).await
    }
    async fn species_confirmed(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<Plant<Stored>, PipelineError> {
        pipeline::Pipeline::species_confirmed(self, editor, plant, choice).await
    }
    async fn answer(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        answer: AnswerRequest,
    ) -> Result<Handled<Scan<Stored>>, PipelineError> {
        pipeline::Pipeline::answer(self, editor, scan, answer).await
    }
    async fn photo_answer_start(
        &self,
        scan: ScanId,
        question: QuestionId,
        photo: PhotoInfo,
    ) -> Result<PhotoAnswerStart, PipelineError> {
        pipeline::Pipeline::photo_answer_start(self, scan, question, photo).await
    }
    fn photo_answer_added(&self, claim: QuestionStepClaim) -> Option<FollowUp> {
        pipeline::Pipeline::photo_answer_added(self, claim)
    }
    async fn worker_events(
        &self,
        lease: &Lease,
        job: LeasedJob,
        events: Vec<WorkerEvent>,
    ) -> Result<(), PipelineError> {
        pipeline::Pipeline::worker_events(self, lease, job, events).await
    }
    async fn job_completed(
        &self,
        lease: &Lease,
        job: LeasedJob,
        complete: CompleteJob,
    ) -> Result<Handled<CompletionOutcome>, PipelineError> {
        pipeline::Pipeline::job_completed(self, lease, job, complete).await
    }
    async fn job_failed_for_good(&self, job: LeasedJob) -> Result<Option<FollowUp>, PipelineError> {
        pipeline::Pipeline::job_failed_for_good(self, job).await
    }
    async fn sweep(&self) -> Result<Vec<FollowUp>, PipelineError> {
        pipeline::Pipeline::sweep(self).await
    }
    async fn store_revision(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        write: PlanWrite,
    ) -> Result<ActionPlan, PipelineError> {
        pipeline::Pipeline::store_revision(self, editor, scan, write).await
    }
}

#[async_trait]
impl ChatTurns for agent::ChatAgent {
    async fn start_turn(&self, request: TurnRequest) -> Result<TurnParts, TurnError> {
        let (events, outcome) = agent::ChatAgent::start_turn(self, request)
            .await?
            .into_parts();
        Ok(TurnParts {
            events: events.boxed(),
            outcome,
        })
    }

    fn apply_preview(
        &self,
        base: &ActionPlan,
        preview: &Preview,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<RevisedPlan, PreviewError> {
        agent::tools::revise::apply_preview(base, preview, action, now)
    }
}

impl ImageProcessing for vision::upload::UploadProcessor {
    fn process(&self, input: UploadInput<'_>) -> Result<ProcessedUpload, UploadError> {
        vision::upload::UploadProcessor::process(self, input)
    }
    fn process_chat(&self, file: &[u8]) -> Result<ProcessedImage, UploadError> {
        vision::upload::UploadProcessor::process_chat(self, file)
    }
}

// ---------------------------------------------------------------------------------------------
// Start-up
// ---------------------------------------------------------------------------------------------

/// The CPU models `api` loads: the text embedder, the upload pipeline (face detector) and the
/// measurer, from `{MODELS_DIR}/manifest.toml`, every file's hash checked.
struct Models {
    /// The text embedder, shared by `api` (garden notes), the pipeline (memory summaries) and
    /// the agent (searches and memories); each call names its `EmbedMode`.
    embedder: Arc<dyn TextEmbedder>,
    images: Arc<dyn ImageProcessing>,
    measurer: Arc<dyn pipeline::ImageMeasurer>,
}

/// Loads the ONNX sessions on a blocking thread: they take seconds.
async fn load_models(models_dir: &str) -> anyhow::Result<Models> {
    let manifest_path = Path::new(models_dir).join("manifest.toml");
    tokio::task::spawn_blocking(move || -> anyhow::Result<Models> {
        let runtime =
            vision::runtime::OrtRuntime::init(&vision::runtime::RuntimeConfig::cpu("api"))?;
        let manifest = vision::manifest::Manifest::load(&manifest_path)?;
        let embedder = vision::embed::ArcticEmbedder::load(
            &runtime,
            &manifest,
            vision::embed::EmbedderConfig::default(),
        )?;
        let images = vision::upload::UploadProcessor::load(
            &runtime,
            &manifest,
            vision::upload::UploadConfig::default(),
        )?;
        let spots = Arc::new(vision::measure::spots::ColourSpotSegmenter::default());
        let measurer =
            vision::measure::Measurer::new(vision::measure::MeasurementConfig::default(), spots);
        Ok(Models {
            embedder: Arc::new(embedder),
            images: Arc::new(images),
            measurer: Arc::new(pipeline::measure::VisionMeasurer::new(Arc::new(measurer))),
        })
    })
    .await?
}

/// The database side: the schema applied as root, then the `api` user's connection, the
/// repositories, memories and the knowledge index.
struct Databases {
    store: Arc<dyn Store>,
    memory: Arc<dyn MemoryStore>,
    knowledge_repo: Arc<dyn KnowledgeRepo>,
    knowledge: Arc<dyn KnowledgeStore>,
    jobs_queued: Arc<tokio::sync::Notify>,
}

async fn connect_databases(config: &Config) -> anyhow::Result<Databases> {
    let db = &config.database;
    store::SurrealStore::apply_schema(&db.url, &db.root_user, db.root_password.expose_secret())
        .await
        .context("apply schema.surql")?;
    let surreal = Arc::new(
        store::SurrealStore::connect(&store::StoreConfig {
            url: db.url.clone(),
            username: db.user.clone(),
            password: db.password.expose_secret().to_owned(),
        })
        .await
        .context("SurrealDB")?,
    );
    // The store signals this after every write that queues or requeues a job commits; the
    // broker's claim long poll waits on it, and nothing else wakes it.
    let jobs_queued = surreal.jobs_queued();
    let knowledge_repo: Arc<dyn KnowledgeRepo> = surreal.clone();
    let knowledge: Arc<dyn KnowledgeStore> = Arc::new(
        knowledge::SurrealKnowledgeStore::load(Arc::clone(&knowledge_repo))
            .await
            .context("knowledge base")?,
    );
    Ok(Databases {
        store: surreal.clone(),
        memory: surreal,
        knowledge_repo,
        knowledge,
        jobs_queued,
    })
}

/// The outbound adapters, over one shared HTTP client.
struct Outbound {
    auth: Arc<dyn AuthVerifier>,
    webhooks: Arc<dyn WebhookVerifier>,
    auth_admin: Arc<dyn AuthAdmin>,
    weather: Arc<dyn WeatherProvider>,
    soil: Arc<dyn SoilProvider>,
    geocoder: Arc<dyn Geocoder>,
    notifier: Arc<dyn Notifier>,
    chat_model: Arc<dyn ChatModel>,
    classifier: Arc<dyn TextClassifier>,
}

async fn outbound(config: &Config, clock: &Arc<dyn Clock>) -> anyhow::Result<Outbound> {
    let http = client(&HttpConfig::default()).context("HTTP client")?;
    let mut verifier = ClerkVerifierConfig::new(config.clerk.issuer.clone());
    verifier.allowed_azp.clone_from(&config.web_origins);
    // The time-zone boundaries are a few megabytes to parse: off the async threads.
    let zones = Arc::new(tokio::task::spawn_blocking(TimeZoneFinder::default).await?);
    let defaults = ModelIds::default();
    let strong = config
        .providers
        .llm_chat_model
        .clone()
        .unwrap_or_else(|| defaults.chat.clone());
    let cheap = config
        .providers
        .llm_cheap_model
        .clone()
        .unwrap_or_else(|| defaults.memory_extraction.clone());
    let models = ModelIds {
        chat: strong.clone(),
        plan_writer: strong,
        memory_extraction: cheap.clone(),
        off_topic: cheap.clone(),
        classifier: cheap,
        ..defaults
    };
    let llm = AnthropicClient::new(
        http.clone(),
        AnthropicConfig {
            models,
            ..AnthropicConfig::new(config.providers.llm_api_key.clone())
        },
    );
    let chat_model: Arc<dyn ChatModel> = Arc::new(agent::model::AnthropicChat::new(llm));
    let classifier: Arc<dyn TextClassifier> = match &config.providers.jev {
        Some((base_url, api_key)) => Arc::new(JevClassifier::new(
            http.clone(),
            JevConfig {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                timeout: Duration::from_secs(2),
            },
        )),
        None => Arc::new(agent::model::LlmClassifier::new(
            Arc::clone(&chat_model),
            256,
        )),
    };
    Ok(Outbound {
        auth: Arc::new(ClerkVerifier::new(http.clone(), verifier)),
        webhooks: Arc::new(
            SvixVerifier::new(&config.clerk.webhook_secret).context("CLERK_WEBHOOK_SECRET")?,
        ),
        auth_admin: Arc::new(ClerkAdmin::new(
            http.clone(),
            ClerkAdminConfig {
                base_url: "https://api.clerk.com/v1".to_owned(),
                secret_key: config.clerk.secret_key.clone(),
                timeout: Duration::from_secs(10),
            },
        )),
        weather: Arc::new(OpenMeteo::new(
            http.clone(),
            OpenMeteoConfig {
                api_key: config.providers.open_meteo_api_key.clone(),
                thresholds: config.weather,
                ..OpenMeteoConfig::default()
            },
            Arc::clone(clock),
        )),
        soil: Arc::new(SoilGrids::new(
            http.clone(),
            SoilGridsConfig::default(),
            Arc::clone(clock),
        )),
        geocoder: Arc::new(CompositeGeocoder::new(
            http.clone(),
            GeocoderConfig::default(),
            zones,
        )),
        notifier: Arc::new(ExpoPush::new(
            http,
            ExpoPushConfig {
                access_token: config.providers.expo_access_token.clone(),
                ..ExpoPushConfig::default()
            },
        )),
        chat_model,
        classifier,
    })
}

/// The object store the config points at.
fn blob_store(config: &Config) -> Arc<dyn BlobStore> {
    Arc::new(blob::S3BlobStore::new(&blob::BlobConfig {
        endpoint: config.storage.endpoint.clone(),
        access_key: config.storage.access_key.clone(),
        secret_key: config.storage.secret_key.expose_secret().to_owned(),
        bucket: config.storage.bucket.clone(),
        region: "us-east-1".to_owned(),
    }))
}

/// The analysis pipeline over the ports, with its plan author on the chat model.
fn pipeline_api(
    config: &Config,
    ports: &Ports,
    writers: &Arc<ScanWriters>,
    measurer: Arc<dyn pipeline::ImageMeasurer>,
    chat_model: &Arc<dyn ChatModel>,
    products: &Arc<agent::safety::ProductCatalogue>,
) -> Arc<dyn PipelineApi> {
    Arc::new(pipeline::Pipeline::new(pipeline::PipelineDeps {
        store: Arc::clone(&ports.store),
        writer: writers.clone(),
        knowledge: Arc::clone(&ports.knowledge),
        weather: Arc::clone(&ports.weather),
        memory: Arc::clone(&ports.memory),
        embedder: Arc::clone(&ports.embedder),
        blobs: Arc::clone(&ports.blob),
        measurer,
        plan_author: Arc::new(agent::PlanWriter::new(
            Arc::clone(chat_model),
            Arc::clone(products),
        )),
        clock: Arc::clone(&ports.clock),
        ids: Arc::clone(&ports.ids),
        config: pipeline::PipelineConfig {
            diagnosis_margin: config.diagnosis_margin,
            weather: config.weather,
            cpu_permits: Arc::new(Semaphore::new(config.measure_permits.max(1))),
        },
    }))
}

/// The chat agent over the ports, with its chat model, classifier and product catalogue.
fn chat_turns(
    ports: &Ports,
    chat: Arc<dyn ChatModel>,
    classifier: Arc<dyn TextClassifier>,
    products: Arc<agent::safety::ProductCatalogue>,
    tasks: TaskTracker,
) -> Arc<dyn ChatTurns> {
    Arc::new(agent::ChatAgent::new(
        agent::AgentDeps {
            stores: agent::AgentStores::from_store(&ports.store),
            memory: Arc::clone(&ports.memory),
            knowledge: Arc::clone(&ports.knowledge),
            chat,
            classifier,
            embedder: Arc::clone(&ports.embedder),
            blob: Arc::clone(&ports.blob),
            weather: Arc::clone(&ports.weather),
            clock: Arc::clone(&ports.clock),
            ids: Arc::clone(&ports.ids),
            products,
            config: agent::AgentConfig::default(),
        },
        tasks,
    ))
}

/// Builds every port and the state, in order: the databases (schema as root, then connect as
/// `api`; the knowledge index), the object store, the outbound adapters, the CPU models, the
/// species photo index, then `api`'s own pieces and the pipeline and agent that need the writer.
pub async fn build(
    config: Config,
    shutdown: CancellationToken,
    tasks: TaskTracker,
) -> anyhow::Result<Arc<AppState>> {
    let config = Arc::new(config);
    let dbs = connect_databases(&config).await?;
    let blob = blob_store(&config);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Arc<dyn IdGen> = Arc::new(UlidGen);
    let out = outbound(&config, &clock).await?;
    let models = load_models(&config.models_dir).await.context("models")?;
    let snapshot = dbs
        .knowledge_repo
        .load_knowledge()
        .await
        .context("species photos")?;
    let products = Arc::new(agent::safety::ProductCatalogue::new(
        &dbs.knowledge.products().await?,
    ));
    let store = dbs.store;
    let ports = Ports {
        store: Arc::clone(&store),
        memory: dbs.memory,
        knowledge: dbs.knowledge,
        blob: Arc::clone(&blob),
        auth: out.auth,
        webhooks: out.webhooks,
        weather: out.weather,
        soil: out.soil,
        geocoder: out.geocoder,
        notifier: Arc::clone(&out.notifier),
        embedder: models.embedder,
        signer: Arc::new(HmacSigner::new(
            config.url_signing_key.clone(),
            config.public_base.clone(),
        )),
        photos: Arc::new(PhotoIndex::new(snapshot.species_photos)),
        clock: Arc::clone(&clock),
        ids,
    };
    let hub = Arc::new(EventHub::default());
    let broker = Arc::new(JobBroker::new(
        store.clone(),
        Arc::clone(&clock),
        dbs.jobs_queued,
    ));
    let pusher = Pusher::new(Arc::clone(&store), out.notifier);
    let writers = Arc::new(ScanWriters::new(
        Arc::clone(&store),
        Arc::clone(&clock),
        Arc::clone(&hub),
        pusher.clone(),
        tasks.clone(),
        shutdown.clone(),
    ));
    let pipeline = pipeline_api(
        &config,
        &ports,
        &writers,
        models.measurer,
        &out.chat_model,
        &products,
    );
    let chat = chat_turns(
        &ports,
        out.chat_model,
        out.classifier,
        products,
        tasks.clone(),
    );
    // The one delete queue: every RustFS delete in `api` goes through it, and `main` drains it.
    let deletes = blob::DeleteQueue::new(Arc::clone(&blob));
    let deletions = DeletionRunner::new(
        store,
        deletes.clone(),
        out.auth_admin,
        pusher,
        clock,
        tasks.clone(),
        shutdown.clone(),
    );
    Ok(Arc::new(AppState {
        limits: RateLimits::new(config.rate_per_minute),
        upload_permits: Arc::new(Semaphore::new(config.upload_permits.max(1))),
        workers: WorkerTokens::new(config.worker_tokens.clone()),
        config,
        ports,
        pipeline,
        chat,
        images: models.images,
        hub,
        writers,
        broker,
        deletions,
        deletes,
        shutdown,
        tasks,
    }))
}
