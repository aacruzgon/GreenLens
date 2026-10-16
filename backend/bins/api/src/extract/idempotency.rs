//! `Idempotency-Key` on the create calls ([contracts 1], idempotency; [database 4], idempotency
//! keys).
//!
//! | Case | Answer |
//! |---|---|
//! | No header | Runs normally; nothing is stored |
//! | New key | Runs; a `2xx` or deterministic `4xx` (`400`, `403`, `404`, `413`, `415`, `422`) is stored for 24 hours, anything else releases the key |
//! | Same key, same fingerprint, finished | The stored status and body (chat: `200 { message }`) |
//! | Same key, same fingerprint, running | `409 request_in_progress`, `retryable`, `retry_after_s` |
//! | Same key, different fingerprint | `422 idempotency_conflict` |
//!
//! The fingerprint of a JSON body is a SHA-256 of its canonical form (keys sorted at every level,
//! whatever order the client sent); of a multipart body, of its fields and file bytes, never the
//! raw body, whose boundary changes on every retry ([`crate::uploads::UploadForm::fingerprint`]).

use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequest, FromRequestParts, Request},
    http::{HeaderMap, Method},
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use domain::{
    error::ErrorKind,
    id::{IdempotencyKey, UserId},
    ports::{IdempotencyBegin, IdempotencyRepo, IdempotencyScope, StoredResponse},
    rules::limits::IDEMPOTENCY_TTL,
};

use crate::{
    error::ApiError,
    extract::{self, auth::AuthUser},
    reply::{self, Wire},
    state::AppState,
};

/// `retry_after_s` of `409 request_in_progress`.
const IN_PROGRESS_RETRY_S: u32 = 2;
/// The JSON body cap of idempotent routes (the contracts give none; 1 MiB).
pub const JSON_MAX_BYTES: usize = 1024 * 1024;

/// What `begin` decided.
pub enum Begin {
    /// Run the handler; finish the record afterwards.
    Fresh(Begun),
    /// The stored response of an earlier, finished request.
    Replay(Response),
}

/// A started (or absent) idempotency record. Finish it with [`Begun::finish`],
/// [`Begun::complete`] or [`Begun::release`]; if it is dropped unfinished (a panic, a cancelled
/// request), its `Drop` releases the key in a spawned task, so a retry runs again instead of
/// getting `409 request_in_progress` until the key expires.
pub struct Begun {
    scope: Option<IdempotencyScope>,
    repo: Arc<dyn IdempotencyRepo>,
    request_id: String,
}

impl Begun {
    /// A record for a request without the header.
    fn none(repo: Arc<dyn IdempotencyRepo>, request_id: String) -> Self {
        Self {
            scope: None,
            repo,
            request_id,
        }
    }

    /// Stores the handler's result when contracts 1 allows it, and returns the response.
    pub async fn finish<T: Serialize + ToSchema>(
        mut self,
        result: Result<Wire<T>, ApiError>,
    ) -> Response {
        let Some(scope) = self.scope.take() else {
            return result.into_response();
        };
        let stored = match &result {
            Ok(wire) => wire.stored().ok(),
            Err(error) if error.code().is_stored_for_idempotency() => {
                serde_json::to_value(error.body(&self.request_id))
                    .ok()
                    .map(|body| StoredResponse {
                        status: error.code().http_status(),
                        body,
                    })
            }
            Err(_) => None,
        };
        let outcome = match stored {
            Some(response) => self.repo.finish(&scope, response).await,
            None => self.repo.release(&scope).await,
        };
        if let Err(error) = outcome {
            tracing::warn!(error = %error, "idempotency record not finished");
        }
        result.into_response()
    }

    /// Stores a response built outside a `Wire` (the chat replay, `200 { message }`).
    pub async fn complete(mut self, response: StoredResponse) {
        if let Some(scope) = self.scope.take()
            && let Err(error) = self.repo.finish(&scope, response).await
        {
            tracing::warn!(error = %error, "idempotency record not finished");
        }
    }

    /// Releases the key: the next request with it runs again.
    pub async fn release(mut self) {
        if let Some(scope) = self.scope.take()
            && let Err(error) = self.repo.release(&scope).await
        {
            tracing::warn!(error = %error, "idempotency key not released");
        }
    }
}

impl Drop for Begun {
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let repo = Arc::clone(&self.repo);
            runtime.spawn(async move {
                let _ = repo.release(&scope).await;
            });
        }
    }
}

