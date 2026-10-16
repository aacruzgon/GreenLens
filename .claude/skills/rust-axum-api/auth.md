# Authentication: Clerk session tokens, worker tokens, the auth webhook

Part of [rust-axum-api](SKILL.md). Who sends which header is fixed by [contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); what the API must check in a Clerk token is in [frontend 8, what the backend needs from Clerk](../../../docs/frontend/README.md#what-the-backend-needs-from-clerk), which itself marks those checks *unverified*. The webhook is [contracts 9, auth webhook](../../../docs/architecture/contracts.md#auth-webhook). In the backend, the token and webhook checks are `integrations` adapters behind the `AuthVerifier` and `WebhookVerifier` ports ([backend integrations 10](../../../docs/backend/integrations.md#10-clerk), [11](../../../docs/backend/integrations.md#11-svix-and-the-auth-webhook)), and `api` calls them from extractors and handlers ([backend api 9](../../../docs/backend/api.md#9-authentication-and-access)).

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
| [`leeway`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html#structfield.leeway) | 60 s | 5 s | The backend's `ClerkVerifier` uses 5 s, applied to its own `exp` and `nbf` checks ([backend integrations 10, session tokens](../../../docs/backend/integrations.md#session-tokens-authverifier): "checked against the caller's `now` with a 5 s leeway"). 60 s lets an expired token through for a minute; Clerk's own "Allowed Clock Skew" setting already moves `nbf` back ([session tokens](https://clerk.com/docs/guides/sessions/session-tokens)) |
| [`validate_aud`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html#structfield.validate_aud) | `true`, `aud: None` | left as is | Clerk's default claims have no `aud` ([session tokens](https://clerk.com/docs/guides/sessions/session-tokens)). If a token carries one while `aud` is `None`, `validate` rejects it (`InvalidAudience`), so adding an `aud` claim in Clerk needs `set_audience` here |

Then, outside `jsonwebtoken`:

- **`azp`**: if present, it must be one of the allowed origins (the web build's origin). Clerk omits it when the request had no `Origin`, which is the case for the native app, and its guide says to skip the check then ([manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification)).
- **`sub`** is the Clerk user ID, mapped to `user.auth_id`. The user record is created by the user's **first authenticated request**, an upsert on `auth_id`, so two first requests racing each other agree; the `user.created` webhook creates nothing, because Clerk doesn't guarantee event order and a late `user.created` must never recreate a deleted user (section 3; [contracts 9, auth webhook](../../../docs/architecture/contracts.md#auth-webhook); [contracts 12, question 24](../../../docs/architecture/contracts.md#12-open-questions); [backend api 22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions)). [Architecture 4, identity](../../../docs/architecture/README.md#identity) says the same since v0.7.
- **A deleted account's token is refused.** A Clerk session token stays valid for up to its lifetime (60 s by default) after the account is deleted. While the account's tombstone `deleted_user:[auth_id]` exists, the store's upsert refuses the auth id, and `domain::authz::authenticate` turns that into `AuthError::AccountDeleted`: `401 unauthenticated`, never a new, empty user ([backend api 9, `AuthUser`](../../../docs/backend/api.md#authuser); [backend domain 8](../../../docs/backend/domain.md#8-authorization-proofs), `authenticate`). The tombstone is written before anything else, kept until 24 h after Clerk confirms the deletion, and an unfinished account deletion resumes its Clerk retries after a restart, so the deletion survives a restart and the tokens stay refused all that time ([contracts 9, account](../../../docs/architecture/contracts.md#account); [contracts 12, question 40](../../../docs/architecture/contracts.md#12-open-questions), settled in v0.10.1; [backend api 17](../../../docs/backend/api.md#17-deletion)).
- **Keys**: fetched from the instance's JWKS (the Frontend API URL plus `/.well-known/jwks.json`, or `https://api.clerk.com/v1/jwks` with the secret key; [manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification)) at start-up, cached by `kid`, and refreshed when a token names an unknown `kid`, at most once every 30 s so garbage tokens can't make the API hammer Clerk ([backend integrations 10](../../../docs/backend/integrations.md#session-tokens-authverifier)). Build each key with [`DecodingKey::from_jwk`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.DecodingKey.html#method.from_jwk) and look it up with [`JwkSet::find`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/jwk/struct.JwkSet.html#method.find) or by iterating `keys`.
- **Every failure is `401 unauthenticated`** with the same message ([contracts 2](../../../docs/architecture/contracts.md#2-errors): a missing, expired or invalid token). Log the reason at `debug`; don't tell the client which check failed (this skill's choice).
- **The scheme is case-insensitive.** `bearer abc` and `BEARER abc` are the same as `Bearer abc` ([MDN, `Authorization`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Authorization): auth schemes are case-insensitive); `bearer` compares it with `eq_ignore_ascii_case`, and its test checks it.
- **Crypto backend**: `jsonwebtoken` 11 picks its `CryptoProvider` from its features. With exactly one of `aws_lc_rs` and `rust_crypto` on, that one is the default; with none or both, the default provider panics on first use ("Could not automatically determine the process-level CryptoProvider") unless the binary calls [`CryptoProvider::install_default`](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/crypto/struct.CryptoProvider.html#method.install_default) once at start-up (`CryptoProvider::from_crate_features` in [`src/crypto/mod.rs`](https://docs.rs/crate/jsonwebtoken/11.1.0/source/src/crypto/mod.rs)). PDM enables only `aws_lc_rs` ([versions](../rust/versions.md)), so nothing is installed by hand; a second crate in the workspace that turns on `rust_crypto` would make every `decode` panic.

**Where the backend's `ClerkVerifier` differs from the example** ([backend integrations 10, session tokens](../../../docs/backend/integrations.md#session-tokens-authverifier)): `jsonwebtoken`'s own time checks read the system clock, so it turns them off and checks `exp` and `nbf` itself against the `Clock` port's `now`, with the same 5 s leeway, which makes them testable; RS256 is the only algorithm; keys are cached by `kid` for an hour, and an unknown `kid` refetches the whole set at most once every 30 s, single-flight behind a mutex. A JWKS that can't be fetched is `503 upstream_unavailable`, not `401`.

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

/// Fetches the JWKS. Called at start-up and on an unknown `kid`, at most once every 30 s.
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

The `AuthUser` extractor that uses it (section 4) calls `bearer`, then `verify`; on `UnknownKey` the verifier refetches the JWKS (rate-limited, as above) and verifies once more; then the extractor maps `sub` to the user (refused for a tombstoned account), checks the general per-user rate limit ([layers 5](layers.md#5-rate-limits)), and caches the user in the request's extensions, so a second extractor on the same request (the access extractor and the idempotency extractor) doesn't verify twice ([backend api 9, `AuthUser`](../../../docs/backend/api.md#authuser)).

## 2. Workers: bearer token and `X-Lease-Token`

- **Worker tokens**: one random token per worker process, from configuration (`.env`, never committed; [AGENTS.md, secrets](../../../AGENTS.md#secrets)). The `WorkerAuth` extractor on every `/internal/*` handler maps the bearer token to a worker identity and rejects anything else with `401 unauthenticated`; so does a claim or heartbeat whose body names another worker ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); [backend api 9, workers](../../../docs/backend/api.md#workers)).
- **Compare without leaking the token through timing.** [`aws_lc_rs::constant_time::verify_slices_are_equal`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/constant_time/fn.verify_slices_are_equal.html) is constant in the contents but returns early when the lengths differ ([`constant_time.rs`](https://docs.rs/crate/aws-lc-rs/1.18.2/source/src/constant_time.rs)), and stopping at the first matching token tells an attacker which slot matched. So `WorkerTokens` stores the SHA-256 digest of each token ([`aws_lc_rs::digest`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/digest/index.html)), hashes the presented token, compares the 32-byte digests, and checks every configured token without an early exit. Hashing first makes every comparison the same length whatever was sent (the test checks that tokens of any length are told apart). **A deliberate difference from [backend api 9, workers](../../../docs/backend/api.md#workers)**, which keeps each raw token as a `SecretString` (`WorkerTokens(BTreeMap<WorkerId, SecretString>)`) and compares it in constant time: `verify_slices_are_equal` on raw tokens still returns early on a length mismatch, so it leaks a configured token's length; storing digests also keeps the raw tokens out of the process after start-up. Reported to the backend's owners.
- **`X-Lease-Token`**: required on every call about a leased job (`events`, `complete`, the job's images; [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). The `JobLease` extractor reads it with `WorkerAuth` and the `{id}` capture, and the handler passes all three to the store, which checks them **in the same compare-and-set** that writes, so a requeue between the check and the write can't slip through ([database 4](../../../docs/database/README.md#job-claim-with-slots-)). A missing or empty header is `400 invalid_request` with `field: "X-Lease-Token"`; a mismatch, an expired or cancelled lease, or a job of an older run is `409 lease_lost` with `{ job_id }` ([contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers); [contracts 2](../../../docs/architecture/contracts.md#2-errors); [contracts 12, question 23](../../../docs/architecture/contracts.md#12-open-questions); [backend api 22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions)).
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
  - compare in constant time. [`aws_lc_rs::hmac::verify`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/hmac/fn.verify.html) computes the HMAC and compares it in constant time; the base64 decoding uses `data-encoding` 2.11.1's [`BASE64`](https://docs.rs/data-encoding/2.11.1/data_encoding/constant.BASE64.html) (standard alphabet, padded), as the backend's verifier does ([backend integrations 11](../../../docs/backend/integrations.md#11-svix-and-the-auth-webhook); [versions](../rust/versions.md)).
- Reject a `svix-timestamp` more than 5 minutes from now, in either direction, as contracts 9 says and as Svix's own libraries do: they "reject webhooks with a timestamp that are more than five minutes away (past or future)" ([Svix, why verify](https://docs.svix.com/receiving/verifying-payloads/why)).
- The signing secret's `whsec_` prefix is optional, as in the `svix` crate, and an empty key is refused at start-up ([backend integrations 11](../../../docs/backend/integrations.md#11-svix-and-the-auth-webhook)).
- Then map the body to an event and act on it ([contracts 9, auth webhook](../../../docs/architecture/contracts.md#auth-webhook); [contracts 12, question 24](../../../docs/architecture/contracts.md#12-open-questions); [backend api 22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions)):

| Delivery | Response | Why |
|---|---|---|
| A bad or missing signature, a missing header, or a timestamp more than 5 minutes away | `401 unauthenticated` | Svix redelivers; `invalid_signature` is the signed-URL `403`, not this |
| `user.deleted` | `204`, and the account cascade starts | Idempotent: the cascade starts with a compare-and-set on the account's tombstone, so a redelivery, or a `user.deleted` during a `DELETE /me` cascade, starts nothing |
| `user.created` | `204`, and **nothing else** | No upsert: Clerk doesn't guarantee event order, so a late `user.created` would recreate a deleted user. The first authenticated request creates the record (section 1) |
| An event type it doesn't handle, a `user.*` event without `data.id`, or a body it can't read after a good signature | `204`, the unreadable body logged | Svix treats anything but a `2xx` as a failure and retries it on its schedule (after 5 s, 5 min, 30 min, 2 h, 5 h, 10 h and 10 h), and an endpoint whose deliveries keep failing for 5 days is disabled ([Svix, retries](https://docs.svix.com/retries)). The same bytes will never parse, so a `400` would only burn those retries and count towards disabling the endpoint |

The test uses the example from Svix's manual-verification page (secret `whsec_plJ3nmyCDGBKInavdOK15jsl`, `msg_loFOjxBNrRLzqYUf`, timestamp `1731705121`, body `{"event_type":"ping","data":{"success":true}}`, signature `v1,rAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=`). The same signature comes out of Python's standard library, independently of this code: `base64.b64encode(hmac.new(base64.b64decode("plJ3nmyCDGBKInavdOK15jsl"), b"msg_loFOjxBNrRLzqYUf.1731705121." + body, hashlib.sha256).digest())`.

```rust
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use data_encoding::BASE64;

/// The Svix signing scheme (docs.svix.com, verifying webhooks manually).
pub struct SvixVerifier {
    key: aws_lc_rs::hmac::Key,
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the signing secret is empty")]
    Empty,
    #[error("the signing secret isn't base64")]
    Base64(#[from] data_encoding::DecodeError),
}

impl SvixVerifier {
    /// `secret`: the endpoint's signing secret, `whsec_<base64>`, from configuration. The
    /// prefix is optional, as in the `svix` crate (backend integrations 11).
    pub fn new(secret: &str) -> Result<Self, SecretError> {
        let encoded = secret.strip_prefix("whsec_").unwrap_or(secret);
        let bytes = BASE64.decode(encoded.as_bytes())?;
        if bytes.is_empty() {
            return Err(SecretError::Empty);
        }
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
            .filter_map(|encoded| BASE64.decode(encoded.as_bytes()).ok());
        let mut matched = false;
        for tag in tags {
            matched |= aws_lc_rs::hmac::verify(&self.key, &signed, &tag).is_ok();
        }
        matched
    }
}

/// 401 `unauthenticated`: the only failure the webhook answers (contracts 9).
#[derive(Debug, PartialEq, Eq)]
pub struct BadSignature;

/// Clerk's envelope, read leniently: unknown fields and types are ignored.
#[derive(Debug, serde::Deserialize)]
struct ClerkEvent {
    #[serde(rename = "type")]
    kind: String,
    data: ClerkData,
}

#[derive(Debug, serde::Deserialize)]
struct ClerkData {
    /// Optional in Clerk's schema for `user.deleted` (backend integrations 11).
    id: Option<String>,
}

/// contracts 9 `AuthWebhookEvent`: the two events the API maps.
#[derive(Debug, PartialEq, Eq)]
pub enum AuthWebhookEvent {
    UserCreated { auth_id: String },
    UserDeleted { auth_id: String },
}

/// What the handler does with a verified delivery.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// `user.created`, other types, unreadable bodies: acknowledge only.
    Nothing,
    /// `user.deleted`: start the account cascade (a no-op when one already runs).
    DeleteAccount { auth_id: String },
}

/// contracts 9: 5 minutes; Svix applies it to the past and the future.
const TOLERANCE_S: i64 = 5 * 60;

/// Checks the three Svix headers, the timestamp and the signature.
pub fn verify_request(
    verifier: &SvixVerifier,
    headers: &HeaderMap,
    body: &[u8],
    now_unix_s: i64,
) -> Result<(), BadSignature> {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let (Some(id), Some(timestamp), Some(signatures)) = (
        header("svix-id"),
        header("svix-timestamp"),
        header("svix-signature"),
    ) else {
        return Err(BadSignature);
    };
    let sent: i64 = timestamp.trim().parse().map_err(|_| BadSignature)?;
    if (now_unix_s - sent).abs() > TOLERANCE_S || !verifier.verify(id, timestamp, signatures, body)
    {
        return Err(BadSignature);
    }
    Ok(())
}

/// Verifies first, then maps. `Ok(None)`: a type the API doesn't handle, a `user.*`
/// event without `data.id`, or a signed body that doesn't parse (logged: Svix would
/// only redeliver the same bytes).
pub fn verified_event(
    verifier: &SvixVerifier,
    headers: &HeaderMap,
    body: &Bytes,
    now_unix_s: i64,
) -> Result<Option<AuthWebhookEvent>, BadSignature> {
    verify_request(verifier, headers, body, now_unix_s)?;
    let event: ClerkEvent = match serde_json::from_slice(body) {
        Ok(event) => event,
        Err(error) => {
            tracing::warn!(%error, "signed auth webhook body did not parse; ignored");
            return Ok(None);
        }
    };
    let Some(auth_id) = event.data.id else {
        return Ok(None);
    };
    Ok(match event.kind.as_str() {
        "user.created" => Some(AuthWebhookEvent::UserCreated { auth_id }),
        "user.deleted" => Some(AuthWebhookEvent::UserDeleted { auth_id }),
        _ => None,
    })
}

/// contracts 9: `401` for a failed check, `204` for everything else.
pub fn respond(result: Result<Option<AuthWebhookEvent>, BadSignature>) -> (StatusCode, Action) {
    match result {
        Err(BadSignature) => (StatusCode::UNAUTHORIZED, Action::Nothing),
        Ok(Some(AuthWebhookEvent::UserDeleted { auth_id })) => {
            (StatusCode::NO_CONTENT, Action::DeleteAccount { auth_id })
        }
        // `user.created` changes nothing: the first authenticated request makes the user.
        Ok(Some(AuthWebhookEvent::UserCreated { .. }) | None) => {
            (StatusCode::NO_CONTENT, Action::Nothing)
        }
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

    fn check(signatures: &str, body: &[u8], now: i64) -> Result<(), BadSignature> {
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
        assert_eq!(check(SIGNATURE, changed, TIMESTAMP), Err(BadSignature));
        let other = "v1,sAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=";
        assert_eq!(check(other, BODY, TIMESTAMP), Err(BadSignature));
        let unversioned = SIGNATURE.trim_start_matches("v1,");
        assert_eq!(check(unversioned, BODY, TIMESTAMP), Err(BadSignature));
    }

    #[test]
    fn timestamps_more_than_five_minutes_away_are_rejected() {
        assert_eq!(check(SIGNATURE, BODY, TIMESTAMP + 300), Ok(()));
        assert_eq!(check(SIGNATURE, BODY, TIMESTAMP - 300), Ok(()));
        for now in [TIMESTAMP + 301, TIMESTAMP - 301] {
            assert_eq!(check(SIGNATURE, BODY, now), Err(BadSignature));
        }
    }

    #[test]
    fn the_prefix_is_optional_and_an_empty_secret_is_refused() {
        let bare = SvixVerifier::new("plJ3nmyCDGBKInavdOK15jsl").unwrap();
        let signed = headers(ID, TIMESTAMP, SIGNATURE);
        assert_eq!(verify_request(&bare, &signed, BODY, TIMESTAMP), Ok(()));
        assert!(matches!(
            SvixVerifier::new("whsec_"),
            Err(SecretError::Empty)
        ));
    }

    /// Signs `body` as Svix would, with the test secret, as `msg_1` at `1000`.
    fn deliver(body: &'static [u8]) -> (StatusCode, Action) {
        let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let key_bytes = BASE64.decode(b"MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw").unwrap();
        let key = aws_lc_rs::hmac::Key::new(aws_lc_rs::hmac::HMAC_SHA256, &key_bytes);
        let signed = [b"msg_1.1000.".as_slice(), body].concat();
        let tag = aws_lc_rs::hmac::sign(&key, &signed);
        let signature = format!("v1,{}", BASE64.encode(tag.as_ref()));
        let verifier = SvixVerifier::new(secret).unwrap();
        let body = Bytes::from_static(body);
        respond(verified_event(
            &verifier,
            &headers("msg_1", 1000, &signature),
            &body,
            1000,
        ))
    }

    #[test]
    fn user_created_is_only_acknowledged() {
        let created = br#"{"type":"user.created","data":{"id":"user_2abc"}}"#;
        assert_eq!(deliver(created), (StatusCode::NO_CONTENT, Action::Nothing));
    }

    #[test]
    fn user_deleted_starts_the_account_cascade() {
        let deleted = br#"{"type":"user.deleted","data":{"id":"user_2abc","deleted":true}}"#;
        assert_eq!(
            deliver(deleted),
            (
                StatusCode::NO_CONTENT,
                Action::DeleteAccount {
                    auth_id: "user_2abc".to_owned()
                }
            )
        );
    }

    #[test]
    fn unreadable_or_unhandled_signed_bodies_are_204_so_svix_stops() {
        for body in [
            b"not json".as_slice(),
            br#"{"type":"session.created","data":{"id":"sess_1"}}"#,
            br#"{"type":"user.deleted","data":{}}"#,
        ] {
            assert_eq!(deliver(body), (StatusCode::NO_CONTENT, Action::Nothing));
        }
    }

    #[test]
    fn a_failed_check_is_401() {
        let verifier = SvixVerifier::new(SECRET).unwrap();
        let body = Bytes::from_static(BODY);
        let late = verified_event(
            &verifier,
            &headers(ID, TIMESTAMP, SIGNATURE),
            &body,
            TIMESTAMP + 301,
        );
        assert_eq!(respond(late), (StatusCode::UNAUTHORIZED, Action::Nothing));
        let unsigned = verified_event(&verifier, &HeaderMap::new(), &body, TIMESTAMP);
        assert_eq!(respond(unsigned).0, StatusCode::UNAUTHORIZED);
    }
}
```

## 4. Which routes get which check

Every check is an **extractor**, not a `route_layer` middleware: the proof a repository call needs comes out of the extractor, typed by the role the handler names, and a middleware can only put untyped values in the request's extensions, so it can't hand a typed proof to the handler ([backend api 9](../../../docs/backend/api.md#9-authentication-and-access); [backend domain 8](../../../docs/backend/domain.md#8-authorization-proofs)). An extractor runs only when a route matched, so an unknown path is still `404`, not `401`.

| Routes | Extractor or check | Notes |
|---|---|---|
| Everything the app calls | `AuthUser`: the Clerk session token (section 1), the user (refused for a tombstoned account), the per-user rate limit ([layers 5](layers.md#5-rate-limits)) | The first step of every other app extractor |
| A route whose `{id}` belongs to a garden | `Access<R, T>`: `AuthUser`, the `{id}` capture, and a `Membership<R>` proof from `domain::authz::check_membership` or `check_access` | `404` for a malformed ID, an unknown record, a record being deleted and a non-member alike; `403 forbidden` with `required_role` for a member below `R` |
| A garden ID in a body or query (`POST /scans`, `GET /plants?garden_id=`) | `AuthUser`, then `state.member::<R>(&user, garden)` in the handler, after the body is parsed | The same proof, the same `404` and `403` |
| Conversations, their messages and pending actions | `ConvAccess<R, T>`: owner **and** member, else `404` ([contracts 8, access](../../../docs/architecture/contracts.md#access)) | A `ConversationOwner<R>` proof |
| `GET /scans/{id}/images/{image_id}`, `GET /scans/{id}/masks/{mask_id}`, `GET /species/{id}/images/{hash}`, `GET /conversations/{id}/images/{image_id}` | The URL's `exp` and `sig` only | No `Authorization` header ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)); a bad signature is `403 invalid_signature` |
| `/internal/*` | `WorkerAuth`; `JobLease` (`WorkerAuth`, `{id}`, `X-Lease-Token`) per job call | Section 2; [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) |
| `POST /webhooks/auth` | The Svix signature, in the handler, on the raw `Bytes` | Section 3 |
| `GET /status`, `GET /healthz` | None | Return no user data ([contracts 9, ops](../../../docs/architecture/contracts.md#ops)) |

**`Membership<R>`** is `domain`'s proof that a user is a member of a garden with at least role `R` ([backend domain 8](../../../docs/backend/domain.md#8-authorization-proofs)). `R` is one of four sealed marker types (`Viewer`, `Reviewer`, `Editor`, `Owner`); the constructor is private to `domain` and the only functions that call it read the stored role through the `AccessRepo` port, so a handler gets a proof only by running the check. A repository method that writes takes `Membership<Editor>`, so a handler holding `Membership<Viewer>` doesn't compile (the `compile_fail` blocks below). `at_least` is the free compile-time downgrade (an owner's proof works where an editor's is asked for); `require` is the run-time upgrade for a route whose role depends on the request, such as removing a member (an owner, or the member themselves). The example cuts it down to one module with the same visibility rules.

Two Rust points the extractors rely on ([backend api 9, extractors and the state generic](../../../docs/backend/api.md#extractors-and-the-state-generic)):

- **`Arc<AppState>: FromRef<S>` and `S: Send + Sync`** on every impl, so an extractor works with any router state that can produce the `Arc` (axum's blanket `impl<T: Clone> FromRef<T> for T` covers the real router), and its future is `Send` ([`FromRef`](https://docs.rs/axum/0.8.9/axum/extract/trait.FromRef.html); [`FromRequestParts`](https://docs.rs/axum-core/0.5.6/axum_core/extract/trait.FromRequestParts.html), whose method returns `impl Future + Send`).
- **An extractor with nothing to await returns `std::future::ready(..)`**, written as `fn from_request_parts(..) -> impl Future<Output = ..> + Send`. An `async fn` impl without an `.await` trips Clippy's pedantic [`unused_async_trait_impl`](https://clippy.rust-lang.org/rust-1.99.0/index.html#unused_async_trait_impl) on 1.99, not `unused_async` (✓ reproduced on 1.99.1 with `-W clippy::pedantic`); an impl may refine the trait's declared return type this way ([`std::future::ready`](https://doc.rust-lang.org/std/future/fn.ready.html)). `AuthUser` below is such a stand-in; the real one awaits the verifier and the store.

The block copies `ApiError` ([error-responses 2](error-responses.md#2-the-compiled-example)), so the rejections are the real statuses.

```rust
use std::{collections::HashMap, future::Future, sync::Arc};

use axum::{
    Router,
    extract::{FromRef, FromRequestParts, Path},
    http::request::Parts,
    routing::get,
};

pub type LookupError = Box<dyn std::error::Error + Send + Sync>;
pub type UserId = String;
pub type GardenId = ulid::Ulid;

/// `domain::authz`, cut down. Only this module can build a `Membership`.
pub mod authz {
    use std::marker::PhantomData;

    use super::{GardenId, LookupError, Role, UserId};

    mod sealed {
        pub trait Sealed {}
    }

    /// A minimum role as a type: exactly the four markers below.
    pub trait MinRole: sealed::Sealed + Copy + std::fmt::Debug + Send + Sync + 'static {
        const ROLE: Role;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Viewer;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Reviewer;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Editor;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Owner;

    macro_rules! min_role {
        ($($marker:ident => $role:ident),*) => {
            $(
                impl sealed::Sealed for $marker {}
                impl MinRole for $marker {
                    const ROLE: Role = Role::$role;
                }
            )*
        };
    }
    min_role!(Viewer => Viewer, Reviewer => Reviewer, Editor => Editor, Owner => Owner);

    /// `Self` is at least `R`: implemented only where `Self::ROLE >= R::ROLE`.
    pub trait AtLeast<R: MinRole>: MinRole {}

    macro_rules! at_least {
        ($($hi:ty => [$($lo:ty),*]);* $(;)?) => {
            $($(impl AtLeast<$lo> for $hi {})*)*
        };
    }
    at_least! {
        Viewer => [Viewer];
        Reviewer => [Viewer, Reviewer];
        Editor => [Viewer, Reviewer, Editor];
        Owner => [Viewer, Reviewer, Editor, Owner];
    }

    /// Proof that `user` is a member of `garden` with at least role `R`. Private fields, a
    /// private constructor, no `Default`, no `Deserialize`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Membership<R: MinRole> {
        user: UserId,
        garden: GardenId,
        role: Role,
        min: PhantomData<fn() -> R>,
    }

    impl<R: MinRole> Membership<R> {
        const fn new(user: UserId, garden: GardenId, role: Role) -> Self {
            Self {
                user,
                garden,
                role,
                min: PhantomData,
            }
        }

        pub const fn garden(&self) -> GardenId {
            self.garden
        }

        pub const fn role(&self) -> Role {
            self.role
        }

        /// The compile-time downgrade.
        pub fn at_least<R2: MinRole>(self) -> Membership<R2>
        where
            R: AtLeast<R2>,
        {
            Membership::new(self.user, self.garden, self.role)
        }

        /// The run-time upgrade, for a role that depends on the request.
        pub fn require<R2: MinRole>(self) -> Result<Membership<R2>, AccessError> {
            if self.role >= R2::ROLE {
                Ok(Membership::new(self.user, self.garden, self.role))
            } else {
                Err(AccessError::Forbidden { required: R2::ROLE })
            }
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum AccessError {
        /// Not a member, or no such record: always `404`.
        #[error("not found")]
        NotFound,
        /// A member below the required role: `403` with `required_role`.
        #[error("requires the {required:?} role")]
        Forbidden { required: Role },
        #[error("access lookup failed")]
        Store(#[source] LookupError),
    }

    /// `domain::ports::AccessRepo`, cut down: the stored role, `None` for a non-member.
    #[async_trait::async_trait]
    pub trait AccessRepo: Send + Sync {
        async fn role_in(&self, user: &str, garden: GardenId) -> Result<Option<Role>, LookupError>;
    }

    /// The only way to a proof: read the role through the port, then check it.
    pub async fn check_membership<R: MinRole>(
        access: &dyn AccessRepo,
        user: &str,
        garden: GardenId,
    ) -> Result<Membership<R>, AccessError> {
        let role = access
            .role_in(user, garden)
            .await
            .map_err(AccessError::Store)?
            .ok_or(AccessError::NotFound)?;
        Membership::<Viewer>::new(user.to_owned(), garden, role).require()
    }
}

impl From<authz::AccessError> for ApiError {
    fn from(error: authz::AccessError) -> Self {
        match error {
            authz::AccessError::NotFound => Self::NotFound,
            authz::AccessError::Forbidden { required } => Self::Forbidden {
                required_role: required,
            },
            authz::AccessError::Store(source) => Self::UpstreamUnavailable {
                upstream: "database",
                retry_after_s: UNAVAILABLE_RETRY_AFTER_S,
                source,
            },
        }
    }
}

/// Ports only (backend api 6).
pub struct AppState {
    pub access: Arc<dyn authz::AccessRepo>,
}

/// Stand-in for `AuthUser` (section 1 has the real checks). It awaits nothing, so it
/// returns a ready future instead of being an `async fn` (`unused_async_trait_impl`).
pub struct AuthUser(pub UserId);

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let user = parts
            .headers
            .get("x-test-user")
            .and_then(|value| value.to_str().ok())
            .map(|user| Self(user.to_owned()))
            .ok_or(ApiError::Unauthenticated);
        std::future::ready(user)
    }
}

/// The caller and a proof of at least role `R` in the garden `{id}`.
pub struct Access<R: authz::MinRole> {
    pub user: UserId,
    pub member: authz::Membership<R>,
}

impl<S, R> FromRequestParts<S> for Access<R>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
    R: authz::MinRole,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        // A malformed ID is the same 404 as an unknown one, as `ApiPath` answers it.
        let Path(garden) = Path::<GardenId>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::NotFound)?;
        let app = Arc::<AppState>::from_ref(state);
        let member = authz::check_membership::<R>(&*app.access, &user, garden).await?;
        Ok(Self { user, member })
    }
}

/// A garden write: it compiles only with an editor's proof, or an owner's downgraded.
pub fn rename_garden(member: &authz::Membership<authz::Editor>, name: &str) -> String {
    format!("{} is now {name}", member.garden())
}

async fn read_garden(access: Access<authz::Viewer>) -> String {
    format!("{:?}", access.member.role())
}

async fn update_garden(access: Access<authz::Editor>) -> String {
    rename_garden(&access.member, "Back yard")
}

pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/gardens/{id}", get(read_garden).patch(update_garden))
        .with_state(state)
}

// copy of: rust-axum-api/error-responses.md ErrorCode
/// contracts 2 `ErrorCode`. In the domain crate; `ToSchema` adds it to the generated spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    InvalidSignature,
    NotFound,
    InvalidState,
    RequestInProgress,
    LeaseLost,
    UploadTooLarge,
    UnsupportedMediaType,
    ImageQuality,
    MissingRequiredShot,
    TooManyImages,
    IdempotencyConflict,
    RateLimited,
    BudgetExceeded,
    Internal,
    ProcessingUnavailable,
    UpstreamUnavailable,
}
// end: ErrorCode

// copy of: rust-axum-api/error-responses.md ApiError
/// contracts 1, roles: each role has every right of the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Reviewer,
    Editor,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityProblem {
    TooBlurry,
    TooDark,
    TooBright,
}

impl QualityProblem {
    /// contracts 2 `QualityHint`: one hint per problem.
    pub const fn hint(self) -> &'static str {
        match self {
            Self::TooBlurry => "tap_to_focus",
            Self::TooDark => "more_light",
            Self::TooBright => "avoid_glare",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::TooBlurry => {
                "This photo is too blurry. Hold the phone still and tap the leaf to focus."
            }
            Self::TooDark => "This photo is too dark. Move into better light.",
            Self::TooBright => {
                "This photo is too bright. Shade the plant or step out of direct sun."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateHint {
    NewScan,
    NeighbourRemoved,
    PlantRemoved,
}

/// contracts 2 `invalid_state` details; every key optional.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct InvalidStateDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_map_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<StateHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    ChatTokens,
    Scans,
}

/// contracts 2: the four upload formats.
pub const ACCEPTED_MEDIA_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/heic"];
/// contracts 2: `max_bytes` of `upload_too_large`, an image over 12 MB (read as MiB).
pub const IMAGE_MAX_BYTES: usize = 12 * 1024 * 1024;
/// contracts 2: `retry_after_s` of `upstream_unavailable` (also a request past the API's
/// timeout) unless the provider named a longer wait.
pub const UNAVAILABLE_RETRY_AFTER_S: u64 = 5;
/// contracts 2: `retry_after_s` of `request_in_progress` and `processing_unavailable`.
pub const SHORT_RETRY_AFTER_S: u64 = 2;

/// One variant per contracts 2 code. `Display` is the `message`: a sentence safe to show.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{problem}")]
    InvalidRequest { field: String, problem: String },
    #[error("Please sign in again.")]
    Unauthenticated,
    #[error("Ask an owner to change your role.")]
    Forbidden { required_role: Role },
    #[error("This link has expired.")]
    InvalidSignature,
    #[error("We couldn't find that.")]
    NotFound,
    #[error("This can't be done right now.")]
    InvalidState(InvalidStateDetails),
    #[error("This request is still being processed.")]
    RequestInProgress { retry_after_s: u64 },
    #[error("This job's lease is no longer valid.")]
    LeaseLost { job_id: String },
    #[error("This photo is too large to upload.")]
    UploadTooLarge { max_bytes: usize },
    #[error("This file type isn't supported. Use JPEG, PNG, WebP or HEIC.")]
    UnsupportedMediaType,
    #[error("{}", .problem.message())]
    ImageQuality {
        problem: QualityProblem,
        blur_score: f64,
        exposure: f64,
    },
    #[error("A photo of the whole plant is needed.")]
    MissingRequiredShot { missing: Vec<String> },
    #[error("There are already enough photos of this kind.")]
    TooManyImages { kind: String, max: u32 },
    #[error("This request doesn't match the earlier one with the same key.")]
    IdempotencyConflict,
    #[error("Too many requests. Try again in {retry_after_s} seconds.")]
    RateLimited { retry_after_s: u64 },
    #[error("You've reached today's limit.")]
    BudgetExceeded {
        budget: Budget,
        resets_at: Timestamp,
    },
    #[error("Something went wrong on our side.")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("We couldn't process this photo right now. It wasn't saved.")]
    ProcessingUnavailable {
        retry_after_s: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("A service we depend on is unavailable. Try again shortly.")]
    UpstreamUnavailable {
        upstream: &'static str,
        retry_after_s: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ApiError {
    pub fn invalid_request(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self::InvalidRequest {
            field: field.into(),
            problem: problem.into(),
        }
    }

    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } | Self::InvalidSignature => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::InvalidState(_) | Self::RequestInProgress { .. } | Self::LeaseLost { .. } => {
                StatusCode::CONFLICT
            }
            Self::UploadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::ImageQuality { .. }
            | Self::MissingRequiredShot { .. }
            | Self::TooManyImages { .. }
            | Self::IdempotencyConflict => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited { .. } | Self::BudgetExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ProcessingUnavailable { .. } | Self::UpstreamUnavailable { .. } => {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
    }

    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidRequest { .. } => ErrorCode::InvalidRequest,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::Forbidden { .. } => ErrorCode::Forbidden,
            Self::InvalidSignature => ErrorCode::InvalidSignature,
            Self::NotFound => ErrorCode::NotFound,
            Self::InvalidState(_) => ErrorCode::InvalidState,
            Self::RequestInProgress { .. } => ErrorCode::RequestInProgress,
            Self::LeaseLost { .. } => ErrorCode::LeaseLost,
            Self::UploadTooLarge { .. } => ErrorCode::UploadTooLarge,
            Self::UnsupportedMediaType => ErrorCode::UnsupportedMediaType,
            Self::ImageQuality { .. } => ErrorCode::ImageQuality,
            Self::MissingRequiredShot { .. } => ErrorCode::MissingRequiredShot,
            Self::TooManyImages { .. } => ErrorCode::TooManyImages,
            Self::IdempotencyConflict => ErrorCode::IdempotencyConflict,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::BudgetExceeded { .. } => ErrorCode::BudgetExceeded,
            Self::Internal(_) => ErrorCode::Internal,
            Self::ProcessingUnavailable { .. } => ErrorCode::ProcessingUnavailable,
            Self::UpstreamUnavailable { .. } => ErrorCode::UpstreamUnavailable,
        }
    }

    /// contracts 2: `true` for the codes it marks retryable, and for `image_quality`,
    /// whose example sends `true` (a retake can pass).
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RequestInProgress { .. }
                | Self::ImageQuality { .. }
                | Self::RateLimited { .. }
                | Self::ProcessingUnavailable { .. }
                | Self::UpstreamUnavailable { .. }
        )
    }

    /// contracts 2: sent with `request_in_progress`, `rate_limited` and both `503` codes.
    pub const fn retry_after_s(&self) -> Option<u64> {
        match self {
            Self::RequestInProgress { retry_after_s }
            | Self::RateLimited { retry_after_s }
            | Self::ProcessingUnavailable { retry_after_s, .. }
            | Self::UpstreamUnavailable { retry_after_s, .. } => Some(*retry_after_s),
            _ => None,
        }
    }

    pub fn details(&self) -> Option<serde_json::Value> {
        use serde_json::json;
        let details = match self {
            Self::InvalidRequest { field, problem } => {
                json!({ "field": field, "problem": problem })
            }
            Self::Forbidden { required_role } => json!({ "required_role": required_role }),
            Self::InvalidState(details) => json!(details),
            Self::LeaseLost { job_id } => json!({ "job_id": job_id }),
            Self::UploadTooLarge { max_bytes } => json!({ "max_bytes": max_bytes }),
            Self::UnsupportedMediaType => json!({ "accepted": ACCEPTED_MEDIA_TYPES }),
            Self::ImageQuality {
                problem,
                blur_score,
                exposure,
            } => json!({
                "problem": problem, "blur_score": blur_score,
                "exposure": exposure, "hint": problem.hint(),
            }),
            Self::MissingRequiredShot { missing } => json!({ "missing": missing }),
            Self::TooManyImages { kind, max } => json!({ "kind": kind, "max": max }),
            Self::BudgetExceeded { budget, resets_at } => {
                json!({ "budget": budget, "resets_at": resets_at })
            }
            Self::UpstreamUnavailable { upstream, .. } => json!({ "upstream": upstream }),
            Self::Unauthenticated
            | Self::InvalidSignature
            | Self::NotFound
            | Self::RequestInProgress { .. }
            | Self::IdempotencyConflict
            | Self::RateLimited { .. }
            | Self::Internal(_)
            | Self::ProcessingUnavailable { .. } => return None,
        };
        Some(details)
    }

    /// The source chain of a `5xx`, for the log line; never part of the body.
    fn cause(&self) -> Option<String> {
        match self {
            Self::Internal(source)
            | Self::ProcessingUnavailable { source, .. }
            | Self::UpstreamUnavailable { source, .. } => Some(Chain(&**source).to_string()),
            _ => None,
        }
    }

    pub fn parts(&self) -> ErrorParts {
        ErrorParts {
            code: self.code(),
            message: self.to_string(),
            retryable: self.retryable(),
            retry_after_s: self.retry_after_s(),
            details: self.details(),
            cause: self.cause(),
        }
    }
}

/// Everything of the body except `request_id`, left on the response for `render_errors`.
#[derive(Debug, Clone)]
pub struct ErrorParts {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub retry_after_s: Option<u64>,
    pub details: Option<serde_json::Value>,
    /// The source chain, logged by `render_errors`; set for `5xx` only.
    pub cause: Option<String>,
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut response = axum::response::IntoResponse::into_response(self.status());
        response.extensions_mut().insert(self.parts());
        response
    }
}
// end: ApiError

// copy of: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1,
/// wire format; contracts 12, question 12). Keeps full precision in memory and in the
/// database; only the wire form is truncated. No `now()`: every reading comes from the
/// `Clock` port, so tests control time (backend domain 4).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

// copy of: rust-errors/SKILL.md Chain
/// The log form of an error: its message, then each source's, joined by ": ".
/// Log it with `error = %Chain(&error)`; store it with `Chain(&error).to_string()`.
pub struct Chain<'a>(pub &'a (dyn std::error::Error + 'static));

impl std::fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(error) = source {
            write!(f, ": {error}")?;
            source = error.source();
        }
        Ok(())
    }
}
// end: Chain

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode};
    use tower::ServiceExt;

    const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5F";

    struct Roles(HashMap<(String, GardenId), Role>);

    #[async_trait::async_trait]
    impl authz::AccessRepo for Roles {
        async fn role_in(&self, user: &str, garden: GardenId) -> Result<Option<Role>, LookupError> {
            Ok(self.0.get(&(user.to_owned(), garden)).copied())
        }
    }

    fn state() -> Arc<AppState> {
        let garden: GardenId = GARDEN.parse().unwrap();
        let roles = [
            ("vic", Role::Viewer),
            ("eve", Role::Editor),
            ("olu", Role::Owner),
        ]
        .map(|(user, role)| ((user.to_owned(), garden), role));
        Arc::new(AppState {
            access: Arc::new(Roles(HashMap::from(roles))),
        })
    }

    async fn call(method: &str, uri: &str, user: Option<&str>) -> (StatusCode, Option<ErrorParts>) {
        let mut request = axum::http::Request::builder().method(method).uri(uri);
        if let Some(user) = user {
            request = request.header("x-test-user", user);
        }
        let response = app(state())
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        (
            response.status(),
            response.extensions().get::<ErrorParts>().cloned(),
        )
    }

    #[tokio::test]
    async fn the_route_names_its_role_and_the_extractor_checks_it() {
        let uri = format!("/gardens/{GARDEN}");
        assert_eq!(call("GET", &uri, Some("vic")).await.0, StatusCode::OK);
        assert_eq!(call("PATCH", &uri, Some("eve")).await.0, StatusCode::OK);
        // An owner's proof passes where an editor's is required.
        assert_eq!(call("PATCH", &uri, Some("olu")).await.0, StatusCode::OK);
        let (status, parts) = call("PATCH", &uri, Some("vic")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(parts.unwrap().details.unwrap()["required_role"], "editor");
    }

    #[tokio::test]
    async fn non_members_unknown_and_malformed_ids_all_get_the_same_404() {
        let other = "/gardens/01JAX3K9T6R2B8V0QZ4M7D1E5G";
        for (uri, user) in [
            (format!("/gardens/{GARDEN}"), "stranger"),
            (other.to_owned(), "olu"),
            ("/gardens/not-a-ulid".to_owned(), "olu"),
        ] {
            let (status, parts) = call("GET", &uri, Some(user)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri} as {user}");
            assert_eq!(parts.unwrap().code, ErrorCode::NotFound);
        }
        let uri = format!("/gardens/{GARDEN}");
        assert_eq!(call("GET", &uri, None).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn an_owner_downgrades_at_compile_time_and_a_viewer_upgrades_only_at_run_time() {
        let state = state();
        let garden: GardenId = GARDEN.parse().unwrap();
        let owner = authz::check_membership::<authz::Owner>(&*state.access, "olu", garden)
            .await
            .unwrap();
        let editor: authz::Membership<authz::Editor> = owner.at_least();
        assert!(rename_garden(&editor, "Plot 4").ends_with("is now Plot 4"));
        let viewer = authz::check_membership::<authz::Viewer>(&*state.access, "vic", garden)
            .await
            .unwrap();
        assert!(matches!(
            viewer.require::<authz::Editor>(),
            Err(authz::AccessError::Forbidden {
                required: Role::Editor
            })
        ));
    }
}
```

The proof's type is what makes "editor or above" a compile-time rule. A viewer's proof where an editor's is required is a type mismatch ([backend domain 8](../../../docs/backend/domain.md#8-authorization-proofs) has the same three checks as doc tests, and the three blocks below are them):

```rust,compile_fail,E0308
mod authz {
    use std::marker::PhantomData;

    pub struct Viewer;
    pub struct Editor;

    pub struct Membership<R> {
        min: PhantomData<fn() -> R>,
    }

    pub fn viewer() -> Membership<Viewer> {
        Membership { min: PhantomData }
    }
}

fn create_plant(_: authz::Membership<authz::Editor>) {}

fn main() {
    let proof = authz::viewer();
    create_plant(proof); // expected `Membership<Editor>`, found `Membership<Viewer>`
}
```

A downgrade is free, and an upgrade doesn't exist at compile time: `at_least` asks for `R: AtLeast<R2>`, which `Editor: AtLeast<Owner>` isn't:

```rust,compile_fail,E0277
mod authz {
    use std::marker::PhantomData;

    pub struct Editor;
    pub struct Owner;

    pub trait AtLeast<R> {}
    impl AtLeast<Editor> for Editor {}
    impl AtLeast<Editor> for Owner {}
    impl AtLeast<Owner> for Owner {}

    pub struct Membership<R> {
        min: PhantomData<fn() -> R>,
    }

    impl<R> Membership<R> {
        pub fn at_least<R2>(self) -> Membership<R2>
        where
            R: AtLeast<R2>,
        {
            Membership { min: PhantomData }
        }
    }
}

fn handler(proof: authz::Membership<authz::Editor>) -> authz::Membership<authz::Owner> {
    proof.at_least::<authz::Owner>() // `Editor: AtLeast<Owner>` is not satisfied
}

fn main() {}
```

And no crate outside `domain` can build one by hand, because the constructor is private:

```rust,compile_fail,E0624
mod authz {
    use std::marker::PhantomData;

    pub struct Owner;

    pub struct Membership<R> {
        min: PhantomData<fn() -> R>,
    }

    impl<R> Membership<R> {
        const fn new() -> Self {
            Self { min: PhantomData }
        }
    }
}

fn main() {
    let _forged: authz::Membership<authz::Owner> = authz::Membership::new();
}
```
