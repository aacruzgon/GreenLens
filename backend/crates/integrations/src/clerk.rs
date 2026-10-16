//! Clerk: session-token verification ([`AuthVerifier`]; [contracts 1], authentication headers;
//! architecture 4) and account deletion through the Backend API ([`AuthAdmin`]; [contracts 9],
//! account).
//!
//! **Verification.** RS256 only. The key comes from the instance's JWKS
//! (`{issuer}/.well-known/jwks.json`, the Frontend API: no secret needed, not rate limited),
//! built from configuration and never from the token's `iss` (no SSRF). Keys are cached by
//! `kid` for an hour. An unknown `kid` refetches the set, at most once every 30 s, and the
//! refetch is single-flight. Time comes from the caller's `now` (the `Clock` port), so
//! `jsonwebtoken`'s own `exp`/`nbf` checks, which read the system clock, are switched off and
//! done here with a 5 s leeway.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::{
    id::AuthId,
    ports::{AuthAdmin, AuthError, AuthVerifier, ProviderError, VerifiedToken},
    wire::Timestamp,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use moka::future::Cache;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tokio::{sync::Mutex, time::Instant};

use crate::http::{self, TransportError};

/// The `upstream` name of the Backend API.
pub const PROVIDER: &str = "clerk";

/// Verifier settings.
#[derive(Debug, Clone)]
pub struct ClerkVerifierConfig {
    /// The instance's Frontend API URL, which is the `iss` claim:
    /// `https://clerk.example.com`. Development and production differ.
    pub issuer: String,
    /// Web origins allowed in `azp`. Native apps send no `Origin`, so their tokens carry no
    /// `azp` and skip the check.
    pub allowed_azp: Vec<String>,
    /// Clock skew allowed on `exp` and `nbf`.
    pub leeway: Duration,
    /// How long a key stays cached.
    pub key_ttl: Duration,
    /// The least time between two JWKS fetches.
    pub min_refetch: Duration,
    /// JWKS fetch deadline.
    pub timeout: Duration,
}

impl ClerkVerifierConfig {
    /// The defaults for an issuer: 5 s leeway, 1 h key cache, 30 s between fetches, 5 s timeout.
    pub fn new(issuer: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into().trim_end_matches('/').to_owned(),
            allowed_azp: Vec::new(),
            leeway: Duration::from_secs(5),
            key_ttl: Duration::from_hours(1),
            min_refetch: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
        }
    }
}

/// The claims PDM reads from a Clerk session token (v2). Unknown claims are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClerkClaims {
    /// The user id: `user_…`.
    pub sub: String,
    /// The session id.
    #[serde(default)]
    pub sid: Option<String>,
    /// The Frontend API URL.
    pub iss: String,
    /// Expiry, Unix seconds.
    pub exp: i64,
    /// Not before, Unix seconds.
    pub nbf: i64,
    /// The requesting origin; absent for native apps.
    #[serde(default)]
    pub azp: Option<String>,
    /// Session status (`pending` with organisations, unused by PDM).
    #[serde(default)]
    pub sts: Option<String>,
    /// A display name, when a custom session-token claim adds it.
    #[serde(default)]
    pub name: Option<String>,
}

/// Verifies session tokens. One per process; `Clone` shares the key cache.
#[derive(Clone)]
pub struct ClerkVerifier {
    http: reqwest::Client,
    config: Arc<ClerkVerifierConfig>,
    validation: Arc<Validation>,
    keys: Cache<String, Arc<DecodingKey>>,
    /// When the JWKS was last fetched; the mutex makes concurrent misses single-flight.
    last_fetch: Arc<Mutex<Option<Instant>>>,
}