/// Starts the record for `(user, method, path, key)` with `fingerprint`.
pub async fn begin(
    state: &AppState,
    user: UserId,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    request_id: String,
    fingerprint: &str,
) -> Result<Begin, ApiError> {
    let repo: Arc<dyn IdempotencyRepo> = state.ports.store.clone();
    let Some(raw) = headers.get("idempotency-key") else {
        return Ok(Begin::Fresh(Begun::none(repo, request_id)));
    };
    let key: IdempotencyKey = raw
        .to_str()
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::invalid_request("Idempotency-Key", "not a UUID"))?;
    let scope = IdempotencyScope {
        user,
        route: format!("{method} {path}"),
        key,
    };
    let expires_at = state.ports.clock.now().plus(IDEMPOTENCY_TTL);
    match repo.begin(&scope, fingerprint, expires_at).await? {
        IdempotencyBegin::Started => Ok(Begin::Fresh(Begun {
            scope: Some(scope),
            repo,
            request_id,
        })),
        IdempotencyBegin::Replay(stored) => Ok(Begin::Replay(reply::replay(stored))),
        IdempotencyBegin::InProgress => {
            Err(ApiError::new(ErrorKind::RequestInProgress).retry_after(IN_PROGRESS_RETRY_S))
        }
        IdempotencyBegin::Conflict => Err(ApiError::new(ErrorKind::IdempotencyConflict)),
    }
}

/// SHA-256 of a JSON value's canonical form: object keys sorted at every level, no whitespace.
/// Written by hand rather than through `serde_json::to_vec`, whose key order depends on whether
/// any crate in the build turned on `serde_json/preserve_order`.
pub fn canonical_json_hash(value: &serde_json::Value) -> String {
    fn write(hasher: &mut Sha256, value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                hasher.update(b"{");
                for key in keys {
                    write(hasher, &serde_json::Value::String(key.clone()));
                    hasher.update(b":");
                    write(hasher, &map[key]);
                    hasher.update(b",");
                }
                hasher.update(b"}");
            }
            serde_json::Value::Array(items) => {
                hasher.update(b"[");
                for item in items {
                    write(hasher, item);
                    hasher.update(b",");
                }
                hasher.update(b"]");
            }
            scalar => hasher.update(scalar.to_string().as_bytes()),
        }
    }
    let mut hasher = Sha256::new();
    write(&mut hasher, value);
    hex_digest(hasher)
}

/// The lowercase hex of a finished SHA-256.
pub fn hex_digest(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            use std::fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// A JSON body under an idempotency record: the extractor reads the body, fingerprints it,
/// begins the record, then deserialises `T`. A replay or a refusal is the rejection, so the
/// handler runs only for a fresh request.
pub struct IdemJson<T> {
    /// The caller (the extractor authenticates first, to scope the key).
    pub user: domain::authz::CurrentUser,
    /// The body.
    pub body: T,
    /// The record to finish.
    pub begun: Begun,
}

/// `IdemJson`'s rejection: an error, or the stored response of a finished request.
pub enum IdemRejection {
    /// An error response.
    Error(ApiError),
    /// The replay.
    Replay(Response),
}

impl IntoResponse for IdemRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Error(e) => e.into_response(),
            Self::Replay(r) => r,
        }
    }
}

impl From<ApiError> for IdemRejection {
    fn from(e: ApiError) -> Self {
        Self::Error(e)
    }
}

impl<S, T> FromRequest<S> for IdemJson<T>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = IdemRejection;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = request.into_parts();
        let AuthUser(user) = AuthUser::from_request_parts(&mut parts, state).await?;
        let app = Arc::<AppState>::from_ref(state);
        let request_id = extract::request_id(&parts);
        let bytes = axum::body::to_bytes(body, JSON_MAX_BYTES)
            .await
            .map_err(|_| ApiError::invalid_request("body", "at most 1 MiB of JSON"))?;
        let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
        let fingerprint = if let Ok(value) = &parsed {
            canonical_json_hash(value)
        } else {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            hex_digest(hasher)
        };
        let begun = match begin(
            &app,
            user.id(),
            &parts.method,
            parts.uri.path(),
            &parts.headers,
            request_id,
            &fingerprint,
        )
        .await?
        {
            Begin::Fresh(begun) => begun,
            Begin::Replay(response) => return Err(IdemRejection::Replay(response)),
        };
        let decoded = parsed
            .map_err(|e| ApiError::invalid_request("body", e.to_string()))
            .and_then(|value| {
                serde_path_to_error::deserialize::<_, T>(value).map_err(|e| {
                    let path = e.path().to_string();
                    let field = if path == "." { "body".to_owned() } else { path };
                    ApiError::invalid_request(field, e.inner().to_string())
                })
            });
        match decoded {
            Ok(body) => Ok(Self { user, body, begun }),
            // A malformed body is a deterministic 400: stored like any other.
            Err(error) => Err(IdemRejection::Replay(
                begun
                    .finish::<domain::garden::ops::Healthz>(Err(error))
                    .await,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_order_doesnt_change_the_fingerprint() {
        let a: serde_json::Value =
            serde_json::from_str(r#"{"b": 1, "a": {"y": [1, 2], "x": null}}"#).unwrap();
        let b: serde_json::Value =
            serde_json::from_str(r#"{"a": {"x": null, "y": [1, 2]}, "b": 1}"#).unwrap();
        let c: serde_json::Value =
            serde_json::from_str(r#"{"a": {"x": null, "y": [2, 1]}, "b": 1}"#).unwrap();
        assert_eq!(canonical_json_hash(&a), canonical_json_hash(&b));
        assert_ne!(canonical_json_hash(&a), canonical_json_hash(&c));
    }
}
