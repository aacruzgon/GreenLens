//! `SurrealDB` Agent Memory: the [`BeliefStore`] over its REST API (architecture 7, memory;
//! docs/database/agent-memory.md).
//!
//! The service is hosted on `SurrealDB` Cloud. Every call is `{base_url}/api/v1/{context_id}/…`
//! with `Authorization: Bearer <key>`. Measured against the live service on 2026-10-17:
//!
//! - the `/facts` write body is `snake_case`, the `/query` body `camelCase`, responses `camelCase`,
//!   and **unknown fields are ignored silently**, so the tests assert exact request bodies;
//! - a scope path must be registered before the first write to it (`POST /scopes`, idempotent,
//!   parents created); the adapter caches registered paths and, on the service's
//!   `400 unknown scope node`, registers again and retries the write once;
//! - no `Idempotency-Key` is sent: the service replays a stored response for 24 h, which would
//!   break a rebuild that re-sends a memory after a forget. Triples reconcile by entity and
//!   key, so a re-send creates no duplicate;
//! - an entity that doesn't exist answers `404`, which here means "no beliefs";
//! - `/query` takes 2 to 10 s, a direct entity read about 0.4 s.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
use domain::{
    chat::{Belief, BeliefHit, MemorySource},
    id::{GardenId, MemoryId, PlantId},
    ports::{BELIEFS_PROVIDER, BeliefStore, BeliefSubject, BeliefWrite, ProviderError},
    wire::Timestamp,
};
use moka::future::Cache;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::http::{self, Attempt, ExposeSecret, RetryPolicy, SecretString, TransportError};

/// The most hits one `/query` may ask for.
const MAX_K: usize = 50;
/// Entries per page of a history read.
const HISTORY_PAGE: &str = "500";
/// Pages followed before a history read gives up (a cursor loop guard).
const MAX_HISTORY_PAGES: usize = 200;
/// A write retried after a connect error or a 5xx: two retries, 0.2 s then 0.4 s ceilings.
const WRITE_RETRY: RetryPolicy = RetryPolicy {
    max_retries: 2,
    base: Duration::from_millis(200),
    cap: Duration::from_secs(2),
    max_retry_after: Duration::from_secs(10),
};

/// Settings.
#[derive(Debug, Clone)]
pub struct AgentMemoryConfig {
    /// The service's origin, such as `https://<instance>.agent-memory.surreal.cloud`.
    pub base_url: String,
    /// The context every call belongs to.
    pub context_id: String,
    /// The API key; never printed.
    pub api_key: SecretString,
    /// Deadline of an entity read (`current`, `history`): 3 s.
    pub read_timeout: Duration,
    /// Deadline of `/query`: 8 s, as it measured 2 to 10 s.
    pub search_timeout: Duration,
    /// Deadline of a write, a scope registration or a forget: 15 s.
    pub write_timeout: Duration,
}

impl AgentMemoryConfig {
    /// A configuration with the default timeouts.
    pub fn new(
        base_url: impl Into<String>,
        context_id: impl Into<String>,
        api_key: SecretString,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            context_id: context_id.into(),
            api_key,
            read_timeout: Duration::from_secs(3),
            search_timeout: Duration::from_secs(8),
            write_timeout: Duration::from_secs(15),
        }
    }
}

/// The adapter. One per process: the cache of registered scope paths is shared by every call.
pub struct AgentMemory {
    http: reqwest::Client,
    config: AgentMemoryConfig,
    /// Scope paths known to be registered.
    scopes: Cache<String, ()>,
}

impl std::fmt::Debug for AgentMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentMemory")
            .field("base_url", &self.config.base_url)
            .field("context_id", &self.config.context_id)
            .finish_non_exhaustive()
    }
}

/// What came back from one call, before any status is judged.
struct Reply {
    status: StatusCode,
    retry_after_s: Option<u32>,
    body: Bytes,
}

/// What one attempt at `POST /facts` found.
enum Written {
    Done,
    UnknownScope,
}

/// One attribute of an entity read, a history entry (camelCase, as observed live).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attribute {
    key: String,
    value: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    valid_until: Option<String>,
    #[serde(default)]
    superseded_by: Option<String>,
    created_at: String,
}

