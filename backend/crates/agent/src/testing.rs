//! Test doubles for the agent's unit tests. `domain`'s `ScriptedChat` replays one script for
//! every call; the tool loop and the plan writer's retry need a different answer per call, so
//! [`ScriptedModel`] pops one script per call and records every request it was sent.

use std::{collections::VecDeque, sync::Mutex};

use domain::{
    chat::Usage,
    ports::{
        ChatChunk, ChatCompletion, ChatModel, ChatPurpose, ChatRequest, ModelError, StopReason,
    },
};
use futures::stream::{self, BoxStream, StreamExt as _};

/// A chat model with one scripted answer per call.
#[derive(Debug, Default)]
pub struct ScriptedModel {
    streams: Mutex<VecDeque<Vec<ChatChunk>>>,
    completions: Mutex<VecDeque<ChatCompletion>>,
    /// Every request, in call order.
    pub requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedModel {
    /// A model whose `stream` calls answer with these scripts, in order; an empty script fails
    /// with `ModelError::Unavailable`.
    pub fn streaming(scripts: Vec<Vec<ChatChunk>>) -> Self {
        Self {
            streams: Mutex::new(scripts.into()),
            ..Self::default()
        }
    }

    /// A model whose one `complete` call answers with `text`.
    pub fn completing(text: &str, stop: StopReason) -> Self {
        Self::completions(vec![(text.to_owned(), stop)])
    }

    /// A model whose `complete` calls answer in order.
    pub fn completions(answers: Vec<(String, StopReason)>) -> Self {
        Self {
            completions: Mutex::new(
                answers
                    .into_iter()
                    .map(|(text, stop)| ChatCompletion {
                        text,
                        tool_calls: Vec::new(),
                        usage: Usage {
                            tokens_in: 100,
                            tokens_out: 20,
                        },
                        stop,
                    })
                    .collect(),
            ),
            ..Self::default()
        }
    }

    /// The requests so far.
    pub fn sent(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ChatModel for ScriptedModel {
    fn model_name(&self, _: ChatPurpose) -> String {
        "scripted".to_owned()
    }

    fn stream<'a>(
        &'a self,
        request: &'a ChatRequest,
    ) -> BoxStream<'a, Result<ChatChunk, ModelError>> {
        self.requests.lock().unwrap().push(request.clone());
        let script = self
            .streams
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| vec![ChatChunk::Done(StopReason::EndTurn)]);
        if script.is_empty() {
            // An explicitly empty script stands for a provider failure.
            return stream::iter([Err(ModelError::Unavailable {
                model: "scripted",
                source: "overloaded".into(),
            })])
            .boxed();
        }
        stream::iter(script.into_iter().map(Ok)).boxed()
    }

    async fn complete(&self, request: &ChatRequest) -> Result<ChatCompletion, ModelError> {
        self.requests.lock().unwrap().push(request.clone());
        self.completions
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ModelError::Unavailable {
                model: "scripted",
                source: "no scripted completion left".into(),
            })
    }
}

/// The ids the shared harness uses.
pub const USER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E50";
/// The garden of [`owner`].
pub const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E51";
/// The conversation of [`owner`].
pub const CONVERSATION: &str = "01JAX3K9T6R2B8V0QZ4M7D1E52";

/// Now, for the shared harness.
pub fn now() -> domain::wire::Timestamp {
    "2026-10-16T12:00:00Z".parse().expect("a valid timestamp")
}

