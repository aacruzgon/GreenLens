//! The evaluation's bench: an embedded store with the schema, the pinned clock, the farm
//! synced from Era's simulators through `api`'s real sync, and a [`ChatAgent`] per model.
//!
//! What the bench leaves out of production: Agent Memory (`NoBeliefs`), the weather (always
//! unavailable, so the context has none; the clock is pinned to the fixture's day), object
//! storage (in memory, empty: no question has an image) and the knowledge base (the embedded
//! store has none, so `search_knowledge` finds nothing and plants have no species), and the
//! product catalogue (empty, so the dose check's rules that depend on a product name in the
//! sentence, percentages and ratios, behave unlike production's).

use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use secrecy::SecretString;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use api::{
    connect::{ConnectVendors, Exchanged, VendorError},
    farm_sync::{FarmSources, FarmSync, RunEnd, SyncConfig},
    sealing::{ConnectorKey, SealPurpose, StoredTokens},
    wiring::UlidGen,
};
use domain::{
    connectors::{
        ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus,
    },
    id::{ConnectorId, GardenId, UserId},
    ports::{
        AuthError, AuthVerifier, CellRequest, ChatModel, Clock, FarmSource, IdGen, IdGenExt as _,
        KnowledgeStore as _, NewConnector, NoBeliefs, ProviderError, Store, TextEmbedder,
        VerifiedToken, WeatherProvider,
    },
    records::weather::CellWeather,
    wire::Timestamp,
};
use integrations::{
    anthropic::{AnthropicClient, AnthropicConfig, ModelIds},
    era_sim::ERA_TIMEOUT,
    google_drive::{DriveClient, DriveConfig, DriveSource},
    http::{self, HttpConfig},
    slack::{SlackClient, SlackConfig, SlackSource},
};
use store::{SurrealStore, apply_schema_with};

use crate::meter::Meter;

/// The key the run's tokens are sealed under: the store is in memory and dropped at the end.
const KEY_HEX: &str = "4f8e2c1a9b7d6e5f0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f6071";

/// The clock, pinned to `garden.json`'s `as_of` (docs/EON section 2).
pub struct Pinned(pub Timestamp);

impl Clock for Pinned {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Signs in any bearer as one member.
pub struct SignedIn(pub domain::id::AuthId);

#[async_trait]
impl AuthVerifier for SignedIn {
    async fn verify(&self, _bearer: &str, now: Timestamp) -> Result<VerifiedToken, AuthError> {
        Ok(VerifiedToken {
            auth_id: self.0.clone(),
            display_name: None,
            expires_at: now.plus(Duration::from_hours(1)),
        })
    }
}

/// No weather: the context goes on without it.
pub struct NoWeather;

#[async_trait]
impl WeatherProvider for NoWeather {
    async fn fetch_batch(&self, _: &[CellRequest]) -> Result<Vec<CellWeather>, ProviderError> {
        Err(unavailable())
    }

    async fn fetch_cell(&self, _: &CellRequest) -> Result<CellWeather, ProviderError> {
        Err(unavailable())
    }
}

fn unavailable() -> ProviderError {
    ProviderError::Unavailable {
        provider: "open-meteo",
        source: "the evaluation has no weather".into(),
    }
}

/// A fresh embedded store with the schema.
pub async fn store() -> anyhow::Result<Arc<SurrealStore>> {
    let db = surrealdb::engine::any::connect("mem://").await?;
    apply_schema_with(&db).await?;
    Ok(Arc::new(SurrealStore::from_client(db).await?))
}

/// The text embedder from `{models_dir}/manifest.toml`, on the CPU (`ORT_DYLIB_PATH` names
/// ONNX Runtime).
pub async fn embedder(models_dir: std::path::PathBuf) -> anyhow::Result<Arc<dyn TextEmbedder>> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<Arc<dyn TextEmbedder>> {
        let runtime =
            vision::runtime::OrtRuntime::init(&vision::runtime::RuntimeConfig::cpu("era-eval"))?;
        let manifest = vision::manifest::Manifest::load(&models_dir.join("manifest.toml"))?;
        let embedder = vision::embed::ArcticEmbedder::load(
            &runtime,
            &manifest,
            vision::embed::EmbedderConfig::default(),
        )?;
        Ok(Arc::new(embedder))
    })
    .await?
}

