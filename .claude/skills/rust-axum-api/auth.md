# Authentication: Clerk session tokens, worker tokens, the auth webhook

Part of [rust-axum-api](SKILL.md). Who sends which header is fixed by [contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); what the API must check in a Clerk token is in [frontend 8, what the backend needs from Clerk](../../../docs/frontend/README.md#what-the-backend-needs-from-clerk), which itself marks those checks *unverified*. The webhook is [contracts 9, auth webhook](../../../docs/architecture/contracts.md#auth-webhook).

## Contents

1. [Clerk session tokens](#1-clerk-session-tokens)
2. [Workers: bearer token and `X-Lease-Token`](#2-workers-bearer-token-and-x-lease-token)
3. [The auth webhook](#3-the-auth-webhook)
4. [Which routes get which check](#4-which-routes-get-which-check)

## 1. Clerk session tokens

What `jsonwebtoken` 11.1.0 does by default, from [`src/validation.rs`](https://docs.rs/crate/jsonwebtoken/11.1.0/source/src/validation.rs) (`Validation::new_impl` and `validate`) and its [`Validation` docs](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html):

| Field | Default | Set to | Why |
|---|---|---|---|
| `algorithms` | the one passed to `Validation::new` | `[RS256]` | Clerk's verification example sets `algorithms: ['RS256']` and says to check the token's algorithm ([manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification); *unverified*: the example, not prose, names RS256) |
| `required_spec_claims` | `{"exp"}` | `exp`, `nbf`, `iss`, `sub` | `iss` and `nbf` are checked **only when present** unless required: `validate` compares `iss` only when the token has one, and the [`validate_nbf`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html#structfield.validate_nbf) docs say "Validation only happens if `nbf` claim is present" ([`required_spec_claims`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html#structfield.required_spec_claims)). The `no_issuer` case of the test checks it |
| `validate_nbf` | `false` | `true` | [Frontend 8](../../../docs/frontend/README.md#what-the-backend-needs-from-clerk) asks for `nbf` |
| `iss` | `None` (not checked) | the Clerk Frontend API URL | Clerk sets `iss` to it ([session tokens](https://clerk.com/docs/guides/sessions/session-tokens)) |
| [`leeway`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html#structfield.leeway) | 60 s | 5 s (this skill's choice) | 60 s lets an expired token through for a minute; Clerk's own "Allowed Clock Skew" setting already moves `nbf` back ([session tokens](https://clerk.com/docs/guides/sessions/session-tokens)) |
| [`validate_aud`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html#structfield.validate_aud) | `true`, `aud: None` | left as is | Clerk's default claims have no `aud` ([session tokens](https://clerk.com/docs/guides/sessions/session-tokens)). If a token carries one while `aud` is `None`, `validate` rejects it (`InvalidAudience`), so adding an `aud` claim in Clerk needs `set_audience` here |

Then, outside `jsonwebtoken`:

- **`azp`**: if present, it must be one of the allowed origins (the web build's origin). Clerk omits it when the request had no `Origin`, which is the case for the native app, and its guide says to skip the check then ([manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification)).
- **`sub`** is the Clerk user ID, mapped to `user.auth_id`; the user record is upserted on `auth_id` on first sight, because the webhook may not have arrived ([architecture 4, identity](../../../docs/architecture/README.md#identity)).
- **Keys**: fetched from the instance's JWKS (the Frontend API URL plus `/.well-known/jwks.json`, or `https://api.clerk.com/v1/jwks` with the secret key; [manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification)) at start-up, cached by `kid`, and refreshed when a token names an unknown `kid`, at most once a minute so garbage tokens can't make the API hammer Clerk. Build each key with [`DecodingKey::from_jwk`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.DecodingKey.html#method.from_jwk) and look it up with [`JwkSet::find`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/jwk/struct.JwkSet.html#method.find) or by iterating `keys`.
- **Every failure is `401 unauthenticated`** with the same message ([contracts 2](../../../docs/architecture/contracts.md#2-errors): a missing, expired or invalid token). Log the reason at `debug`; don't tell the client which check failed (this skill's choice).
- **The scheme is case-insensitive.** `bearer abc` and `BEARER abc` are the same as `Bearer abc` ([MDN, `Authorization`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Authorization): auth schemes are case-insensitive); `bearer` compares it with `eq_ignore_ascii_case`, and its test checks it.
- **Crypto backend**: `jsonwebtoken` 11 picks its `CryptoProvider` from its features. With exactly one of `aws_lc_rs` and `rust_crypto` on, that one is the default; with none or both, the default provider panics on first use ("Could not automatically determine the process-level CryptoProvider") unless the binary calls [`CryptoProvider::install_default`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/crypto/struct.CryptoProvider.html#method.install_default) once at start-up (`CryptoProvider::from_crate_features` in [`src/crypto/mod.rs`](https://docs.rs/crate/jsonwebtoken/11.1.0/source/src/crypto/mod.rs)). PDM enables only `aws_lc_rs` ([versions](../rust/versions.md)), so nothing is installed by hand; a second crate in the workspace that turns on `rust_crypto` would make every `decode` panic.

*Unverified about Clerk itself* (to confirm against the instance on day 1): the exact `iss` value, that tokens are RS256, the JWKS URL, and whether the native app's tokens really lack `azp`. The `sts` claim (pending session status) matters only with Organizations, which PDM doesn't use.

The test signs with HS256 and a shared secret so it needs no key material; production uses RS256 keys from the JWKS. The claim checks are the same code.

```rust
use std::{
    collections::HashMap,
    sync::{PoisonError, RwLock},
};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;

/// The claims the API reads from a Clerk session token.
#[derive(Debug, Deserialize)]
pub struct SessionClaims {
    pub sub: String,
    pub azp: Option<String>,
    pub sid: Option<String>,
}

/// Every variant becomes `401 unauthenticated`; the variant is only logged.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("no bearer token")]
    Missing,
    #[error("token header has no kid")]
    NoKeyId,
    #[error("unknown signing key {0}")]
    UnknownKey(String),
    #[error("authorized party {0:?} is not an allowed origin")]
    Azp(String),
    #[error("token rejected")]
    Jwt(#[from] jsonwebtoken::errors::Error),
}

pub struct SessionVerifier {
    keys: RwLock<HashMap<String, DecodingKey>>,
    validation: Validation,
    allowed_origins: Vec<String>,
}

impl SessionVerifier {
    /// `issuer`: the Clerk Frontend API URL. `algorithm`: `Algorithm::RS256` in production.
    pub fn new(issuer: &str, allowed_origins: Vec<String>, algorithm: Algorithm) -> Self {
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[issuer]);
        validation.set_required_spec_claims(&["exp", "nbf", "iss", "sub"]);
        validation.validate_nbf = true;
        validation.leeway = 5;
        Self {
            keys: RwLock::new(HashMap::new()),
            validation,
            allowed_origins,
        }
    }

    /// Replaces the cached keys with a freshly fetched JWKS. Keys without a `kid` are skipped.
    pub fn replace_keys(&self, set: &JwkSet) -> Result<usize, jsonwebtoken::errors::Error> {
        let mut fresh = HashMap::new();
        for jwk in &set.keys {
            if let Some(kid) = &jwk.common.key_id {
                fresh.insert(kid.clone(), DecodingKey::from_jwk(jwk)?);
            }
        }
        let count = fresh.len();
        *self.keys.write().unwrap_or_else(PoisonError::into_inner) = fresh;
        Ok(count)
    }

    pub fn insert_key(&self, kid: &str, key: DecodingKey) {
        self.keys
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(kid.to_owned(), key);
    }

    /// `Err(AuthError::UnknownKey)` tells the caller to refresh the JWKS (rate-limited) and retry once.
    pub fn verify(&self, token: &str) -> Result<SessionClaims, AuthError> {
        let kid = decode_header(token)?.kid.ok_or(AuthError::NoKeyId)?;
        let key = self
            .keys
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&kid)
            .cloned()
            .ok_or(AuthError::UnknownKey(kid))?;
        let claims = decode::<SessionClaims>(token, &key, &self.validation)?.claims;
        if let Some(azp) = &claims.azp
            && !self.allowed_origins.iter().any(|origin| origin == azp)
        {
            return Err(AuthError::Azp(azp.clone()));
        }
        Ok(claims)
    }
}

/// `Authorization: Bearer <token>`; the scheme is case-insensitive.
pub fn bearer(headers: &axum::http::HeaderMap) -> Result<&str, AuthError> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim())
        .filter(|token| !token.is_empty())
        .ok_or(AuthError::Missing)
}

/// Fetches the JWKS. Called at start-up and on an unknown `kid`, at most once a minute.
pub async fn fetch_jwks(client: &reqwest::Client, url: &str) -> Result<JwkSet, reqwest::Error> {
    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<JwkSet>()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode, errors::ErrorKind, get_current_timestamp};
    use serde_json::{Value, json};

    const ISSUER: &str = "https://clerk.example.org";
    const WEB: &str = "https://app.example.org";

    fn verifier() -> SessionVerifier {
        let verifier = SessionVerifier::new(ISSUER, vec![WEB.to_owned()], Algorithm::HS256);
        verifier.insert_key("k1", DecodingKey::from_secret(b"test only"));
        verifier
    }

    fn token(claims: &Value) -> String {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("k1".to_owned());
        encode(&header, claims, &EncodingKey::from_secret(b"test only")).unwrap()
    }

    fn claims(changes: &Value) -> Value {
        let now = get_current_timestamp();
        let mut claims = json!({
            "sub": "user_2abc", "iss": ISSUER, "azp": WEB, "sid": "sess_1",
            "nbf": now - 10, "iat": now - 10, "exp": now + 60,
        });
        for (key, value) in changes.as_object().unwrap() {
            if value.is_null() {
                claims.as_object_mut().unwrap().remove(key);
            } else {
                claims[key] = value.clone();
            }
        }
        claims
    }

    fn kind(result: Result<SessionClaims, AuthError>) -> ErrorKind {
        match result {
            Err(AuthError::Jwt(e)) => e.into_kind(),
            other => panic!("expected a jsonwebtoken error, got {other:?}"),
        }
    }

    #[test]
    fn a_valid_token_yields_the_clerk_user_id() {
        let verified = verifier().verify(&token(&claims(&json!({})))).unwrap();
        assert_eq!(verified.sub, "user_2abc");
    }

    #[test]
    fn native_app_tokens_without_azp_pass() {
        let verified = verifier().verify(&token(&claims(&json!({ "azp": null }))));
        assert!(verified.is_ok());
    }

    #[test]
    fn exp_nbf_iss_and_azp_are_enforced() {
        let now = get_current_timestamp();
        let v = verifier();
        let expired = claims(&json!({ "exp": now - 30 }));
        assert_eq!(
            kind(v.verify(&token(&expired))),
            ErrorKind::ExpiredSignature
        );
        let early = claims(&json!({ "nbf": now + 30 }));
        assert_eq!(kind(v.verify(&token(&early))), ErrorKind::ImmatureSignature);
        let other_issuer = claims(&json!({ "iss": "https://evil.example" }));
        assert_eq!(
            kind(v.verify(&token(&other_issuer))),
            ErrorKind::InvalidIssuer
        );
        let no_issuer = claims(&json!({ "iss": null }));
        assert_eq!(
            kind(v.verify(&token(&no_issuer))),
            ErrorKind::MissingRequiredClaim("iss".to_owned())
        );
        let other_origin = claims(&json!({ "azp": "https://evil.example" }));
        assert!(matches!(
            v.verify(&token(&other_origin)),
            Err(AuthError::Azp(_))
        ));
    }

    #[test]
    fn an_unknown_kid_asks_for_a_refresh() {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("k2".to_owned());
        let signed = encode(
            &header,
            &claims(&json!({})),
            &EncodingKey::from_secret(b"x"),
        )
        .unwrap();
        assert!(matches!(
            verifier().verify(&signed),
            Err(AuthError::UnknownKey(kid)) if kid == "k2"
        ));
    }

    #[test]
    fn bearer_needs_the_scheme_in_any_case_and_a_token() {
        let mut headers = axum::http::HeaderMap::new();
        assert!(matches!(bearer(&headers), Err(AuthError::Missing)));
        for value in ["Bearer abc", "bearer abc", "BEARER abc"] {
            headers.insert("authorization", value.parse().unwrap());
            assert_eq!(bearer(&headers).unwrap(), "abc");
        }
        for value in ["Basic abc", "Bearer ", "Bearer", "Bearerabc"] {
            headers.insert("authorization", value.parse().unwrap());
            assert!(
                matches!(bearer(&headers), Err(AuthError::Missing)),
                "{value}"
            );
        }
    }
}
```

The middleware that uses it (`from_fn_with_state`, see [layers 2](layers.md#2-the-stack-in-order)) calls `bearer`, then `verify`; on `UnknownKey` it refreshes the JWKS if the last refresh is over a minute old and verifies once more; then it maps `sub` to the user and inserts that user into the request's extensions for handlers and the rate limiter. Handlers read it with an extractor whose rejection is `ApiError::Internal` (a missing user there means the layer is missing: a bug, not a 401).

## 2. Workers: bearer token and `X-Lease-Token`

- **Worker tokens**: one random token per worker process, from configuration (`.env`, never committed; [AGENTS.md, secrets](../../../AGENTS.md#secrets)). The `/internal/*` middleware maps the bearer token to a worker identity and rejects anything else with `401 unauthenticated` ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers)).
- **Compare without leaking the token through timing.** [`aws_lc_rs::constant_time::verify_slices_are_equal`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/constant_time/fn.verify_slices_are_equal.html) is constant in the contents but returns early when the lengths differ ([`constant_time.rs`](https://docs.rs/crate/aws-lc-rs/1.18.2/source/src/constant_time.rs)), and stopping at the first matching token tells an attacker which slot matched. So `WorkerTokens` stores the SHA-256 digest of each token ([`aws_lc_rs::digest`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/digest/index.html)), hashes the presented token, compares the 32-byte digests, and checks every configured token without an early exit. Hashing first makes every comparison the same length whatever was sent (this skill's choice; the test checks that tokens of any length are told apart).
- **`X-Lease-Token`**: required on every call about a leased job (`events`, `complete`, the job's images; [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). The handler passes it to the store, which checks it **in the same compare-and-set** that writes, so a requeue between the check and the write can't slip through ([database 4](../../../docs/database/README.md#job-claim-with-slots-)). A mismatch, an expired or cancelled lease, or a job of an older run is `409 lease_lost` with `{ job_id }` ([contracts 2](../../../docs/architecture/contracts.md#2-errors)). **Not in the contracts yet:** a missing header is `400 invalid_request` with `field: "X-Lease-Token"`; contracts 2 only defines the wrong-token case ([contracts 12, question 23](../../../docs/architecture/contracts.md#12-open-questions)).
- **A worker's job errors are not HTTP errors.** A failed job is reported in `POST /internal/jobs/{id}/complete` with `status: "failed"` and `error`, answered `204` ([rust-errors](../rust-errors/SKILL.md#worker-job-errors)).

```rust
use aws_lc_rs::{
    constant_time::verify_slices_are_equal,
    digest::{Digest, SHA256, digest},
};
use axum::http::HeaderMap;

/// The configured worker tokens, kept only as SHA-256 digests: (worker name, digest).
pub struct WorkerTokens(Vec<(String, Digest)>);

impl WorkerTokens {
    pub fn new(tokens: Vec<(String, Vec<u8>)>) -> Self {
        let hashed = tokens
            .into_iter()
            .map(|(name, token)| (name, digest(&SHA256, &token)))
            .collect();
        Self(hashed)
    }

    /// Compares the presented token's digest with every configured digest, each in
    /// constant time, and never stops early.
    pub fn identify(&self, presented: &str) -> Option<&str> {
        let presented = digest(&SHA256, presented.as_bytes());
        let mut found = None;
        for (name, token) in &self.0 {
            let equal = verify_slices_are_equal(token.as_ref(), presented.as_ref()).is_ok();
            found = if equal { Some(name.as_str()) } else { found };
        }
        found
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LeaseHeaderError {
    /// 400 `invalid_request`, `field: "X-Lease-Token"`.
    Missing,
}

/// The store compares it inside the write; this only reads it.
pub fn lease_token(headers: &HeaderMap) -> Result<&str, LeaseHeaderError> {
    headers
        .get("x-lease-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
        .ok_or(LeaseHeaderError::Missing)
}

#[test]
fn worker_tokens_identify_their_worker_only() {
    let tokens = WorkerTokens::new(vec![
        ("rig-3070-perception-1".to_owned(), b"t-perception".to_vec()),
        ("rig-5070-vlm-1".to_owned(), b"t-vlm".to_vec()),
    ]);
    assert_eq!(
        tokens.identify("t-perception"),
        Some("rig-3070-perception-1")
    );
    assert_eq!(tokens.identify("t-vlm"), Some("rig-5070-vlm-1"));
    // A prefix, an extension and an empty token: every length is compared as 32 bytes.
    for wrong in ["t-vl", "t-vlmx", "", &"x".repeat(4096)] {
        assert_eq!(tokens.identify(wrong), None, "{wrong}");
    }

    let mut headers = HeaderMap::new();
    assert_eq!(lease_token(&headers), Err(LeaseHeaderError::Missing));
    headers.insert("x-lease-token", "lt_1".parse().unwrap());
    assert_eq!(lease_token(&headers), Ok("lt_1"));
}
```

## 3. The auth webhook

`POST /webhooks/auth` ([contracts 9](../../../docs/architecture/contracts.md#auth-webhook)):

- Take the body as **`Bytes`**, never `ApiJson`: the signature covers the raw body, and parsing then re-serialising changes it ([Svix, verifying payloads](https://docs.svix.com/receiving/verifying-payloads/how)).
- Verify **before** parsing (contracts 9). The scheme, from [Svix, verifying webhooks manually](https://docs.svix.com/receiving/verifying-payloads/how-manual):
  - the signed content is `{svix-id}.{svix-timestamp}.{body}`, with the raw body;
  - the key is the base64 part of the signing secret, after the `whsec_` prefix, decoded;
  - the signature is HMAC-SHA256, base64-encoded;
  - `svix-signature` is a space-separated list of `v1,<base64>` entries (other versions may appear), any of which may match;
  - compare in constant time. [`aws_lc_rs::hmac::verify`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/hmac/fn.verify.html) computes the HMAC and compares it in constant time; the base64 decoding uses `base64` 0.23.1 ([`Engine::decode`](https://docs.rs/base64/0.23.1/base64/engine/trait.Engine.html#method.decode), [versions](../rust/versions.md)).
- Reject a `svix-timestamp` more than 5 minutes from now, in either direction. Contracts 9 says "more than 5 minutes old"; Svix's own libraries "reject webhooks with a timestamp that are more than five minutes away (past or future)" ([Svix, why verify](https://docs.svix.com/receiving/verifying-payloads/why)), and a future timestamp is no fresher than an old one.
- Then parse `AuthWebhookEvent`, apply it idempotently (upsert on `auth_id`, or start the deletion cascade), answer `204` (contracts 9).
- **Not in the contracts yet:** a bad signature or a timestamp more than 5 minutes away answers `401 unauthenticated`, an unreadable body after a good signature `400 invalid_request`; contracts 9 gives no code ([contracts 12, question 24](../../../docs/architecture/contracts.md#12-open-questions)).

The test uses the example from Svix's manual-verification page (secret `whsec_plJ3nmyCDGBKInavdOK15jsl`, `msg_loFOjxBNrRLzqYUf`, timestamp `1731705121`, body `{"event_type":"ping","data":{"success":true}}`, signature `v1,rAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=`). The same signature comes out of Python's standard library, independently of this code: `base64.b64encode(hmac.new(base64.b64decode("plJ3nmyCDGBKInavdOK15jsl"), b"msg_loFOjxBNrRLzqYUf.1731705121." + body, hashlib.sha256).digest())`.

```rust
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use base64::{Engine, engine::general_purpose::STANDARD};

/// The Svix signing scheme (docs.svix.com, verifying webhooks manually).
pub struct SvixVerifier {
    key: aws_lc_rs::hmac::Key,
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the signing secret doesn't start with whsec_")]
    Prefix,
    #[error("the signing secret isn't base64")]
    Base64(#[from] base64::DecodeError),
}

impl SvixVerifier {
    /// `secret`: the endpoint's signing secret, `whsec_<base64>`, from configuration.
    pub fn new(secret: &str) -> Result<Self, SecretError> {
        let encoded = secret.strip_prefix("whsec_").ok_or(SecretError::Prefix)?;
        let bytes = STANDARD.decode(encoded)?;
        let key = aws_lc_rs::hmac::Key::new(aws_lc_rs::hmac::HMAC_SHA256, &bytes);
        Ok(Self { key })
    }

    /// `true` if any `v1` signature in `signatures` matches; each is checked in
    /// constant time, and every entry is checked.
    pub fn verify(&self, id: &str, timestamp: &str, signatures: &str, body: &[u8]) -> bool {
        let mut signed = Vec::with_capacity(id.len() + timestamp.len() + body.len() + 2);
        signed.extend_from_slice(id.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(timestamp.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(body);
        let tags = signatures
            .split(' ')
            .filter_map(|entry| entry.strip_prefix("v1,"))
            .filter_map(|encoded| STANDARD.decode(encoded).ok());
        let mut matched = false;
        for tag in tags {
            matched |= aws_lc_rs::hmac::verify(&self.key, &signed, &tag).is_ok();
        }
        matched
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WebhookError {
    /// 401 `unauthenticated`.
    BadSignature,
    /// 400 `invalid_request`.
    BadBody,
}

#[derive(Debug, serde::Deserialize)]
pub struct AuthWebhookEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: AuthWebhookData,
}

#[derive(Debug, serde::Deserialize)]
pub struct AuthWebhookData {
    pub id: String,
}

/// contracts 9: 5 minutes; Svix applies it to the past and the future.
const TOLERANCE_S: i64 = 5 * 60;

/// Checks the three Svix headers, the timestamp and the signature.
pub fn verify_request(
    verifier: &SvixVerifier,
    headers: &HeaderMap,
    body: &[u8],
    now_unix_s: i64,
) -> Result<(), WebhookError> {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let (Some(id), Some(timestamp), Some(signatures)) = (
        header("svix-id"),
        header("svix-timestamp"),
        header("svix-signature"),
    ) else {
        return Err(WebhookError::BadSignature);
    };
    let sent: i64 = timestamp.parse().map_err(|_| WebhookError::BadSignature)?;
    if (now_unix_s - sent).abs() > TOLERANCE_S || !verifier.verify(id, timestamp, signatures, body)
    {
        return Err(WebhookError::BadSignature);
    }
    Ok(())
}

pub fn verified_event(
    verifier: &SvixVerifier,
    headers: &HeaderMap,
    body: &Bytes,
    now_unix_s: i64,
) -> Result<AuthWebhookEvent, WebhookError> {
    verify_request(verifier, headers, body, now_unix_s)?;
    serde_json::from_slice(body).map_err(|_| WebhookError::BadBody)
}

/// The handler applies the event idempotently and answers 204.
pub fn status_for(result: &Result<AuthWebhookEvent, WebhookError>) -> StatusCode {
    match result {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(WebhookError::BadSignature) => StatusCode::UNAUTHORIZED,
        Err(WebhookError::BadBody) => StatusCode::BAD_REQUEST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example on docs.svix.com, "verifying webhooks manually".
    const SECRET: &str = "whsec_plJ3nmyCDGBKInavdOK15jsl";
    const ID: &str = "msg_loFOjxBNrRLzqYUf";
    const TIMESTAMP: i64 = 1_731_705_121;
    const BODY: &[u8] = br#"{"event_type":"ping","data":{"success":true}}"#;
    const SIGNATURE: &str = "v1,rAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=";

    fn headers(id: &str, timestamp: i64, signatures: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("svix-id", id.parse().unwrap());
        headers.insert("svix-timestamp", timestamp.to_string().parse().unwrap());
        headers.insert("svix-signature", signatures.parse().unwrap());
        headers
    }

    fn check(signatures: &str, body: &[u8], now: i64) -> Result<(), WebhookError> {
        let verifier = SvixVerifier::new(SECRET).unwrap();
        verify_request(&verifier, &headers(ID, TIMESTAMP, signatures), body, now)
    }

    #[test]
    fn svix_documented_example_verifies() {
        assert_eq!(check(SIGNATURE, BODY, TIMESTAMP), Ok(()));
        // Any entry of the list may match; other versions are skipped.
        let several = format!("v2,abc v1,AAAA {SIGNATURE}");
        assert_eq!(check(&several, BODY, TIMESTAMP), Ok(()));
    }

    #[test]
    fn a_changed_body_or_signature_fails() {
        let changed = br#"{"event_type":"ping","data":{"success":false}}"#;
        assert_eq!(
            check(SIGNATURE, changed, TIMESTAMP),
            Err(WebhookError::BadSignature)
        );
        let other = "v1,sAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=";
        assert_eq!(
            check(other, BODY, TIMESTAMP),
            Err(WebhookError::BadSignature)
        );
        let unversioned = SIGNATURE.trim_start_matches("v1,");
        assert_eq!(
            check(unversioned, BODY, TIMESTAMP),
            Err(WebhookError::BadSignature)
        );
    }

    #[test]
    fn timestamps_more_than_five_minutes_away_are_rejected() {
        assert_eq!(check(SIGNATURE, BODY, TIMESTAMP + 300), Ok(()));
        assert_eq!(check(SIGNATURE, BODY, TIMESTAMP - 300), Ok(()));
        for now in [TIMESTAMP + 301, TIMESTAMP - 301] {
            assert_eq!(check(SIGNATURE, BODY, now), Err(WebhookError::BadSignature));
        }
    }

    #[test]
    fn a_secret_without_the_prefix_is_refused() {
        assert!(matches!(
            SvixVerifier::new("plJ3nmyCDGBKInavdOK15jsl"),
            Err(SecretError::Prefix)
        ));
    }

    #[test]
    fn a_verified_auth_event_is_parsed_and_a_bad_one_is_400() {
        let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let verifier = SvixVerifier::new(secret).unwrap();
        let sign = |body: &[u8]| {
            let key_bytes = STANDARD.decode("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw").unwrap();
            let key = aws_lc_rs::hmac::Key::new(aws_lc_rs::hmac::HMAC_SHA256, &key_bytes);
            let signed = [b"msg_1.1000.".as_slice(), body].concat();
            let tag = aws_lc_rs::hmac::sign(&key, &signed);
            format!("v1,{}", STANDARD.encode(tag.as_ref()))
        };
        let body = Bytes::from_static(br#"{"type":"user.created","data":{"id":"user_2abc"}}"#);
        let event = verified_event(
            &verifier,
            &headers("msg_1", 1000, &sign(&body)),
            &body,
            1000,
        );
        let event = event.unwrap();
        assert_eq!(
            (event.kind.as_str(), event.data.id.as_str()),
            ("user.created", "user_2abc")
        );
        let garbage = Bytes::from_static(b"not json");
        let result = verified_event(
            &verifier,
            &headers("msg_1", 1000, &sign(&garbage)),
            &garbage,
            1000,
        );
        assert_eq!(status_for(&result), StatusCode::BAD_REQUEST);
    }
}
```

## 4. Which routes get which check

| Routes | Check | Notes |
|---|---|---|
| Everything the app calls | Clerk session token, then the user in extensions | Then the per-user rate limit ([layers 5](layers.md#5-rate-limits)) |
| `GET /scans/{id}/images/{image_id}`, `GET /scans/{id}/masks/{mask_id}`, `GET /species/{id}/images/{hash}`, `GET /conversations/{id}/images/{image_id}` | The URL's `exp` and `sig` only | No `Authorization` header ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)); a bad signature is `403 invalid_signature` |
| `/internal/*` | Worker token; `X-Lease-Token` per job call | [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) |
| `POST /webhooks/auth` | Svix signature | Section 3 |
| `GET /status`, `GET /healthz` | None | Return no user data ([contracts 9, ops](../../../docs/architecture/contracts.md#ops)) |

Build each group as its own `Router` with its own `route_layer` or `layer`, then `merge` them, so a route can't end up in the wrong group by accident. `Router::route_layer` runs the layer only when a route matched, so an unknown path still gets `404`, not `401` ([`Router::route_layer`](https://docs.rs/axum/0.8.9/axum/struct.Router.html#method.route_layer)).