/// A session proof for [`CONVERSATION`] in [`GARDEN`], built through the real checks over
/// mocked ports.
pub async fn owner(
    role: domain::wire::Role,
) -> domain::authz::ConversationOwner<domain::authz::Viewer> {
    use domain::ports::{
        ConversationOwnerRow, MockAccessRepo, MockAuthVerifier, MockIdentityRepo, VerifiedToken,
    };
    let mut verifier = MockAuthVerifier::new();
    verifier.expect_verify().returning(|_, _| {
        Ok(VerifiedToken {
            auth_id: "user_test".parse().expect("a valid auth id"),
            display_name: None,
            expires_at: now().plus(std::time::Duration::from_secs(60)),
        })
    });
    let mut identity = MockIdentityRepo::new();
    identity
        .expect_upsert_user()
        .returning(|_, _| Ok(USER.parse().expect("a valid id")));
    let user = domain::authz::authenticate(&verifier, &identity, "token", now())
        .await
        .expect("the mocked token verifies");
    let mut access = MockAccessRepo::new();
    access.expect_conversation_owner().returning(|_| {
        Ok(Some(ConversationOwnerRow {
            user: USER.parse().expect("a valid id"),
            garden: GARDEN.parse().expect("a valid id"),
        }))
    });
    access
        .expect_role_in()
        .returning(move |_, _| Ok(Some(role)));
    domain::authz::check_conversation(&access, &user, CONVERSATION.parse().expect("a valid id"))
        .await
        .expect("the mocked member owns the conversation")
}

/// Deps for a garden-anchored turn over mocks: an empty garden, no tasks, no map, no weather,
/// a 4-dimension embedder. `memory` and `beliefs` are the test's.
pub fn garden_deps(
    memory: domain::ports::MockMemoryStore,
    beliefs: std::sync::Arc<dyn domain::ports::BeliefStore>,
) -> crate::AgentDeps {
    use std::sync::Arc;

    use domain::{
        ports::{
            Embedding, EmbeddingModel, MockAnalysisRepo, MockBlobStore, MockChatRepo,
            MockGardenRepo, MockKnowledgeStore, MockMapRepo, MockPlanRepo, MockPlantRepo,
            MockScanRepo, MockTextClassifier, MockTextEmbedder, MockUsageRepo,
            MockWeatherCacheRepo, MockWeatherProvider, StoreError,
        },
        testing::{FixedClock, SequentialIds},
        wire::Page,
    };

    let mut chats = MockChatRepo::new();
    chats
        .expect_context()
        .returning(|_, _| Ok((None, Vec::new())));
    let mut gardens = MockGardenRepo::new();
    gardens.expect_garden().returning(|_| {
        Ok(serde_json::from_value(serde_json::json!({
            "id": GARDEN, "name": "Back yard", "my_role": "editor",
            "location_label": "Toronto", "location_precision": "about_1_km",
            "time_zone": "America/Toronto", "soil_profile_status": "pending",
            "member_count": 1, "created_at": "2026-10-01T00:00:00Z"
        }))
        .expect("a valid garden"))
    });
    gardens
        .expect_garden_context()
        .returning(|_| Err(StoreError::Unavailable("offline".into())));
    let mut plants = MockPlantRepo::new();
    plants.expect_plants().returning(|_, _| {
        Ok(Page {
            items: Vec::new(),
            next_cursor: None,
        })
    });
    let mut plan_repo = MockPlanRepo::new();
    plan_repo.expect_tasks().returning(|_, _, _| {
        Ok(Page {
            items: Vec::new(),
            next_cursor: None,
        })
    });
    let mut maps = MockMapRepo::new();
    maps.expect_map().returning(|_| Ok(None));
    let mut embedder = MockTextEmbedder::new();
    embedder
        .expect_embed()
        .returning(|_, texts| Ok(texts.iter().map(|_| Embedding(vec![0.1; 4])).collect()));
    embedder.expect_model().returning(|| EmbeddingModel {
        name: "test-embed".to_owned(),
        version: "1".to_owned(),
        dimension: 4,
    });
    crate::AgentDeps {
        stores: crate::AgentStores {
            chats: Arc::new(chats),
            scans: Arc::new(MockScanRepo::new()),
            analysis: Arc::new(MockAnalysisRepo::new()),
            plans: Arc::new(plan_repo),
            plants: Arc::new(plants),
            gardens: Arc::new(gardens),
            maps: Arc::new(maps),
            usage: Arc::new(MockUsageRepo::new()),
            weather_cache: Arc::new(MockWeatherCacheRepo::new()),
            farm: Arc::new(no_connectors()),
        },
        memory: Arc::new(memory),
        beliefs,
        knowledge: Arc::new(MockKnowledgeStore::new()),
        chat: Arc::new(ScriptedModel::default()),
        classifier: Arc::new(MockTextClassifier::new()),
        embedder: Arc::new(embedder),
        blob: Arc::new(MockBlobStore::new()),
        weather: Arc::new(MockWeatherProvider::new()),
        clock: Arc::new(FixedClock::at(now())),
        ids: Arc::new(SequentialIds::default()),
        products: Arc::new(crate::safety::ProductCatalogue::new(
            &crate::fixtures::products(),
        )),
        config: crate::AgentConfig::default(),
    }
}

