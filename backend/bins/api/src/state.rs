//! `AppState`: every port as an `Arc<dyn …>`, plus the pieces `api` owns (the event hub, the
//! scan writers, the job broker, the limiters, the deletion runner, shutdown).
//!
//! Handlers see it as `State<Arc<AppState>>`. Extractors are generic over the router's state `S`
//! and ask only `Arc<AppState>: FromRef<S>`, so they work on the real router (`S =
//! Arc<AppState>`, through axum's blanket `impl<T: Clone> FromRef<T> for T`) and on any test
//! router whose state contains one.

use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use domain::{
    authz::{self, CurrentUser, Membership, MinRole},
    id::GardenId,
    ports::{
        AuthVerifier, BlobStore, Clock, Geocoder, IdGen, KnowledgeStore, MemoryStore, Notifier,
        SoilProvider, SpeciesPhotos, Store, TextEmbedder, UrlSigner, WeatherProvider,
        WebhookVerifier,
    },
    url::Signer,
};

use crate::{
    broker::JobBroker,
    config::Config,
    deletion::DeletionRunner,
    error::ApiError,
    events::{hub::EventHub, writer::ScanWriters},
    limits::RateLimits,
    seams::{ChatTurns, ImageProcessing, PipelineApi},
    workers::WorkerTokens,
};

/// The domain ports, built by `wiring.rs` from the library crates.
#[derive(Clone)]
pub struct Ports {
    /// `store`: every repository (`Store` is their supertrait).
    pub store: Arc<dyn Store>,
    /// `store`: memories.
    pub memory: Arc<dyn MemoryStore>,
    /// `knowledge`: profiles, species list, chunk search.
    pub knowledge: Arc<dyn KnowledgeStore>,
    /// `blob`: `RustFS`.
    pub blob: Arc<dyn BlobStore>,
    /// `integrations`: Clerk JWKS.
    pub auth: Arc<dyn AuthVerifier>,
    /// `integrations`: Svix verification.
    pub webhooks: Arc<dyn WebhookVerifier>,
    /// `integrations`: Open-Meteo.
    pub weather: Arc<dyn WeatherProvider>,
    /// `integrations`: `SoilGrids`.
    pub soil: Arc<dyn SoilProvider>,
    /// `integrations`: the geocoder.
    pub geocoder: Arc<dyn Geocoder>,
    /// `integrations`: Expo Push.
    pub notifier: Arc<dyn Notifier>,
    /// `vision`: the text embedder, on CPU.
    pub embedder: Arc<dyn TextEmbedder>,
    /// `api`: the HMAC signer of image URLs.
    pub signer: Arc<dyn UrlSigner>,
    /// `api`: the species photo index, loaded from the knowledge base at start.
    pub photos: Arc<dyn SpeciesPhotos>,
    /// System time.
    pub clock: Arc<dyn Clock>,
    /// ULIDs.
    pub ids: Arc<dyn IdGen>,
}

/// Everything a handler, an extractor or a background task can reach.
pub struct AppState {
    /// The configuration.
    pub config: Arc<Config>,
    /// The domain ports.
    pub ports: Ports,
    /// `pipeline::Pipeline`.
    pub pipeline: Arc<dyn PipelineApi>,
    /// `agent::ChatAgent`.
    pub chat: Arc<dyn ChatTurns>,
    /// `vision::upload::UploadProcessor`.
    pub images: Arc<dyn ImageProcessing>,
    /// One broadcast channel per scan with live subscribers.
    pub hub: Arc<EventHub>,
    /// The per-scan writer actors; the pipeline's `ScanWriter`.
    pub writers: Arc<ScanWriters>,
    /// The claim long-poll's wake-up.
    pub broker: Arc<JobBroker>,
    /// Keyed rate limiters.
    pub limits: RateLimits,
    /// Bounds the upload pipelines running on `spawn_blocking`.
    pub upload_permits: Arc<Semaphore>,
    /// Worker bearer tokens.
    pub workers: WorkerTokens,
    /// Garden, plant, scan and account cascades.
    pub deletions: DeletionRunner,
    /// The one path to `RustFS` deletes ([storage 4]): detached, never cancelled, retried, and
    /// drained at shutdown for at least one full attempt. Shared with `deletions`.
    pub deletes: blob::DeleteQueue,
    /// Cancelled once on SIGTERM / Ctrl-C.
    pub shutdown: CancellationToken,
    /// Every background task and spawned follow-up; waited on at shutdown.
    pub tasks: TaskTracker,
}

/// The router's state type.
pub type SharedState = Arc<AppState>;

impl AppState {
    /// A signer for one response or one event send: it borrows the signer and photo ports from
    /// `self` for `'_`, and every URL it makes expires 15 minutes from now.
    pub fn signer(&self) -> Signer<'_> {
        Signer::new(
            &*self.ports.signer,
            &*self.ports.photos,
            self.ports.clock.now(),
        )
    }

    /// The membership check for a garden id from a body or query ([`authz::check_membership`]).
    pub async fn member<R: MinRole>(
        &self,
        user: &CurrentUser,
        garden: GardenId,
    ) -> Result<Membership<R>, ApiError> {
        Ok(authz::check_membership::<R>(&*self.ports.store, user, garden).await?)
    }

    /// Spawns a follow-up on the tracker; failures are logged, the sweep recovers them.
    pub fn spawn_follow_up(self: &Arc<Self>, follow_up: Option<crate::seams::FollowUp>) {
        if let Some(follow_up) = follow_up {
            let state = Arc::clone(self);
            self.tasks.spawn(async move {
                if let Err(error) = state.pipeline.follow_up(follow_up).await {
                    tracing::warn!(?follow_up, error = %error, "follow-up failed; the sweep retries it");
                }
            });
        }
    }
}
