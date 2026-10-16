//! Typed configuration, read once from the environment at start-up ([architecture 12]).
//! Secrets are `SecretString`s: their `Debug` prints `[REDACTED]`, so logging the config never
//! leaks one, and reading one takes an explicit `expose_secret()`.

use std::{collections::BTreeMap, net::SocketAddr, str::FromStr, time::Duration};

use domain::{id::WorkerId, rules::weather::WeatherThresholds};
use secrecy::{ExposeSecret, SecretString};

/// Why the configuration couldn't be read. Start-up turns it into an `anyhow` error and exits.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is unset or empty.
    #[error("{0} is required")]
    Missing(&'static str),
    /// A variable doesn't parse.
    #[error("{name}: {problem}")]
    Invalid {
        /// The variable.
        name: &'static str,
        /// What is wrong with it.
        problem: String,
    },
}

/// `SurrealDB` connection ([database 1], [database 6]).
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// `SURREAL_URL`, such as `ws://surrealdb:8000`. The namespace and database are the
    /// schema's own (`pdm` / `main`, `store::client`).
    pub url: String,
    /// `SURREAL_USER`: the database-level user `api` runs as.
    pub user: String,
    /// `SURREAL_PASS`.
    pub password: SecretString,
    /// `SURREAL_ROOT_USER`: applies `schema.surql` (idempotent) on every start, over a
    /// short-lived connection; `api` can't run its `DEFINE NAMESPACE` itself.
    pub root_user: String,
    /// `SURREAL_ROOT_PASS`.
    pub root_password: SecretString,
}

/// `RustFS` connection ([storage 5]).
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// `RUSTFS_ENDPOINT`.
    pub endpoint: String,
    /// `RUSTFS_BUCKET`.
    pub bucket: String,
    /// `RUSTFS_ACCESS_KEY`.
    pub access_key: String,
    /// `RUSTFS_SECRET_KEY`.
    pub secret_key: SecretString,
}

/// Clerk ([frontend 8], what the backend needs from Clerk).
#[derive(Debug, Clone)]
pub struct ClerkConfig {
    /// `CLERK_ISSUER`: the instance's Frontend API URL, the `iss` claim; the JWKS is
    /// `{issuer}/.well-known/jwks.json`.
    pub issuer: String,
    /// `CLERK_SECRET_KEY`: Backend API key, for account deletion.
    pub secret_key: SecretString,
    /// `CLERK_WEBHOOK_SECRET`: the Svix endpoint secret (`whsec_…`).
    pub webhook_secret: SecretString,
}

/// Outbound providers, handed to the `integrations` and `agent` constructors.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// `LLM_API_KEY`.
    pub llm_api_key: SecretString,
    /// `LLM_CHAT_MODEL`: chat and plan writing ([architecture 10]); the client's default
    /// when unset.
    pub llm_chat_model: Option<String>,
    /// `LLM_CHEAP_MODEL`: summaries, memory extraction, off-topic replies, the classifier
    /// fallback; the client's default when unset.
    pub llm_cheap_model: Option<String>,
    /// `JEV_API_KEY` and `JEV_URL`: the optional text classifier ([architecture 7]); the LLM
    /// fallback when unset.
    pub jev: Option<(String, SecretString)>,
    /// `EXPO_ACCESS_TOKEN`: only when enhanced push security is on.
    pub expo_access_token: Option<SecretString>,
    /// `OPEN_METEO_API_KEY`: only for Open-Meteo's commercial endpoint.
    pub open_meteo_api_key: Option<SecretString>,
}

