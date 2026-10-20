//! The only module that names concrete adapter types: it builds every port from the
//! configuration and assembles `AppState`, in the start-up order of api.md section 4, and it
//! implements `api`'s seam traits for the library crates' types.

mod connect;

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
    connectors::ConnectorEndpoint,
    id::{PendingActionId, PlantId, QuestionId, ScanId},
    jobs::{CompleteJob, WorkerEvent},
    ports::{
        AuthAdmin, AuthVerifier, BeliefStore, BlobStore, ChatModel, Clock, CompletionOutcome,
        Geocoder, IdGen, KnowledgeRepo, KnowledgeStore, Lease, LeasedJob, MemoryStore, NoBeliefs,
        Notifier, PhotoAnswerStart, PlanWrite, QuestionStepClaim, SoilProvider, Store,
        TextClassifier, TextEmbedder, WeatherProvider, WebhookVerifier,
    },
    records::plan::ActionPlan,
    scan::{AnswerRequest, Plant, Scan, SpeciesChoice},
    url::Stored,
    wire::Timestamp,
};
use integrations::{
    agent_memory::{AgentMemory, AgentMemoryConfig},
    anthropic::{AnthropicClient, AnthropicConfig, ModelIds},
    clerk::{ClerkAdmin, ClerkAdminConfig, ClerkVerifier, ClerkVerifierConfig},
    expo_push::{ExpoPush, ExpoPushConfig},
    geocode::{CompositeGeocoder, GeocoderConfig, TimeZoneFinder},
    google_drive::{DriveClient, DriveConfig, DriveSource},
    http::{HttpConfig, client},
    jev::{JevClassifier, JevConfig},
    open_meteo::{OpenMeteo, OpenMeteoConfig},
    slack::{SlackClient, SlackConfig, SlackSource},
    slc::SoilLandscapes,
    soilgrids::{SoilGrids, SoilGridsConfig},
    svix::SvixVerifier,
};

use crate::{
    broker::JobBroker,
    config::Config,
    connect::ConnectVendors,
    deletion::DeletionRunner,
    events::{hub::EventHub, writer::ScanWriters},
    farm_sync::{FarmSources, FarmSync, SyncConfig},
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

/// The database side: the schema and the migrations applied as root, then the `api` user's
/// connection, the repositories, memories, the knowledge index and `knowledge_meta`.
struct Databases {
    store: Arc<dyn Store>,
    memory: Arc<dyn MemoryStore>,
    knowledge_repo: Arc<dyn KnowledgeRepo>,
    knowledge: Arc<dyn KnowledgeStore>,
    jobs_queued: Arc<tokio::sync::Notify>,
    /// `knowledge_meta:current`, `None` before a knowledge file was applied.
    knowledge_meta: Option<store::migrate::KnowledgeMeta>,
}

async fn connect_databases(config: &Config) -> anyhow::Result<Databases> {
    let db = &config.database;
    let applied =
        store::SurrealStore::migrate(&db.url, &db.root_user, db.root_password.expose_secret())
            .await
            .context("apply the migrations")?;
    tracing::info!(
        numbered = ?applied.numbered,
        knowledge = applied.knowledge,
        "migrations applied"
    );
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
    let knowledge_meta = surreal
        .knowledge_meta()
        .await
        .context("read knowledge_meta")?;
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
        knowledge_meta,
    })
}

/// Stops the start when the loaded knowledge set's chunks were embedded by another model than
/// `embedder`, since its query vectors would not match them (docs/database section 1,
/// changing the text-embedding model). Before any knowledge set is loaded, it passes.
fn check_embedder(
    meta: Option<&store::migrate::KnowledgeMeta>,
    embedder: &dyn TextEmbedder,
) -> anyhow::Result<()> {
    let Some(meta) = meta else {
        return Ok(());
    };
    let model = embedder.model();
    anyhow::ensure!(
        meta.matches_embedder(&model.name, &model.version, model.dimension),
        "the knowledge set was embedded with {} ({} dimensions), but this api loaded {}@{} ({} \
         dimensions): regenerate knowledge.surql with this model, or pin the knowledge set's",
        meta.embedding_model,
        meta.embedding_dimension,
        model.name,
        model.version,
        model.dimension,
    );
    Ok(())
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
    beliefs: Arc<dyn BeliefStore>,
    connect: Arc<dyn ConnectVendors>,
    farm: FarmSources,
}