/// `GET /entities/other/{name}`.
#[derive(Debug, Deserialize)]
struct EntityBody {
    #[serde(default)]
    attributes: Vec<Attribute>,
}

/// `GET /entities/other/{name}/history`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryBody {
    #[serde(default)]
    history: Vec<Attribute>,
    #[serde(default)]
    page: Option<Page>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    next_cursor: Option<String>,
}

/// `POST /query`.
#[derive(Debug, Deserialize)]
struct QueryBody {
    #[serde(default)]
    hits: Vec<Hit>,
}

#[derive(Debug, Deserialize)]
struct Hit {
    text: String,
    source: String,
    score: f32,
    #[serde(default)]
    resource: Option<Resource>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    #[serde(default)]
    name: String,
    #[serde(default)]
    key: String,
}

impl AgentMemory {
    /// An adapter over the shared client.
    pub fn new(client: reqwest::Client, config: AgentMemoryConfig) -> Self {
        Self {
            http: client,
            config,
            scopes: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(3600))
                .build(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/api/v1/{}{path}",
            self.config.base_url.trim_end_matches('/'),
            self.config.context_id
        )
    }

    /// One call; the status is not judged.
    async fn raw(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Reply, TransportError> {
        let mut request = self
            .http
            .request(method, self.url(path))
            .bearer_auth(self.config.api_key.expose_secret())
            .query(query)
            .timeout(timeout);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(TransportError::from_reqwest)?;
        let status = response.status();
        let retry_after_s =
            http::retry_after(response.headers()).and_then(|d| u32::try_from(d.as_secs()).ok());
        let body = response
            .bytes()
            .await
            .map_err(TransportError::from_reqwest)?;
        Ok(Reply {
            status,
            retry_after_s,
            body,
        })
    }

    /// One call with transport failures mapped; the status is not judged.
    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Reply, ProviderError> {
        self.raw(method, path, query, body, timeout)
            .await
            .map_err(unavailable)
    }

    /// Registers a scope path unless it is known to be registered.
    async fn ensure_scope(&self, path: &str) -> Result<(), ProviderError> {
        if self.scopes.contains_key(path) {
            return Ok(());
        }
        self.register_scope(path).await
    }

    async fn register_scope(&self, path: &str) -> Result<(), ProviderError> {
        let reply = self
            .call(
                Method::POST,
                "/scopes",
                &[],
                Some(&json!({ "path": path })),
                self.config.write_timeout,
            )
            .await?;
        ensure_success(&reply)?;
        self.scopes.insert(path.to_owned(), ()).await;
        Ok(())
    }

    /// One `POST /facts`, retried after a connect error or a 5xx.
    async fn try_write(&self, body: &Value) -> Result<Written, ProviderError> {
        http::retry(WRITE_RETRY, |_| async move {
            match self
                .raw(
                    Method::POST,
                    "/facts",
                    &[],
                    Some(body),
                    self.config.write_timeout,
                )
                .await
            {
                Err(e) => {
                    let connect = e.connect;
                    let error = unavailable(e);
                    if connect {
                        Attempt::Retry {
                            error,
                            retry_after: None,
                        }
                    } else {
                        Attempt::Fail(error)
                    }
                }
                Ok(r) if r.status.is_success() => Attempt::Done(Written::Done),
                Ok(r) if r.status == StatusCode::BAD_REQUEST && is_unknown_scope(&r.body) => {
                    Attempt::Done(Written::UnknownScope)
                }
                Ok(r) => {
                    let error = status_error(&r);
                    if r.status.is_server_error() {
                        Attempt::Retry {
                            error,
                            retry_after: r.retry_after_s.map(|s| Duration::from_secs(s.into())),
                        }
                    } else {
                        Attempt::Fail(error)
                    }
                }
            }
        })
        .await
    }

    /// The current attributes of one entity; a missing entity has none.
    async fn entity(&self, name: &str, about_plant: bool) -> Result<Vec<Belief>, ProviderError> {
        let reply = self
            .call(
                Method::GET,
                &format!("/entities/other/{name}"),
                &[],
                None,
                self.config.read_timeout,
            )
            .await?;
        if reply.status == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        ensure_success(&reply)?;
        let body: EntityBody = parse(&reply.body)?;
        body.attributes
            .into_iter()
            .filter(|a| a.superseded_by.is_none())
            .map(|a| to_belief(a, about_plant))
            .collect()
    }
}

#[async_trait]
impl BeliefStore for AgentMemory {
    fn enabled(&self) -> bool {
        true
    }