/// Everything `api` reads from the environment. Built by [`Config::from_env`].
#[derive(Debug, Clone)]
pub struct Config {
    /// `API_BIND`.
    pub bind: SocketAddr,
    /// `API_PUBLIC_BASE`: the origin prefixed to every signed image URL.
    pub public_base: String,
    /// `API_WEB_ORIGINS`: the web build's origins, for CORS and Clerk's `azp`.
    pub web_origins: Vec<String>,
    /// `API_URL_SIGNING_KEY`: at least 32 bytes, the HMAC key of signed URLs ([storage 3]).
    pub url_signing_key: SecretString,
    /// `WORKER_TOKENS`: `worker_id=token` pairs, comma separated ([contracts 1]).
    pub worker_tokens: BTreeMap<WorkerId, SecretString>,
    /// `MODELS_DIR`: the face detector and the text embedder (`vision`).
    pub models_dir: String,
    /// `UPLOAD_CPU_PERMITS`: concurrent upload pipelines on `spawn_blocking`.
    pub upload_permits: usize,
    /// `MEASURE_CPU_PERMITS`: concurrent measurement stages (the pipeline's).
    pub measure_permits: usize,
    /// `REQUEST_TIMEOUT_S`: the response-head timeout of ordinary routes.
    pub request_timeout: Duration,
    /// `RATE_PER_MINUTE`: the general per-user request limit.
    pub rate_per_minute: u32,
    /// `SHUTDOWN_DRAIN_S`: how long shutdown waits for background tasks.
    pub shutdown_drain: Duration,
    /// `DIAGNOSIS_MARGIN`: the fusion margin of contracts 7 (open question 3).
    pub diagnosis_margin: i32,
    /// `LOG_FORMAT`: `json` (default) or `pretty`.
    pub log_json: bool,
    /// The weather thresholds: the one instance handed to Open-Meteo (`frost_risk`), the
    /// pipeline (fusion) and the scheduler (triggers). The defaults; not read from the
    /// environment.
    pub weather: WeatherThresholds,
    /// The database.
    pub database: DatabaseConfig,
    /// The object store.
    pub storage: StorageConfig,
    /// The auth provider.
    pub clerk: ClerkConfig,
    /// Outbound providers.
    pub providers: ProviderConfig,
}

/// One variable source: the process environment, or a map in tests.
struct Env<F: Fn(&str) -> Option<String>>(F);

impl<F: Fn(&str) -> Option<String>> Env<F> {
    fn get(&self, name: &'static str) -> Option<String> {
        (self.0)(name).filter(|v| !v.trim().is_empty())
    }

    fn required(&self, name: &'static str) -> Result<String, ConfigError> {
        self.get(name).ok_or(ConfigError::Missing(name))
    }

    fn or(&self, name: &'static str, default: &str) -> String {
        self.get(name).unwrap_or_else(|| default.to_owned())
    }

    fn secret(&self, name: &'static str) -> Result<SecretString, ConfigError> {
        self.required(name).map(SecretString::from)
    }

    fn parse<T: FromStr>(&self, name: &'static str, default: T) -> Result<T, ConfigError>
    where
        T::Err: std::fmt::Display,
    {
        self.get(name).map_or(Ok(default), |v| {
            v.trim().parse().map_err(|e: T::Err| ConfigError::Invalid {
                name,
                problem: e.to_string(),
            })
        })
    }
}

impl Config {
    /// Reads the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads any variable source; tests pass a map.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let env = Env(lookup);
        let issuer = env.required("CLERK_ISSUER")?;
        let url_signing_key = env.secret("API_URL_SIGNING_KEY")?;
        if url_signing_key.expose_secret().len() < 32 {
            return Err(ConfigError::Invalid {
                name: "API_URL_SIGNING_KEY",
                problem: "at least 32 bytes".to_owned(),
            });
        }
        Ok(Self {
            bind: env.parse("API_BIND", SocketAddr::from(([0, 0, 0, 0], 8080)))?,
            public_base: env
                .required("API_PUBLIC_BASE")?
                .trim_end_matches('/')
                .to_owned(),
            web_origins: env
                .or("API_WEB_ORIGINS", "")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            url_signing_key,
            worker_tokens: parse_worker_tokens(&env.secret("WORKER_TOKENS")?)?,
            models_dir: env.or("MODELS_DIR", "/models"),
            upload_permits: env.parse("UPLOAD_CPU_PERMITS", 2)?,
            measure_permits: env.parse("MEASURE_CPU_PERMITS", 2)?,
            request_timeout: Duration::from_secs(env.parse("REQUEST_TIMEOUT_S", 10)?),
            rate_per_minute: env.parse("RATE_PER_MINUTE", 120)?,
            shutdown_drain: Duration::from_secs(env.parse("SHUTDOWN_DRAIN_S", 30)?),
            diagnosis_margin: env.parse("DIAGNOSIS_MARGIN", 2)?,
            log_json: env.or("LOG_FORMAT", "json") != "pretty",
            weather: WeatherThresholds::default(),
            database: DatabaseConfig {
                url: env.or("SURREAL_URL", "ws://surrealdb:8000"),
                user: env.or("SURREAL_USER", "api"),
                password: env.secret("SURREAL_PASS")?,
                root_user: env.or("SURREAL_ROOT_USER", "root"),
                root_password: env.secret("SURREAL_ROOT_PASS")?,
            },
            storage: StorageConfig {
                endpoint: env.or("RUSTFS_ENDPOINT", "http://rustfs:9000"),
                bucket: env.or("RUSTFS_BUCKET", "pdm"),
                access_key: env.required("RUSTFS_ACCESS_KEY")?,
                secret_key: env.secret("RUSTFS_SECRET_KEY")?,
            },
            clerk: ClerkConfig {
                issuer,
                secret_key: env.secret("CLERK_SECRET_KEY")?,
                webhook_secret: env.secret("CLERK_WEBHOOK_SECRET")?,
            },
            providers: ProviderConfig {
                llm_api_key: env.secret("LLM_API_KEY")?,
                llm_chat_model: env.get("LLM_CHAT_MODEL"),
                llm_cheap_model: env.get("LLM_CHEAP_MODEL"),
                jev: match env.get("JEV_API_KEY") {
                    Some(key) => Some((env.required("JEV_URL")?, SecretString::from(key))),
                    None => None,
                },
                expo_access_token: env.get("EXPO_ACCESS_TOKEN").map(SecretString::from),
                open_meteo_api_key: env.get("OPEN_METEO_API_KEY").map(SecretString::from),
            },
        })
    }
}