/// A farm store for a garden with no connectors: `search_farm_records` isn't offered.
pub fn no_connectors() -> domain::ports::MockFarmRecordStore {
    let mut farm = domain::ports::MockFarmRecordStore::new();
    farm.expect_connectors().returning(|_| Ok(Vec::new()));
    farm
}

/// A connector of [`GARDEN`] with this status and record count.
pub fn connector(
    status: domain::connectors::ConnectorStatus,
    record_count: u32,
) -> domain::connectors::Connector {
    serde_json::from_value(serde_json::json!({
        "id": "01JAX3K9T6R2B8V0QZ4M7D1E5C", "garden_id": GARDEN, "kind": "google_drive",
        "status": status, "endpoint": "era", "sources": [], "record_count": record_count,
        "created_at": "2026-10-01T00:00:00Z"
    }))
    .expect("a valid connector")
}

/// The Maple Row fixture's spray log (`D_SPRAY` in `google-drive.json`) as one search hit.
pub fn spray_log_hit() -> domain::connectors::FarmRecordHit {
    let drive: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/era/maple-row/google-drive.json"
    ))
    .expect("the fixture is JSON");
    let file = drive["files"]
        .as_array()
        .and_then(|files| files.iter().find(|f| f["id"] == "D_SPRAY"))
        .expect("the fixture has the spray log");
    serde_json::from_value(serde_json::json!({
        "text": file["text"],
        "citation": {
            "record_id": "01JAX3K9T6R2B8V0QZ4M7D1E5C.0.D_SPRAY",
            "system": "google_drive",
            "title": file["name"],
            "author": "Maria Lopez",
            "at": "2026-09-04T18:00:00Z",
            "link": "http://localhost:4003/file/d/D_SPRAY/view"
        }
    }))
    .expect("a valid hit")
}

/// A belief store that answers every read after a minute.
pub struct SlowBeliefs;

#[async_trait::async_trait]
impl domain::ports::BeliefStore for SlowBeliefs {
    fn enabled(&self) -> bool {
        true
    }

    async fn record(
        &self,
        _: &domain::ports::BeliefWrite,
    ) -> Result<(), domain::ports::ProviderError> {
        Ok(())
    }

    async fn current(
        &self,
        _: domain::id::GardenId,
        _: Option<domain::id::PlantId>,
    ) -> Result<Vec<domain::chat::Belief>, domain::ports::ProviderError> {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        Ok(vec![crate::fixtures::belief("watering", "late", true)])
    }

    async fn history(
        &self,
        _: domain::ports::BeliefSubject,
    ) -> Result<Vec<domain::chat::Belief>, domain::ports::ProviderError> {
        Ok(Vec::new())
    }

    async fn search(
        &self,
        _: domain::id::GardenId,
        _: Option<domain::id::PlantId>,
        _: &str,
        _: usize,
    ) -> Result<Vec<domain::chat::BeliefHit>, domain::ports::ProviderError> {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        Ok(vec![domain::chat::BeliefHit {
            key: "watering".to_owned(),
            text: "late".to_owned(),
            about_plant: true,
            score: 0.9,
        }])
    }

    async fn forget(
        &self,
        _: domain::ports::BeliefSubject,
    ) -> Result<(), domain::ports::ProviderError> {
        Ok(())
    }
}