impl std::fmt::Debug for ClerkVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClerkVerifier")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ClerkVerifier {
    /// A verifier over the shared client.
    pub fn new(http: reqwest::Client, config: ClerkVerifierConfig) -> Self {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&config.issuer]);
        validation.set_required_spec_claims(&["exp", "nbf", "iss", "sub"]);
        validation.validate_aud = false; // session tokens carry no `aud`
        validation.validate_exp = false; // checked below against the injected `now`
        validation.validate_nbf = false;
        let keys = Cache::builder()
            .max_capacity(16)
            .time_to_live(config.key_ttl)
            .build();
        Self {
            http,
            config: Arc::new(config),
            validation: Arc::new(validation),
            keys,
            last_fetch: Arc::new(Mutex::new(None)),
        }
    }

    /// Seeds a key, for tests and for a PEM pinned in configuration.
    pub async fn insert_key(&self, kid: impl Into<String>, key: DecodingKey) {
        self.keys.insert(kid.into(), Arc::new(key)).await;
    }

    async fn key_for(&self, kid: &str) -> Result<Arc<DecodingKey>, AuthError> {
        if let Some(k) = self.keys.get(kid).await {
            return Ok(k);
        }
        let mut last = self.last_fetch.lock().await;
        if let Some(k) = self.keys.get(kid).await {
            return Ok(k); // another request refetched while this one waited
        }
        if last.is_some_and(|t| t.elapsed() < self.config.min_refetch) {
            return Err(AuthError::InvalidToken(Some("unknown kid".into())));
        }
        let url = format!("{}/.well-known/jwks.json", self.config.issuer);
        let response = self
            .http
            .get(url)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| AuthError::Unavailable(Box::new(TransportError::from_reqwest(e))))?;
        if !response.status().is_success() {
            return Err(AuthError::Unavailable(
                format!("JWKS answered {}", response.status()).into(),
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AuthError::Unavailable(Box::new(TransportError::from_reqwest(e))))?;
        let set: JwkSet =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::Unavailable(Box::new(e)))?;
        *last = Some(Instant::now());
        for jwk in &set.keys {
            if let (Some(id), Ok(key)) = (jwk.common.key_id.clone(), DecodingKey::from_jwk(jwk)) {
                self.keys.insert(id, Arc::new(key)).await;
            }
        }
        self.keys
            .get(kid)
            .await
            .ok_or_else(|| AuthError::InvalidToken(Some("unknown kid".into())))
    }

    /// Checks the time claims and `azp` of decoded claims at `now`.
    pub fn check_claims(&self, claims: &ClerkClaims, now: Timestamp) -> Result<(), AuthError> {
        let leeway = i64::try_from(self.config.leeway.as_secs()).unwrap_or(0);
        let now = now.unix_seconds();
        if claims.exp + leeway <= now {
            return Err(AuthError::InvalidToken(Some("expired".into())));
        }
        if claims.nbf - leeway > now {
            return Err(AuthError::InvalidToken(Some("not yet valid".into())));
        }
        if let Some(azp) = &claims.azp
            && !self.config.allowed_azp.iter().any(|o| o == azp)
        {
            return Err(AuthError::InvalidToken(Some("azp not allowed".into())));
        }
        Ok(())
    }
}