/// `rig-3070-perception-1=tok1,fallback-1=tok2`.
fn parse_worker_tokens(
    raw: &SecretString,
) -> Result<BTreeMap<WorkerId, SecretString>, ConfigError> {
    let invalid = |problem: String| ConfigError::Invalid {
        name: "WORKER_TOKENS",
        problem,
    };
    let mut tokens = BTreeMap::new();
    for pair in raw.expose_secret().split(',').map(str::trim) {
        let (id, token) = pair
            .split_once('=')
            .ok_or_else(|| invalid("expected worker_id=token pairs".to_owned()))?;
        let id: WorkerId = id.parse().map_err(|e| invalid(format!("{e}")))?;
        if token.len() < 32 {
            return Err(invalid(format!(
                "the token of {id} is shorter than 32 bytes"
            )));
        }
        tokens.insert(id, SecretString::from(token.to_owned()));
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> BTreeMap<&'static str, String> {
        let long = "x".repeat(40);
        BTreeMap::from([
            ("CLERK_ISSUER", "https://clerk.example.org/".to_owned()),
            ("API_URL_SIGNING_KEY", long.clone()),
            ("API_PUBLIC_BASE", "https://api.example.org/".to_owned()),
            ("WORKER_TOKENS", format!("rig-3070-perception-1={long}")),
            ("SURREAL_PASS", "p".to_owned()),
            ("SURREAL_ROOT_PASS", "r".to_owned()),
            ("RUSTFS_ACCESS_KEY", "a".to_owned()),
            ("RUSTFS_SECRET_KEY", "s".to_owned()),
            ("CLERK_SECRET_KEY", "sk".to_owned()),
            ("CLERK_WEBHOOK_SECRET", "whsec_x".to_owned()),
            ("LLM_API_KEY", "k".to_owned()),
            ("LLM_CHAT_MODEL", "big".to_owned()),
            ("LLM_CHEAP_MODEL", "small".to_owned()),
        ])
    }

    #[test]
    fn defaults_and_derived_values() {
        let vars = base();
        let config = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        assert_eq!(config.bind.port(), 8080);
        assert_eq!(config.public_base, "https://api.example.org");
        assert_eq!(config.request_timeout, Duration::from_secs(10));
        assert_eq!(config.worker_tokens.len(), 1);
        assert!(!format!("{config:?}").contains(&"x".repeat(40)));
    }

    #[test]
    fn short_signing_key_is_refused() {
        let mut vars = base();
        vars.insert("API_URL_SIGNING_KEY", "short".to_owned());
        assert!(matches!(
            Config::from_lookup(|k| vars.get(k).cloned()),
            Err(ConfigError::Invalid {
                name: "API_URL_SIGNING_KEY",
                ..
            })
        ));
    }
}