    async fn record(&self, write: &BeliefWrite) -> Result<(), ProviderError> {
        let path = write.subject.scope_path();
        self.ensure_scope(&path).await?;
        let body = json!({
            "infer": "triples",
            "scopes": [[path]],
            "observed_at": write.observed_at.into_inner().to_rfc3339_opts(SecondsFormat::Secs, true),
            "labels": [
                format!("memory={}", write.memory),
                format!("source={}", source_label(write.source)),
            ],
            "triples": [{
                "entity": { "type": "other", "name": write.subject.entity_name() },
                "key": write.key,
                "value": write.value,
            }],
        });
        if matches!(self.try_write(&body).await?, Written::Done) {
            return Ok(());
        }
        // The service forgot the path (a forget elsewhere, a rebuilt context): register, retry once.
        self.scopes.invalidate(&path).await;
        self.register_scope(&path).await?;
        match self.try_write(&body).await? {
            Written::Done => Ok(()),
            Written::UnknownScope => Err(ProviderError::BadResponse {
                provider: BELIEFS_PROVIDER,
                source: "the scope is still unknown after registering it".into(),
            }),
        }
    }

    async fn current(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
    ) -> Result<Vec<Belief>, ProviderError> {
        let garden_name = BeliefSubject {
            garden,
            plant: None,
        }
        .entity_name();
        let Some(plant) = plant else {
            return self.entity(&garden_name, false).await;
        };
        let plant_name = BeliefSubject {
            garden,
            plant: Some(plant),
        }
        .entity_name();
        let (plant_beliefs, garden_beliefs) = futures::future::join(
            self.entity(&plant_name, true),
            self.entity(&garden_name, false),
        )
        .await;
        let mut all = plant_beliefs?;
        all.extend(garden_beliefs?);
        Ok(all)
    }

    async fn history(&self, subject: BeliefSubject) -> Result<Vec<Belief>, ProviderError> {
        let path = format!("/entities/other/{}/history", subject.entity_name());
        let about_plant = subject.plant.is_some();
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_HISTORY_PAGES {
            let mut query = vec![("limit", HISTORY_PAGE)];
            if let Some(c) = cursor.as_deref() {
                query.push(("cursor", c));
            }
            let reply = self
                .call(Method::GET, &path, &query, None, self.config.read_timeout)
                .await?;
            if reply.status == StatusCode::NOT_FOUND {
                break;
            }
            ensure_success(&reply)?;
            let body: HistoryBody = parse(&reply.body)?;
            for a in body.history {
                out.push(to_belief(a, about_plant)?);
            }
            // Never stop on a short page: only a missing cursor ends the walk.
            let next = body.page.and_then(|p| p.next_cursor);
            if next.is_none() || next == cursor {
                break;
            }
            cursor = next;
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.learned_at));
        Ok(out)
    }