/// The shared HTTP client, with the Era tests' timeouts.
pub fn http() -> anyhow::Result<reqwest::Client> {
    Ok(http::client(&HttpConfig::default())?)
}

/// `conversations.history` and `conversations.replies` page size: the real Slack's.
pub const SLACK_PAGE: u32 = integrations::slack::TIER_1_PAGE_LIMIT;

/// The Slack adapter, pointed at the tenant's simulator, without pacing; pages of
/// [`SLACK_PAGE`].
pub fn slack_source(client: &reqwest::Client, base: &str) -> Arc<dyn FarmSource> {
    let config = SlackConfig {
        timeout: ERA_TIMEOUT,
        history_limit: SLACK_PAGE,
        ..SlackConfig::era(base)
    };
    Arc::new(SlackSource::new(SlackClient::new(client.clone(), config)))
}

/// The Drive adapter, pointed at `ERA_GOOGLE_BASE` when it is set, with the simulators'
/// timeout.
pub fn drive_source(client: &reqwest::Client, base: Option<&str>) -> Option<Arc<dyn FarmSource>> {
    let config = DriveConfig {
        timeout: ERA_TIMEOUT,
        ..DriveConfig::era(base?)
    };
    Some(Arc::new(DriveSource::new(DriveClient::new(
        client.clone(),
        config,
    ))))
}

/// The vendors' OAuth for the sync: Era's connectors hold a minted credential, not OAuth
/// tokens, so there is never anything to refresh, and nothing else is called.
struct NoOAuth;

#[async_trait]
impl ConnectVendors for NoOAuth {
    async fn exchange(
        &self,
        kind: ConnectorKind,
        _code: &str,
        _pkce_verifier: Option<SecretString>,
        _redirect_uri: &str,
    ) -> Result<Exchanged, VendorError> {
        Err(VendorError::NotConfigured(kind))
    }

    async fn refresh(
        &self,
        _kind: ConnectorKind,
        _tokens: &StoredTokens,
        _force: bool,
    ) -> Result<Option<StoredTokens>, VendorError> {
        Ok(None)
    }

    async fn available_sources(
        &self,
        kind: ConnectorKind,
        _tokens: &StoredTokens,
    ) -> Result<Vec<ConnectorSource>, VendorError> {
        Err(VendorError::NotConfigured(kind))
    }

    async fn revoke(&self, kind: ConnectorKind, _tokens: &StoredTokens) -> Result<(), VendorError> {
        Err(VendorError::NotConfigured(kind))
    }
}

/// Creates an `era` connector over `sources` with the tenant's minted credential, sealed like
/// any token.
#[expect(
    clippy::too_many_arguments,
    reason = "one connector's every field, from the run's pieces"
)]
pub async fn connector(
    store: &dyn Store,
    ids: &dyn IdGen,
    garden: GardenId,
    owner: UserId,
    kind: ConnectorKind,
    account: ConnectorAccount,
    sources: Vec<ConnectorSource>,
    token: &str,
    now: Timestamp,
) -> anyhow::Result<ConnectorId> {
    let id: ConnectorId = ids.id();
    let key = ConnectorKey::from_hex(KEY_HEX).map_err(|e| anyhow::anyhow!("key: {e:?}"))?;
    store
        .seed_connector(
            garden,
            NewConnector {
                id,
                kind,
                status: ConnectorStatus::Active,
                endpoint: ConnectorEndpoint::Era,
                created_by: owner,
                account: Some(account),
                secret: Some(key.seal(SealPurpose::Token, id, token.as_bytes())),
                oauth: None,
                sources,
                created_at: now,
            },
        )
        .await?;
    Ok(id)
}