/// The sync when connectors are on (decision 9 of docs/EON): the `connectors` task and
/// start-up seeding run only with it. It renews Google's tokens through `vendors`, the same
/// instance as the routes, on Google's own token endpoint.
fn farm_sync(
    config: &Config,
    ports: &Ports,
    vendors: Arc<dyn ConnectVendors>,
    sources: FarmSources,
) -> Option<Arc<FarmSync>> {
    config.connectors.key.clone().map(|key| {
        Arc::new(FarmSync::new(
            Arc::clone(&ports.store),
            Arc::clone(&ports.embedder),
            Arc::clone(&ports.clock),
            key,
            vendors,
            sources,
            SyncConfig::default(),
        ))
    })
}

/// The farm connectors' adapters: the real Slack, with Tier 1 pacing, and Google Drive for
/// owners' connectors; Era's simulators, without pacing or back-off, for the seeded `era` ones
/// when `ERA_SLACK_BASE` and `ERA_GOOGLE_BASE` are set.
fn farm_sources(
    config: &Config,
    slack_client: impl Fn(SlackConfig) -> SlackClient,
    drive_client: impl Fn(DriveConfig) -> DriveClient,
) -> FarmSources {
    let slack = |c: SlackConfig| Arc::new(SlackSource::new(slack_client(c)));
    let drive = |c: DriveConfig| Arc::new(DriveSource::new(drive_client(c)));
    let mut sources = FarmSources::default()
        .with(ConnectorEndpoint::Vendor, slack(SlackConfig::default()))
        .with(ConnectorEndpoint::Vendor, drive(DriveConfig::default()));
    if let Some(base) = &config.connectors.era_slack_base {
        sources = sources.with(
            ConnectorEndpoint::Era,
            slack(SlackConfig::era(base.clone())),
        );
    }
    if let Some(base) = &config.connectors.era_google_base {
        sources = sources.with(
            ConnectorEndpoint::Era,
            drive(DriveConfig::era(base.clone())),
        );
    }
    sources
}

#[expect(
    clippy::too_many_lines,
    reason = "every outbound adapter is built here, one after another, over one HTTP client"
)]
async fn outbound(config: &Config, clock: &Arc<dyn Clock>) -> anyhow::Result<Outbound> {
    let http = client(&HttpConfig::default()).context("HTTP client")?;
    let mut verifier = ClerkVerifierConfig::new(config.clerk.issuer.clone());
    verifier.allowed_azp.clone_from(&config.web_origins);
    // The time-zone boundaries are a few megabytes to parse: off the async threads.
    let zones = Arc::new(tokio::task::spawn_blocking(TimeZoneFinder::default).await?);
    // Soil Landscapes of Canada, for the Canadian points where SoilGrids has no data.
    let landscapes = Arc::new(SoilLandscapes::embedded().context("Soil Landscapes of Canada")?);
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
    let connect: Arc<dyn ConnectVendors> = Arc::new(connect::VendorClients::new(
        |c| SlackClient::new(http.clone(), c),
        |c| DriveClient::new(http.clone(), c),
        &config.connectors,
        Arc::clone(clock),
    ));
    let farm = farm_sources(
        config,
        |c| SlackClient::new(http.clone(), c),
        |c| DriveClient::new(http.clone(), c),
    );
    let beliefs: Arc<dyn BeliefStore> = match agent_memory(config) {
        Some(c) => Arc::new(AgentMemory::new(http.clone(), c)),
        None => Arc::new(NoBeliefs),
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
        soil: Arc::new(
            SoilGrids::new(http.clone(), SoilGridsConfig::default(), Arc::clone(clock))
                .with_local_fallback(landscapes),
        ),
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
        beliefs,
        connect,
        farm,
    })
}

/// Agent Memory's settings when it is configured, logging whether it is on; the key is never
/// logged.
fn agent_memory(config: &Config) -> Option<AgentMemoryConfig> {
    let Some(m) = &config.providers.agent_memory else {
        tracing::info!("agent memory off: AGENT_MEMORY_URL is not set");
        return None;
    };
    tracing::info!(url = %m.url, context = %m.context_id, "agent memory on");
    Some(AgentMemoryConfig::new(
        m.url.clone(),
        m.context_id.clone(),
        m.api_key.clone(),
    ))
}