    async fn search(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
        query: &str,
        k: usize,
    ) -> Result<Vec<BeliefHit>, ProviderError> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let garden_subject = BeliefSubject {
            garden,
            plant: None,
        };
        let plant_subject = plant.map(|p| BeliefSubject {
            garden,
            plant: Some(p),
        });
        let mut lens = Vec::new();
        let mut names = Vec::new();
        if let Some(s) = plant_subject {
            lens.push(json!([s.scope_path()]));
            names.push((s.entity_name(), true));
        }
        lens.push(json!([garden_subject.scope_path()]));
        names.push((garden_subject.entity_name(), false));
        let body = json!({
            "query": query,
            "k": k.min(MAX_K),
            "lens": lens,
            "include": ["facts"],
        });
        let reply = self
            .call(
                Method::POST,
                "/query",
                &[],
                Some(&body),
                self.config.search_timeout,
            )
            .await?;
        ensure_success(&reply)?;
        let parsed: QueryBody = parse(&reply.body)?;
        let hits = parsed
            .hits
            .into_iter()
            .filter(|h| h.source == "attribute")
            .filter_map(|h| {
                let resource = h.resource?;
                let (name, about_plant) = names.iter().find(|(n, _)| *n == resource.name)?;
                let text = h
                    .text
                    .strip_prefix(&format!("{name}'s "))
                    .unwrap_or(&h.text)
                    .to_owned();
                Some(BeliefHit {
                    key: resource.key,
                    text,
                    about_plant: *about_plant,
                    score: h.score,
                })
            })
            .take(k)
            .collect();
        Ok(hits)
    }

    async fn forget(&self, subject: BeliefSubject) -> Result<(), ProviderError> {
        let path = format!("{}/", subject.scope_path());
        let reply = self
            .call(
                Method::POST,
                "/scopes/forget",
                &[],
                Some(&json!({ "path": path })),
                self.config.write_timeout,
            )
            .await?;
        ensure_success(&reply)?;
        // A garden's forget takes its plants' paths too; dropping every entry is cheap, because
        // registering again is idempotent.
        self.scopes.invalidate_all();
        Ok(())
    }
}

const fn source_label(source: MemorySource) -> &'static str {
    match source {
        MemorySource::Pipeline => "pipeline",
        MemorySource::Chat => "chat",
        MemorySource::User => "user",
    }
}

fn is_unknown_scope(body: &[u8]) -> bool {
    String::from_utf8_lossy(body).contains("unknown scope node")
}

fn to_belief(a: Attribute, about_plant: bool) -> Result<Belief, ProviderError> {
    let superseded_at = match (&a.superseded_by, &a.valid_until) {
        (Some(_), Some(until)) => Some(parse_time(until)?),
        _ => None,
    };
    Ok(Belief {
        memory_id: a
            .labels
            .iter()
            .find_map(|l| l.strip_prefix("memory="))
            .and_then(|id| id.parse::<MemoryId>().ok()),
        learned_at: parse_time(&a.created_at)?,
        superseded_at,
        key: a.key,
        value: a.value,
        about_plant,
    })
}

/// RFC 3339 with any number of fractional digits and any offset.
fn parse_time(s: &str) -> Result<Timestamp, ProviderError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| Timestamp::from(t.with_timezone(&Utc)))
        .map_err(|e| ProviderError::BadResponse {
            provider: BELIEFS_PROVIDER,
            source: Box::new(e),
        })
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, ProviderError> {
    serde_json::from_slice(body).map_err(|e| ProviderError::BadResponse {
        provider: BELIEFS_PROVIDER,
        source: Box::new(e),
    })
}

fn ensure_success(reply: &Reply) -> Result<(), ProviderError> {
    if reply.status.is_success() {
        Ok(())
    } else {
        Err(status_error(reply))
    }
}

/// The error a non-2xx answer maps to. The key is never part of it.
fn status_error(reply: &Reply) -> ProviderError {
    let status = reply.status;
    let detail = http::snippet(&String::from_utf8_lossy(&reply.body), 256);
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ProviderError::RateLimited {
            provider: BELIEFS_PROVIDER,
            retry_after_s: reply.retry_after_s,
        };
    }
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return ProviderError::Unavailable {
            provider: BELIEFS_PROVIDER,
            source: format!("HTTP {status}: the API key was refused: {detail}").into(),
        };
    }
    let source = format!("HTTP {status}: {detail}").into();
    if status.is_server_error() {
        ProviderError::Unavailable {
            provider: BELIEFS_PROVIDER,
            source,
        }
    } else {
        ProviderError::BadResponse {
            provider: BELIEFS_PROVIDER,
            source,
        }
    }
}

fn unavailable(e: TransportError) -> ProviderError {
    ProviderError::Unavailable {
        provider: BELIEFS_PROVIDER,
        source: Box::new(e),
    }
}