/// Runs the sync until every connector has caught up (a first sync of the fixture takes one
/// run per connector without pacing).
pub async fn sync(
    store: Arc<dyn Store>,
    embedder: Arc<dyn TextEmbedder>,
    clock: Arc<dyn Clock>,
    sources: FarmSources,
    connectors: usize,
) -> anyhow::Result<Vec<(ConnectorId, RunEnd)>> {
    let key = ConnectorKey::from_hex(KEY_HEX).map_err(|e| anyhow::anyhow!("key: {e:?}"))?;
    let sync = FarmSync::new(
        store,
        embedder,
        clock,
        key,
        Arc::new(NoOAuth),
        sources,
        SyncConfig::default(),
    );
    let mut ends = Vec::new();
    for _ in 0..5 {
        let run = sync
            .run_due(&CancellationToken::new())
            .await
            .map_err(|e| anyhow::anyhow!("the sync failed: {e:?}"))?;
        ends.extend(run);
        let synced = ends.iter().filter(|(_, e)| *e == RunEnd::Synced).count();
        if synced == connectors {
            return Ok(ends);
        }
        if ends
            .iter()
            .any(|(_, e)| matches!(e, RunEnd::Failed(_) | RunEnd::Internal | RunEnd::NoAdapter))
        {
            break;
        }
    }
    anyhow::bail!("the first sync didn't finish: {ends:?}")
}

/// The chat model for one candidate: `model` for chat and the plan writer, `cheap` for the
/// router's classifier, memory extraction and off-topic replies, as `api` wires them.
pub fn chat_model(
    client: &reqwest::Client,
    api_key: SecretString,
    model: &str,
    cheap: &str,
) -> Arc<Meter> {
    let models = ModelIds {
        chat: model.to_owned(),
        plan_writer: model.to_owned(),
        memory_extraction: cheap.to_owned(),
        off_topic: cheap.to_owned(),
        classifier: cheap.to_owned(),
        ..ModelIds::default()
    };
    let llm = AnthropicClient::new(
        client.clone(),
        AnthropicConfig {
            models,
            ..AnthropicConfig::new(api_key)
        },
    );
    Arc::new(Meter::new(Arc::new(agent::model::AnthropicChat::new(llm))))
}

/// The real chat agent over the bench's ports.
pub async fn chat_agent(
    store: &Arc<SurrealStore>,
    embedder: &Arc<dyn TextEmbedder>,
    clock: &Arc<dyn Clock>,
    model: Arc<dyn ChatModel>,
    tracker: TaskTracker,
) -> anyhow::Result<agent::ChatAgent> {
    let as_store: Arc<dyn Store> = store.clone();
    let knowledge = Arc::new(
        knowledge::SurrealKnowledgeStore::load(store.clone())
            .await
            .context("knowledge index")?,
    );
    let products = Arc::new(agent::safety::ProductCatalogue::new(
        &knowledge.products().await?,
    ));
    let config = agent::AgentConfig {
        // Every question runs on the pinned day; the budget is the evaluation's spend cap.
        daily_chat_tokens: u64::MAX,
        ..agent::AgentConfig::default()
    };
    Ok(agent::ChatAgent::new(
        agent::AgentDeps {
            stores: agent::AgentStores::from_store(&as_store),
            memory: store.clone(),
            beliefs: Arc::new(NoBeliefs),
            knowledge,
            classifier: Arc::new(agent::model::LlmClassifier::new(Arc::clone(&model), 256)),
            chat: model,
            embedder: Arc::clone(embedder),
            blob: Arc::new(blob::MemoryBlobStore::new(Arc::clone(clock))),
            weather: Arc::new(NoWeather),
            clock: Arc::clone(clock),
            ids: Arc::new(UlidGen),
            products,
            config,
        },
        tracker,
    ))
}

/// ULIDs, for the loader and the connectors.
pub fn ids() -> Arc<dyn IdGen> {
    Arc::new(UlidGen)
}