/// Logs whether farm connectors are on, which vendors and simulators are configured, and a
/// warning when vendor or Era settings are set without `CONNECTOR_KEY`, which turns them off.
/// Neither the key nor a client secret is logged.
fn log_connectors(config: &Config) {
    let c = &config.connectors;
    let (slack, google) = (c.slack.is_some(), c.google.is_some());
    let era = c.era_slack_base.is_some() || c.era_google_base.is_some();
    if c.key.is_none() {
        if slack || google || era {
            tracing::warn!(
                slack,
                google,
                era,
                "farm connectors off: CONNECTOR_KEY is not set, so the other connector settings are unused"
            );
        } else {
            tracing::info!("farm connectors off: CONNECTOR_KEY is not set");
        }
        return;
    }
    tracing::info!(
        slack = c.slack.as_ref().map(|s| s.client_id.as_str()),
        google = c.google.as_ref().map(|g| g.client_id.as_str()),
        return_url = %c.return_url,
        era_slack = c.era_slack_base.is_some(),
        era_google = c.era_google_base.is_some(),
        "farm connectors on"
    );
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
        beliefs: Arc::clone(&ports.beliefs),
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
            beliefs_read_timeout: agent::config::BELIEFS_READ_TIMEOUT,
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
            beliefs: Arc::clone(&ports.beliefs),
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

/// Builds every port and the state, in order: the databases (schema and migrations as root,
/// then connect as `api`; the knowledge index), the object store, the outbound adapters, the
/// CPU models and their check against `knowledge_meta`, the species photo index, then `api`'s own pieces and the pipeline and agent that need the writer.
#[expect(
    clippy::too_many_lines,
    reason = "every port and in-process piece is built here, in start-up order"
)]
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
    log_connectors(&config);
    let models = load_models(&config.models_dir).await.context("models")?;
    check_embedder(dbs.knowledge_meta.as_ref(), models.embedder.as_ref())?;
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
        beliefs: Arc::clone(&out.beliefs),
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
    let farm_sync = farm_sync(&config, &ports, Arc::clone(&out.connect), out.farm);
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
    let mut deletions = DeletionRunner::new(
        store,
        deletes.clone(),
        out.auth_admin,
        pusher,
        clock,
        tasks.clone(),
        shutdown.clone(),
    );
    // Connectors on: the deletion runner revokes through the routes' own vendor seam.
    if let Some(key) = &config.connectors.key {
        deletions = deletions.with_revoke(key.clone(), Arc::clone(&out.connect));
    }
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
        connect: out.connect,
        farm_sync,
        shutdown,
        tasks,
    }))
}

#[cfg(test)]
mod embedder_check_tests {
    use domain::ports::{EmbedMode, Embedding, EmbeddingModel, ModelError};

    use super::*;

    struct Named(&'static str, usize);

    #[async_trait]
    impl TextEmbedder for Named {
        fn model(&self) -> EmbeddingModel {
            EmbeddingModel {
                name: "snowflake-arctic-embed-xs".into(),
                version: self.0.into(),
                dimension: self.1,
            }
        }

        async fn embed(
            &self,
            _mode: EmbedMode,
            _texts: &[String],
        ) -> Result<Vec<Embedding>, ModelError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn the_embedder_must_match_the_knowledge_set() {
        let meta = store::migrate::KnowledgeMeta {
            label_set_version: "f4fd4e3401ecb0f0".into(),
            classifier_labels_version: "v1".into(),
            embedding_model: "snowflake-arctic-embed-xs@d8c86521".into(),
            embedding_dimension: 384,
        };
        assert!(
            check_embedder(None, &Named("other", 768)).is_ok(),
            "no knowledge set yet"
        );
        assert!(check_embedder(Some(&meta), &Named("d8c86521", 384)).is_ok());
        let err = check_embedder(Some(&meta), &Named("other", 384)).unwrap_err();
        assert!(
            err.to_string().contains("snowflake-arctic-embed-xs@other"),
            "{err}"
        );
        assert!(check_embedder(Some(&meta), &Named("d8c86521", 768)).is_err());
    }
}