#[cfg(test)]
mod tests {
    use domain::chat::MemorySource;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path, query_param},
    };

    use super::*;

    const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5G";
    const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5H";
    /// The ids as they appear in scope paths and entity names: lowercased.
    const G: &str = "01jax3k9t6r2b8v0qz4m7d1e5g";
    const P: &str = "01jax3k9t6r2b8v0qz4m7d1e5h";
    const MEMORY: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5F";
    const KEY: &str = "sk-very-secret-key";
    const BASE: &str = "/api/v1/ctx";

    fn subject() -> BeliefSubject {
        BeliefSubject {
            garden: GARDEN.parse().unwrap(),
            plant: Some(PLANT.parse().unwrap()),
        }
    }

    fn garden_subject() -> BeliefSubject {
        BeliefSubject {
            plant: None,
            ..subject()
        }
    }

    fn adapter(server: &MockServer) -> AgentMemory {
        AgentMemory::new(
            reqwest::Client::new(),
            AgentMemoryConfig::new(server.uri(), "ctx", SecretString::from(KEY)),
        )
    }

    fn write() -> BeliefWrite {
        BeliefWrite {
            subject: subject(),
            key: "watering".to_owned(),
            value: "every 2 days".to_owned(),
            memory: MEMORY.parse().unwrap(),
            source: MemorySource::User,
            observed_at: "2026-10-17T15:00:00Z".parse().unwrap(),
        }
    }

    fn plant_path() -> String {
        format!("gardens/{G}/plants/{P}")
    }

    fn fact_body() -> Value {
        json!({
            "infer": "triples",
            "scopes": [[plant_path()]],
            "observed_at": "2026-10-17T15:00:00Z",
            "labels": [format!("memory={MEMORY}"), "source=user"],
            "triples": [{
                "entity": {"type": "other", "name": format!("plant_{P}")},
                "key": "watering",
                "value": "every 2 days",
            }],
        })
    }

    fn registered() -> ResponseTemplate {
        ResponseTemplate::new(201).set_body_json(
            json!({"path": "x/", "createdAt": "2026-10-17T15:00:00Z", "tombstonedAt": null}),
        )
    }

    fn written() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(
            json!({"extraction": {"attributes": [], "corrections": []}, "mode": "triples"}),
        )
    }

    fn attribute(key: &str, value: &str, extra: &Value) -> Value {
        let mut a = json!({
            "id": "x1faa2bq2qruegtp7kzf",
            "key": key,
            "value": value,
            "labels": [format!("memory={MEMORY}"), "source=user"],
            "validFrom": null,
            "validUntil": null,
            "supersedes": null,
            "supersededBy": null,
            "createdAt": "2026-10-17T15:54:42+00:00",
        });
        a.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        a
    }

    #[tokio::test]
    async fn record_sends_the_exact_body_and_registers_the_scope_once() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/scopes")))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .and(body_json(json!({"path": plant_path()})))
            .respond_with(registered())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/facts")))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .and(body_json(fact_body()))
            .respond_with(written())
            .expect(2)
            .mount(&server)
            .await;
        let store = adapter(&server);
        store.record(&write()).await.unwrap();
        store.record(&write()).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        assert!(
            requests
                .iter()
                .all(|r| !r.headers.contains_key("idempotency-key"))
        );
    }

    #[tokio::test]
    async fn an_unknown_scope_node_registers_again_and_retries_once() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/scopes")))
            .respond_with(registered())
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/facts")))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"message": "unknown scope node — register it first"})),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/facts")))
            .respond_with(written())
            .expect(1)
            .mount(&server)
            .await;
        adapter(&server).record(&write()).await.unwrap();
    }

    #[tokio::test]
    async fn an_unknown_scope_that_stays_unknown_is_a_bad_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/scopes")))
            .respond_with(registered())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/facts")))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"message": "unknown scope node"})),
            )
            .expect(2)
            .mount(&server)
            .await;
        let err = adapter(&server).record(&write()).await.unwrap_err();
        assert!(matches!(err, ProviderError::BadResponse { .. }));
    }

    #[tokio::test]
    async fn current_merges_plant_and_garden_and_drops_superseded() {
        let server = MockServer::start().await;
        let plant_body = json!({
            "entity": {},
            "attributes": [
                attribute("watering", "every 2 days", &json!({})),
                attribute("watering", "every 3 days", &json!({
                    "supersededBy": "x1faa2bq2qruegtp7kzf",
                    "validUntil": "2026-10-17T15:54:42+00:00",
                })),
                attribute("container", "a 10 L pot", &json!({
                    "labels": ["source=pipeline", "memory=not-a-ulid"],
                    "createdAt": "2026-10-17T15:57:02.010341933+00:00",
                })),
            ],
            "relations": [],
            "truncated": {"attributes": false, "relations": false},
        });
        let garden_body = json!({
            "entity": {},
            "attributes": [attribute("last_treatment", "neem oil", &json!({"labels": []}))],
        });
        Mock::given(method("GET"))
            .and(path(format!("{BASE}/entities/other/plant_{P}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(plant_body))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{BASE}/entities/other/garden_{G}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(garden_body))
            .mount(&server)
            .await;
        let beliefs = adapter(&server)
            .current(GARDEN.parse().unwrap(), Some(PLANT.parse().unwrap()))
            .await
            .unwrap();
        assert_eq!(beliefs.len(), 3);
        assert_eq!(beliefs[0].key, "watering");
        assert_eq!(beliefs[0].value, "every 2 days");
        assert!(beliefs[0].about_plant);
        assert_eq!(beliefs[0].memory_id, Some(MEMORY.parse().unwrap()));
        assert_eq!(
            beliefs[0].learned_at,
            "2026-10-17T15:54:42Z".parse().unwrap()
        );
        assert_eq!(beliefs[0].superseded_at, None);
        assert_eq!(beliefs[1].key, "container");
        assert_eq!(beliefs[1].memory_id, None);
        assert_eq!(
            beliefs[1].learned_at.into_inner().timestamp_subsec_nanos(),
            10_341_933
        );
        assert_eq!(beliefs[2].key, "last_treatment");
        assert!(!beliefs[2].about_plant);
        assert_eq!(beliefs[2].memory_id, None);
    }

    #[tokio::test]
    async fn a_missing_entity_is_no_beliefs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({"message": "entity other/x does not exist"})),
            )
            .expect(3)
            .mount(&server)
            .await;
        let store = adapter(&server);
        let beliefs = store
            .current(GARDEN.parse().unwrap(), Some(PLANT.parse().unwrap()))
            .await
            .unwrap();
        assert_eq!(beliefs.len(), 0);
        assert_eq!(store.history(subject()).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn history_follows_the_cursor_through_short_pages_and_sorts_newest_first() {
        let server = MockServer::start().await;
        let history_path = format!("{BASE}/entities/other/plant_{P}/history");
        let old = attribute(
            "watering",
            "every 3 days",
            &json!({
                "createdAt": "2026-10-14T15:00:00+00:00",
                "supersededBy": "n1",
                "validUntil": "2026-10-17T15:00:00+00:00",
            }),
        );
        let new = attribute("watering", "every 2 days", &json!({}));
        let mid = attribute(
            "container",
            "pot",
            &json!({"createdAt": "2026-10-16T09:00:00.5+00:00"}),
        );
        // Page 1 is short and says there is more; page 2 has no cursor.
        Mock::given(method("GET"))
            .and(path(history_path.clone()))
            .and(query_param("limit", "500"))
            .and(query_param("cursor", "c1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"history": [new, mid], "page": {"hasMore": false}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(history_path))
            .and(query_param("limit", "500"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "history": [old],
                "page": {"hasMore": true, "nextCursor": "c1"},
            })))
            .mount(&server)
            .await;
        let all = adapter(&server).history(subject()).await.unwrap();
        let keys: Vec<_> = all.iter().map(|b| b.value.as_str()).collect();
        assert_eq!(keys, ["every 2 days", "pot", "every 3 days"]);
        assert_eq!(
            all[2].superseded_at,
            Some("2026-10-17T15:00:00Z".parse().unwrap())
        );
        assert!(all.iter().all(|b| b.about_plant));
    }

    #[tokio::test]
    async fn search_sends_the_exact_lens_filters_hits_and_strips_the_prefix() {
        let server = MockServer::start().await;
        let hit = |source: &str, name: &str, key: &str, text: &str, score: f64| {
            json!({
                "id": "attribute:kuk9",
                "text": text,
                "source": source,
                "score": score,
                "resource": {"kind": "attribute", "entityType": "other", "name": name, "key": key},
            })
        };
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/query")))
            .and(body_json(json!({
                "query": "how often to water",
                "k": 50,
                "lens": [[plant_path()], [format!("gardens/{G}")]],
                "include": ["facts"],
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tier": "hybrid",
                "queryMs": 1200,
                "hits": [
                    hit("attribute", &format!("plant_{P}"), "watering",
                        &format!("plant_{P}'s watering is every 3 days"), 0.5),
                    hit("attribute", "plant_OTHER", "watering", "plant_OTHER's watering is daily", 0.4),
                    hit("episode", &format!("plant_{P}"), "x", "an episode", 0.3),
                    hit("attribute", &format!("garden_{G}"), "light",
                        &format!("garden_{G}'s light is full sun"), 0.2),
                ],
            })))
            .expect(1)
            .mount(&server)
            .await;
        let hits = adapter(&server)
            .search(
                GARDEN.parse().unwrap(),
                Some(PLANT.parse().unwrap()),
                "how often to water",
                100,
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].key, "watering");
        assert_eq!(hits[0].text, "watering is every 3 days");
        assert!(hits[0].about_plant);
        assert!((hits[0].score - 0.5).abs() < 1e-6);
        assert_eq!(hits[1].key, "light");
        assert_eq!(hits[1].text, "light is full sun");
        assert!(!hits[1].about_plant);
    }

    #[tokio::test]
    async fn a_garden_search_uses_one_lens_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/query")))
            .and(body_json(json!({
                "query": "q",
                "k": 3,
                "lens": [[format!("gardens/{G}")]],
                "include": ["facts"],
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"hits": []})))
            .expect(1)
            .mount(&server)
            .await;
        let hits = adapter(&server)
            .search(GARDEN.parse().unwrap(), None, "q", 3)
            .await
            .unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[tokio::test]
    async fn forget_posts_the_trailing_slash_path_and_drops_the_cache() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/scopes/forget")))
            .and(body_json(json!({"path": format!("gardens/{G}/")})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"forgotten": 4})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/scopes")))
            .respond_with(registered())
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/facts")))
            .respond_with(written())
            .mount(&server)
            .await;
        let store = adapter(&server);
        store.record(&write()).await.unwrap();
        store.forget(garden_subject()).await.unwrap();
        // The path must be registered again.
        store.record(&write()).await.unwrap();
    }

    async fn status_of(status: u16, retry_after: Option<&str>) -> ProviderError {
        let server = MockServer::start().await;
        let mut response = ResponseTemplate::new(status).set_body_string("nope");
        if let Some(s) = retry_after {
            response = response.insert_header("retry-after", s);
        }
        Mock::given(method("GET"))
            .respond_with(response)
            .mount(&server)
            .await;
        adapter(&server)
            .current(GARDEN.parse().unwrap(), None)
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn statuses_map_to_the_port_errors() {
        assert!(matches!(
            status_of(500, None).await,
            ProviderError::Unavailable {
                provider: "agent_memory",
                ..
            }
        ));
        assert!(matches!(
            status_of(429, Some("7")).await,
            ProviderError::RateLimited {
                provider: "agent_memory",
                retry_after_s: Some(7)
            }
        ));
        let denied = status_of(401, None).await;
        assert!(matches!(denied, ProviderError::Unavailable { .. }));
        let ProviderError::Unavailable { source, .. } = denied else {
            unreachable!()
        };
        assert!(source.to_string().contains("401"));
        assert!(!source.to_string().contains(KEY));
        assert!(matches!(
            status_of(403, None).await,
            ProviderError::Unavailable { .. }
        ));
        assert!(matches!(
            status_of(422, None).await,
            ProviderError::BadResponse { .. }
        ));
    }

    #[tokio::test]
    async fn an_unparsable_body_is_a_bad_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let err = adapter(&server)
            .current(GARDEN.parse().unwrap(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::BadResponse { .. }));
    }

    #[tokio::test]
    async fn a_5xx_write_is_retried_then_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/scopes")))
            .respond_with(registered())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{BASE}/facts")))
            .respond_with(ResponseTemplate::new(503))
            .expect(3)
            .mount(&server)
            .await;
        let err = adapter(&server).record(&write()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn debug_hides_the_key() {
        let server_less = AgentMemory::new(
            reqwest::Client::new(),
            AgentMemoryConfig::new("http://localhost", "ctx", SecretString::from(KEY)),
        );
        assert!(!format!("{server_less:?}").contains(KEY));
        assert!(!format!("{:?}", server_less.config).contains(KEY));
    }

    /// Against a real context: set `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and
    /// `AGENT_MEMORY_API_KEY` (a narrowed key), then
    /// `cargo test -p integrations -- --ignored agent_memory::tests::live_`. It writes under a
    /// fresh garden id and erases that garden's scope at the end, pass or fail.
    #[tokio::test]
    #[ignore = "needs a live Agent Memory context"]
    async fn live_round_trip_supersedes_and_forgets() {
        let var = |n: &str| std::env::var(n).unwrap_or_else(|_| panic!("{n} is not set"));
        let fresh = || {
            let ms = u64::try_from(Utc::now().timestamp_millis()).unwrap();
            ulid::Ulid::from_parts(ms, rand::random::<u128>()).to_string()
        };
        let am = AgentMemory::new(
            reqwest::Client::new(),
            AgentMemoryConfig::new(
                var("AGENT_MEMORY_URL"),
                var("AGENT_MEMORY_CONTEXT_ID"),
                SecretString::from(var("AGENT_MEMORY_API_KEY")),
            ),
        );
        let garden: GardenId = fresh().parse().unwrap();
        let plant: PlantId = fresh().parse().unwrap();
        let on_plant = BeliefSubject {
            garden,
            plant: Some(plant),
        };
        let on_garden = BeliefSubject {
            garden,
            plant: None,
        };
        let now = Timestamp::from(Utc::now());
        let write = |subject, key: &str, value: &str, days_ago: u64| BeliefWrite {
            subject,
            key: key.to_owned(),
            value: value.to_owned(),
            memory: fresh().parse().unwrap(),
            source: MemorySource::User,
            observed_at: now.minus(Duration::from_secs(days_ago * 86_400)),
        };
        let container = write(on_garden, "container", "Everything is in containers.", 5);
        let older = write(on_plant, "watering", "Watered every 3 days.", 3);
        let newer = write(on_plant, "watering", "Watered every 2 days.", 0);

        let run = async {
            for w in [&container, &older, &newer] {
                am.record(w).await?;
            }
            let current = am.current(garden, Some(plant)).await?;
            let history = am.history(on_plant).await?;
            let hits = am
                .search(garden, Some(plant), "How often is it watered?", 5)
                .await?;
            Ok::<_, ProviderError>((current, history, hits))
        }
        .await;
        am.forget(on_garden)
            .await
            .expect("the garden scope is erased");
        let (current, history, hits) = run.expect("the round trip succeeds");

        let watering: Vec<_> = current.iter().filter(|b| b.key == "watering").collect();
        assert_eq!(
            watering.len(),
            1,
            "one current watering belief: {current:?}"
        );
        assert_eq!(watering[0].value, "Watered every 2 days.");
        assert_eq!(watering[0].memory_id, Some(newer.memory));
        assert!(watering[0].about_plant);
        assert!(
            current
                .iter()
                .any(|b| b.key == "container" && !b.about_plant),
            "the garden-level belief comes with the plant's: {current:?}"
        );
        let old = history
            .iter()
            .find(|b| b.value == "Watered every 3 days.")
            .expect("the superseded value stays in the history");
        assert!(old.superseded_at.is_some());
        assert_eq!(
            history.first().map(|b| b.memory_id),
            Some(Some(newer.memory))
        );
        assert!(
            hits.iter()
                .all(|h| h.key == "watering" || h.key == "container"),
            "search returns only this subject's beliefs: {hits:?}"
        );
        assert!(
            am.current(garden, Some(plant)).await.unwrap().is_empty(),
            "forget erased everything"
        );
    }
}