#[async_trait]
impl AuthVerifier for ClerkVerifier {
    async fn verify(&self, bearer: &str, now: Timestamp) -> Result<VerifiedToken, AuthError> {
        let invalid = |e: jsonwebtoken::errors::Error| AuthError::InvalidToken(Some(Box::new(e)));
        let header = decode_header(bearer).map_err(invalid)?;
        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken(Some("no kid".into())))?;
        let key = self.key_for(&kid).await?;
        let claims = decode::<ClerkClaims>(bearer, &key, &self.validation)
            .map_err(invalid)?
            .claims;
        self.check_claims(&claims, now)?;
        let auth_id = claims
            .sub
            .parse::<AuthId>()
            .map_err(|e| AuthError::InvalidToken(Some(Box::new(e))))?;
        let expires_at = chrono::DateTime::from_timestamp(claims.exp, 0)
            .map(Timestamp::from)
            .ok_or_else(|| AuthError::InvalidToken(Some("exp out of range".into())))?;
        Ok(VerifiedToken {
            auth_id,
            display_name: claims.name,
            expires_at,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Backend API
// ---------------------------------------------------------------------------------------------

/// Backend API settings.
#[derive(Debug, Clone)]
pub struct ClerkAdminConfig {
    /// `https://api.clerk.com/v1`.
    pub base_url: String,
    /// The instance secret key, `sk_…`. Never logged.
    pub secret_key: SecretString,
    /// Deadline per call.
    pub timeout: Duration,
}

/// Deletes accounts at Clerk, the last step of account deletion. The caller (the deletion
/// cascade) retries until it succeeds, so one call is one attempt.
#[derive(Debug, Clone)]
pub struct ClerkAdmin {
    http: reqwest::Client,
    config: Arc<ClerkAdminConfig>,
}

impl ClerkAdmin {
    /// An adapter over the shared client.
    pub fn new(http: reqwest::Client, config: ClerkAdminConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }
}

#[async_trait]
impl AuthAdmin for ClerkAdmin {
    async fn delete_account(&self, auth_id: &AuthId) -> Result<(), ProviderError> {
        let url = format!(
            "{}/users/{}",
            self.config.base_url.trim_end_matches('/'),
            path_segment(auth_id.as_str())
        );
        let response = self
            .http
            .delete(url)
            .bearer_auth(self.config.secret_key.expose_secret())
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| ProviderError::Unavailable {
                provider: PROVIDER,
                source: Box::new(TransportError::from_reqwest(e)),
            })?;
        let status = response.status();
        match status.as_u16() {
            200..=299 | 404 => Ok(()), // 404: already gone, so retries are idempotent
            429 => Err(ProviderError::RateLimited {
                provider: PROVIDER,
                retry_after_s: http::retry_after(response.headers())
                    .and_then(|d| u32::try_from(d.as_secs()).ok()),
            }),
            500..=599 => Err(ProviderError::Unavailable {
                provider: PROVIDER,
                source: format!("HTTP {status}").into(),
            }),
            // 400, 401, 403: configuration (a wrong secret key); retrying won't help, alert.
            _ => Err(ProviderError::BadResponse {
                provider: PROVIDER,
                source: format!("HTTP {status}").into(),
            }),
        }
    }
}

/// Percent-encodes a path segment (user ids are `user_` + alphanumerics, but be defensive).
fn path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push_str(&data_encoding::HEXUPPER.encode(&[b]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use chrono::{TimeZone as _, Utc};
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::{Value, json};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    use super::*;

    const NOW: i64 = 1_792_000_000;

    /// A throwaway RSA key pair, made once per test run so no private key is committed:
    /// the private half as PKCS#8 PEM and the public half as the instance's JWKS.
    static KEYS: LazyLock<(String, String)> = LazyLock::new(|| {
        use aws_lc_rs::{
            encoding::AsDer as _,
            rsa::{KeyPair, KeySize},
            signature::KeyPair as _,
        };
        use data_encoding::{BASE64, BASE64URL_NOPAD};

        let pair = KeyPair::generate(KeySize::Rsa2048).unwrap();
        let der = BASE64.encode(pair.as_der().unwrap().as_ref());
        let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
        for line in der.as_bytes().chunks(64) {
            pem.push_str(std::str::from_utf8(line).unwrap());
            pem.push('\n');
        }
        pem.push_str("-----END PRIVATE KEY-----\n");
        let public = pair.public_key();
        let jwks = json!({"keys": [{
            "kty": "RSA", "alg": "RS256", "use": "sig", "kid": "ins_test",
            "n": BASE64URL_NOPAD.encode(public.modulus().big_endian_without_leading_zero()),
            "e": BASE64URL_NOPAD.encode(public.exponent().big_endian_without_leading_zero()),
        }]});
        (pem, jwks.to_string())
    });

    fn encoding_key() -> EncodingKey {
        EncodingKey::from_rsa_pem(KEYS.0.as_bytes()).unwrap()
    }

    fn now() -> Timestamp {
        Timestamp::from(Utc.timestamp_opt(NOW, 0).unwrap())
    }

    fn token(iss: &str, edit: impl FnOnce(&mut Value)) -> String {
        let mut claims = json!({
            "sub": "user_1", "sid": "sess_1", "iss": iss,
            "iat": NOW, "nbf": NOW - 5, "exp": NOW + 60, "v": 2
        });
        edit(&mut claims);
        let mut h = Header::new(Algorithm::RS256);
        h.kid = Some("ins_test".into());
        encode(&h, &claims, &encoding_key()).unwrap()
    }

    async fn verifier(server: &MockServer) -> ClerkVerifier {
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(KEYS.1.as_str()))
            .mount(server)
            .await;
        let mut config = ClerkVerifierConfig::new(server.uri());
        config.allowed_azp = vec!["https://app.example.test".into()];
        ClerkVerifier::new(reqwest::Client::new(), config)
    }

    #[tokio::test]
    async fn verifies_and_caches_the_jwks() {
        let server = MockServer::start().await;
        let v = verifier(&server).await;
        let t = token(&server.uri(), |_| {});
        let ok = v.verify(&t, now()).await.unwrap();
        assert_eq!(ok.auth_id.as_str(), "user_1");
        assert_eq!(ok.expires_at.unix_seconds(), NOW + 60);
        v.verify(&t, now()).await.unwrap();
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "one JWKS fetch"
        );
    }

    #[tokio::test]
    async fn rejects_bad_claims() {
        let server = MockServer::start().await;
        let v = verifier(&server).await;
        let iss = server.uri();
        let cases = [
            token("https://evil.example", |_| {}),
            token(&iss, |c| c["exp"] = json!(NOW - 10)),
            token(&iss, |c| c["nbf"] = json!(NOW + 60)),
            token(&iss, |c| c["azp"] = json!("https://evil.example")),
            token(&iss, |c| {
                c.as_object_mut().unwrap().remove("nbf");
            }),
        ];
        for t in cases {
            assert!(matches!(
                v.verify(&t, now()).await,
                Err(AuthError::InvalidToken(_))
            ));
        }
        let allowed = token(&iss, |c| c["azp"] = json!("https://app.example.test"));
        assert!(v.verify(&allowed, now()).await.is_ok());
        let leeway = token(&iss, |c| c["exp"] = json!(NOW - 3));
        assert!(
            v.verify(&leeway, now()).await.is_ok(),
            "within the 5 s leeway"
        );
    }

    /// Real time, not paused: with paused time tokio would auto-advance to the request's
    /// timeout while the mock server answers.
    #[tokio::test]
    async fn unknown_kid_refetch_is_rate_limited() {
        let server = MockServer::start().await;
        let v = verifier(&server).await;
        let mut h = Header::new(Algorithm::RS256);
        h.kid = Some("rotated".into());
        let t = encode(
            &h,
            &json!({"sub": "user_1", "iss": server.uri(), "nbf": NOW, "exp": NOW + 60}),
            &encoding_key(),
        )
        .unwrap();
        for _ in 0..3 {
            assert!(matches!(
                v.verify(&t, now()).await,
                Err(AuthError::InvalidToken(_))
            ));
        }
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn jwks_down_is_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let v = ClerkVerifier::new(
            reqwest::Client::new(),
            ClerkVerifierConfig::new(server.uri()),
        );
        let t = token(&server.uri(), |_| {});
        assert!(matches!(
            v.verify(&t, now()).await,
            Err(AuthError::Unavailable(_))
        ));
    }

    fn admin(server: &MockServer) -> ClerkAdmin {
        ClerkAdmin::new(
            reqwest::Client::new(),
            ClerkAdminConfig {
                base_url: format!("{}/v1", server.uri()),
                secret_key: SecretString::from("sk_test_x"),
                timeout: Duration::from_secs(5),
            },
        )
    }

    #[tokio::test]
    async fn delete_treats_404_as_done() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/users/user_1"))
            .and(header("authorization", "Bearer sk_test_x"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"object": "user", "id": "user_1", "deleted": true})),
            )
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/users/user_2"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/users/user_3"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "10"))
            .mount(&server)
            .await;
        let a = admin(&server);
        assert!(a.delete_account(&"user_1".parse().unwrap()).await.is_ok());
        assert!(a.delete_account(&"user_2".parse().unwrap()).await.is_ok());
        assert!(matches!(
            a.delete_account(&"user_3".parse().unwrap()).await,
            Err(ProviderError::RateLimited {
                retry_after_s: Some(10),
                ..
            })
        ));
    }
}
