//! Test support for Era's Slack and Google Drive simulators (docs/EON, E2.3). Compiled only with
//! the `era-sim` feature, so no release build contains it.
//!
//! A [`Tenant`] is one isolated copy of a simulator's company, with a unique id. Tests pass
//! [`Tenant::token`] as the client's bearer token: the credential selects the tenant, so no
//! `X-Sim-Tenant` header and no test-only constructor exist, and the request journal is on.
//! `close` is best effort and there is no async `Drop`; a leaked tenant is harmless because ids
//! are unique. Faults are global to a simulator, so every test named `era_*` runs alone, in the
//! nextest group `era` (`backend/.config/nextest.toml`).
//!
//! Every failure panics with the status and body: this is test support, and the bodies are the
//! simulator's own, never user data.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// A credential is valid for one hour; it is minted again after this long.
const TOKEN_MAX_AGE: Duration = Duration::from_mins(50);

/// Which simulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimKind {
    Slack,
    Drive,
}

impl SimKind {
    fn env_var(self) -> &'static str {
        match self {
            Self::Slack => "ERA_SLACK_BASE",
            Self::Drive => "ERA_GOOGLE_BASE",
        }
    }
}

/// The simulator's base URL (no trailing slash) from `ERA_SLACK_BASE` or `ERA_GOOGLE_BASE`.
/// Panics when the variable is unset, so an Era test fails rather than skips.
#[must_use]
pub fn base_url(kind: SimKind) -> String {
    match std::env::var(kind.env_var()) {
        Ok(url) if !url.is_empty() => url.trim_end_matches('/').to_owned(),
        _ => panic!(
            "{} is not set: start Era's simulators (docker compose -f infra/dev.compose.yml \
             --profile era up -d --wait era-slack era-drive) and set ERA_SLACK_BASE and \
             ERA_GOOGLE_BASE (docs/EON)",
            kind.env_var()
        ),
    }
}

/// Reads `backend/tests/fixtures/era/<name>/<file>` as JSON.
#[must_use]
pub fn fixture(name: &str, file: &str) -> Value {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "../../tests/fixtures/era",
        name,
        file,
    ]
    .iter()
    .collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("fixture {} is not JSON: {e}", path.display()))
}

/// One request the simulator's journal recorded.
#[derive(Clone, Debug, Deserialize)]
pub struct JournalEntry {
    pub method: String,
    pub route: String,
    pub status: u16,
    pub latency_ms: f64,
}

#[derive(Deserialize)]
struct Journal {
    requests: Vec<JournalEntry>,
}

async fn expect_ok(what: &str, response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert!(status.is_success(), "{what}: {status}: {body}");
    body
}

/// Sets and clears the faults of one simulator. They are global to it, not per tenant.
pub struct Faults;

impl Faults {
    /// `POST /_admin/faults` with `fail_next`, `retry_after_seconds`, `throttle`, `error_rate`,
    /// `latency_ms` or `quota_limit`.
    pub async fn set(kind: SimKind, faults: Value) {
        let url = format!("{}/_admin/faults", base_url(kind));
        let response = reqwest::Client::new().post(url).json(&faults).send().await;
        expect_ok("set faults", response.expect("simulator unreachable")).await;
    }

    /// `POST /_admin/faults/reset`.
    pub async fn reset(kind: SimKind) {
        let url = format!("{}/_admin/faults/reset", base_url(kind));
        let response = reqwest::Client::new().post(url).send().await;
        expect_ok("reset faults", response.expect("simulator unreachable")).await;
    }
}

/// One tenant of a simulator, with a minted credential.
pub struct Tenant {
    kind: SimKind,
    id: String,
    base: String,
    http: reqwest::Client,
    credential: Mutex<(String, Instant)>,
}

impl Tenant {
    /// A fresh tenant with a unique id (`t-<ulid>`), the fixture injected when given.
    pub async fn create(kind: SimKind, fixture: Option<&Value>) -> Self {
        let base = base_url(kind);
        let id = format!("t-{}", ulid::Ulid::generate().to_string().to_lowercase());
        let http = reqwest::Client::new();
        let credential = Mutex::new((mint(&http, &base, &id).await, Instant::now()));
        let tenant = Self {
            kind,
            id,
            base,
            http,
            credential,
        };
        if let Some(fixture) = fixture {
            tenant.inject(fixture).await;
        }
        tenant
    }

    #[must_use]
    pub fn kind(&self) -> SimKind {
        self.kind
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The simulator's base URL, where the client under test is pointed.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The tenant-bound bearer token; minted again when older than 50 minutes.
    pub async fn token(&self) -> String {
        let mut credential = self.credential.lock().await;
        if credential.1.elapsed() > TOKEN_MAX_AGE {
            *credential = (mint(&self.http, &self.base, &self.id).await, Instant::now());
        }
        credential.0.clone()
    }

    fn admin(&self, path: &str) -> String {
        format!("{}/_admin/tenants/{}{path}", self.base, self.id)
    }

    /// Adds a fixture to the tenant (`POST .../inject`).
    pub async fn inject(&self, fixture: &Value) {
        let response = self
            .http
            .post(self.admin("/inject"))
            .json(fixture)
            .send()
            .await;
        expect_ok("inject", response.expect("simulator unreachable")).await;
    }

    /// Moves the tenant to `day0`, `day1` or `day2`.
    pub async fn state(&self, day: &str) {
        let response = self
            .http
            .post(self.admin(&format!("/state/{day}")))
            .send()
            .await;
        expect_ok("set state", response.expect("simulator unreachable")).await;
    }

    /// The tenant's current state name (`day1` and so on).
    pub async fn current_state(&self) -> String {
        let response = self.http.get(self.admin("/state")).send().await;
        let body = expect_ok("get state", response.expect("simulator unreachable")).await;
        let value: Value = serde_json::from_str(&body).expect("state is JSON");
        value["state"]
            .as_str()
            .expect("state has a name")
            .to_owned()
    }

    /// The request journal (up to 500 entries), oldest first.
    pub async fn requests(&self) -> Vec<JournalEntry> {
        let response = self.http.get(self.admin("/requests")).send().await;
        let body = expect_ok("journal", response.expect("simulator unreachable")).await;
        serde_json::from_str::<Journal>(&body)
            .expect("journal shape")
            .requests
    }

    /// Returns the tenant to the generated company.
    pub async fn reset(&self) {
        let response = self.http.post(self.admin("/reset")).send().await;
        expect_ok("reset", response.expect("simulator unreachable")).await;
    }

    /// Drops the tenant. Best effort: call it at the end of a test; a test that panics earlier
    /// leaks a tenant with a unique id, which is harmless.
    pub async fn close(self) {
        let response = self.http.delete(self.admin("")).send().await;
        expect_ok("close", response.expect("simulator unreachable")).await;
    }
}

async fn mint(http: &reqwest::Client, base: &str, id: &str) -> String {
    let url = format!("{base}/_admin/tenants/{id}/credentials");
    let response = http.post(url).json(&json!({})).send().await;
    let body = expect_ok("mint credential", response.expect("simulator unreachable")).await;
    let value: Value = serde_json::from_str(&body).expect("credential is JSON");
    value["access_token"]
        .as_str()
        .expect("credential has access_token")
        .to_owned()
}
