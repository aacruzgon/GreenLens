//! Typed configuration, read once from the environment at start-up ([architecture 12]).
//! Secrets are `SecretString`s: their `Debug` prints `[REDACTED]`, so logging the config never
//! leaks one, and reading one takes an explicit `expose_secret()`.

use std::{collections::BTreeMap, net::SocketAddr, str::FromStr, time::Duration};

use domain::{
    id::{GardenId, WorkerId},
    rules::weather::WeatherThresholds,
};
use secrecy::{ExposeSecret, SecretString};

use crate::sealing::ConnectorKey;

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

/// `SurrealDB` Agent Memory, where shared memories are mirrored (architecture 7, memory). All
/// three variables or none.
#[derive(Debug, Clone)]
pub struct AgentMemorySettings {
    /// `AGENT_MEMORY_URL`: the service's origin.
    pub url: String,
    /// `AGENT_MEMORY_CONTEXT_ID`: the context every call belongs to.
    pub context_id: String,
    /// `AGENT_MEMORY_API_KEY`.
    pub api_key: SecretString,
}

/// The three Agent Memory variables, set together or not at all.
const AGENT_MEMORY_VARS: [&str; 3] = [
    "AGENT_MEMORY_URL",
    "AGENT_MEMORY_CONTEXT_ID",
    "AGENT_MEMORY_API_KEY",
];

/// A vendor's OAuth client for the farm connectors ([connectors 5]): both variables or neither.
///
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
#[derive(Debug, Clone)]
pub struct OAuthClient {
    /// `SLACK_CLIENT_ID` or `GOOGLE_CLIENT_ID`: public, it goes in the consent URL.
    pub client_id: String,
    /// `SLACK_CLIENT_SECRET` or `GOOGLE_CLIENT_SECRET`: sent only in the code exchange.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the code exchange, roadmap E3.4")
    )]
    pub client_secret: SecretString,
}

/// The two Slack variables, set together or not at all.
const SLACK_VARS: [&str; 2] = ["SLACK_CLIENT_ID", "SLACK_CLIENT_SECRET"];
/// The two Google variables, set together or not at all.
const GOOGLE_VARS: [&str; 2] = ["GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"];

/// Farm connectors ([connectors 4], [connectors 5]). `key` is the one switch: without
/// `CONNECTOR_KEY` connectors are off (decision 9 of docs/EON), whatever else is set.
///
/// [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
#[derive(Debug, Clone)]
pub struct ConnectorSettings {
    /// `CONNECTOR_KEY`: 64 hex characters (32 bytes, `openssl rand -hex 32`), the AES-256-GCM
    /// key that seals vendor tokens. Unset turns connectors off.
    pub key: Option<ConnectorKey>,
    /// `SLACK_CLIENT_ID` and `SLACK_CLIENT_SECRET`.
    pub slack: Option<OAuthClient>,
    /// `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`: a Web application client.
    pub google: Option<OAuthClient>,
    /// `CONNECTOR_RETURN_URL` (default `pdm://connectors`): where the OAuth callback's `302`
    /// sends the app, as `{return_url}/{connector id}/callback?code&state`. The callback
    /// itself is `{API_PUBLIC_BASE}/connectors/oauth/callback`.
    pub return_url: String,
    /// `ERA_SLACK_BASE`: Era's Slack simulator, for start-up seeding of the demo garden's
    /// `era` connectors (roadmap E4.6). Never used by a route (decision 8).
    pub era_slack_base: Option<String>,
    /// `ERA_GOOGLE_BASE`: Era's Google Drive simulator, as above.
    pub era_google_base: Option<String>,
    /// `ERA_DEMO_GARDEN_ID`: the demo garden (E9.0 creates it through the API, so its id is
    /// known only after) that start-up seeding gives the two `era` connectors.
    pub era_demo_garden: Option<GardenId>,
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
    /// `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and `AGENT_MEMORY_API_KEY`: shared
    /// memories are mirrored into Agent Memory when all three are set; none set turns it off.
    pub agent_memory: Option<AgentMemorySettings>,
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
    /// Farm connectors.
    pub connectors: ConnectorSettings,
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
                bucket: env.or("RUSTFS_BUCKET", "pdm-images"),
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
                agent_memory: agent_memory(&env)?,
            },
            connectors: connectors(&env)?,
        })
    }
}

/// Whether `url` may be the app's return target: `https`, or a custom app scheme, never a
/// scheme a browser would run or read; no query or fragment, and something after `://`.
fn return_url_ok(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    let scheme_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+.-".contains(c));
    scheme_ok
        && !["http", "javascript", "data", "file", "vbscript"].contains(&scheme)
        && !rest.is_empty()
        && !url.contains(['?', '#'])
}

/// The connector settings; a partial vendor pair names its first missing variable.
fn connectors<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
) -> Result<ConnectorSettings, ConfigError> {
    let key = env
        .get("CONNECTOR_KEY")
        .map(|value| {
            ConnectorKey::from_hex(&value).map_err(|e| ConfigError::Invalid {
                name: "CONNECTOR_KEY",
                problem: e.to_string(),
            })
        })
        .transpose()?;
    let return_url = env
        .or("CONNECTOR_RETURN_URL", "pdm://connectors")
        .trim()
        .trim_end_matches('/')
        .to_owned();
    if !return_url_ok(&return_url) {
        return Err(ConfigError::Invalid {
            name: "CONNECTOR_RETURN_URL",
            problem: "expected an https URL or an app scheme (not http, javascript, data, file or vbscript), with no query, such as pdm://connectors"
                .to_owned(),
        });
    }
    Ok(ConnectorSettings {
        key,
        slack: oauth_client(env, SLACK_VARS)?,
        google: oauth_client(env, GOOGLE_VARS)?,
        return_url,
        era_slack_base: http_base(env, "ERA_SLACK_BASE")?,
        era_google_base: http_base(env, "ERA_GOOGLE_BASE")?,
        era_demo_garden: env
            .get("ERA_DEMO_GARDEN_ID")
            .map(|value| {
                value.trim().parse().map_err(|_| ConfigError::Invalid {
                    name: "ERA_DEMO_GARDEN_ID",
                    problem: "expected a garden id (a ULID)".to_owned(),
                })
            })
            .transpose()?,
    })
}

/// A vendor's client id and secret, both or neither; a partial pair names the missing one.
fn oauth_client<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
    [id_var, secret_var]: [&'static str; 2],
) -> Result<Option<OAuthClient>, ConfigError> {
    match (env.get(id_var), env.get(secret_var)) {
        (None, None) => Ok(None),
        (Some(id), Some(secret)) => Ok(Some(OAuthClient {
            client_id: id.trim().to_owned(),
            client_secret: SecretString::from(secret),
        })),
        (id, _) => Err(ConfigError::Invalid {
            name: if id.is_none() { id_var } else { secret_var },
            problem: format!("{id_var} and {secret_var} are set together or not at all"),
        }),
    }
}

/// An optional `http(s)://` origin, without its trailing slash.
fn http_base<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
    name: &'static str,
) -> Result<Option<String>, ConfigError> {
    env.get(name)
        .map(|value| {
            let value = value.trim().trim_end_matches('/');
            if value.starts_with("http://") || value.starts_with("https://") {
                Ok(value.to_owned())
            } else {
                Err(ConfigError::Invalid {
                    name,
                    problem: "expected an http:// or https:// URL".to_owned(),
                })
            }
        })
        .transpose()
}

/// All three Agent Memory variables, or none; a partial set names the first missing one.
fn agent_memory<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
) -> Result<Option<AgentMemorySettings>, ConfigError> {
    let [url, context, key] = AGENT_MEMORY_VARS.map(|name| env.get(name));
    match (url, context, key) {
        (None, None, None) => Ok(None),
        (Some(url), Some(context_id), Some(key)) => Ok(Some(AgentMemorySettings {
            url: url.trim().trim_end_matches('/').to_owned(),
            context_id: context_id.trim().to_owned(),
            api_key: SecretString::from(key),
        })),
        _ => {
            let missing = AGENT_MEMORY_VARS
                .into_iter()
                .find(|name| env.get(name).is_none())
                .unwrap_or(AGENT_MEMORY_VARS[0]);
            Err(ConfigError::Invalid {
                name: missing,
                problem: "AGENT_MEMORY_URL, AGENT_MEMORY_CONTEXT_ID and AGENT_MEMORY_API_KEY \
                          are set together or not at all"
                    .to_owned(),
            })
        }
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
    fn agent_memory_is_off_without_its_variables() {
        let vars = base();
        let config = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        assert!(config.providers.agent_memory.is_none());
    }

    #[test]
    fn agent_memory_takes_all_three_variables() {
        let mut vars = base();
        vars.insert("AGENT_MEMORY_URL", "https://memory.example.org/".to_owned());
        vars.insert("AGENT_MEMORY_CONTEXT_ID", "pdm-dev".to_owned());
        vars.insert("AGENT_MEMORY_API_KEY", "am_secret_key".to_owned());
        let config = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        let memory = config.providers.agent_memory.as_ref().unwrap();
        assert_eq!(memory.url, "https://memory.example.org");
        assert_eq!(memory.context_id, "pdm-dev");
        assert_eq!(memory.api_key.expose_secret(), "am_secret_key");
        assert!(!format!("{config:?}").contains("am_secret_key"));
    }

    #[test]
    fn a_partial_agent_memory_set_is_refused() {
        let mut vars = base();
        vars.insert("AGENT_MEMORY_URL", "https://memory.example.org".to_owned());
        vars.insert("AGENT_MEMORY_API_KEY", "am_secret_key".to_owned());
        let err = Config::from_lookup(|k| vars.get(k).cloned()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "AGENT_MEMORY_CONTEXT_ID",
                ..
            }
        ));
        assert!(err.to_string().contains("set together or not at all"));
    }

    const CONNECTOR_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn load(vars: &BTreeMap<&'static str, String>) -> Result<Config, ConfigError> {
        Config::from_lookup(|k| vars.get(k).cloned())
    }

    #[test]
    fn connectors_are_off_without_their_key() {
        let mut vars = base();
        vars.insert("SLACK_CLIENT_ID", "123.456".to_owned());
        vars.insert("SLACK_CLIENT_SECRET", "slack-secret".to_owned());
        let config = load(&vars).unwrap();
        assert!(config.connectors.key.is_none());
        assert_eq!(config.connectors.return_url, "pdm://connectors");
        assert!(config.connectors.google.is_none());
        assert!(config.connectors.era_slack_base.is_none());
        assert!(config.connectors.era_demo_garden.is_none());
    }

    #[test]
    fn a_demo_garden_id_that_is_not_a_ulid_is_refused() {
        let mut vars = base();
        vars.insert("ERA_DEMO_GARDEN_ID", "maple-row".to_owned());
        assert!(matches!(
            load(&vars),
            Err(ConfigError::Invalid {
                name: "ERA_DEMO_GARDEN_ID",
                ..
            })
        ));
    }

    #[test]
    fn every_connector_setting_is_read() {
        let mut vars = base();
        vars.insert("CONNECTOR_KEY", CONNECTOR_KEY.to_owned());
        vars.insert("SLACK_CLIENT_ID", "123.456".to_owned());
        vars.insert("SLACK_CLIENT_SECRET", "slack-secret".to_owned());
        vars.insert(
            "GOOGLE_CLIENT_ID",
            "abc.apps.googleusercontent.com".to_owned(),
        );
        vars.insert("GOOGLE_CLIENT_SECRET", "google-secret".to_owned());
        vars.insert(
            "CONNECTOR_RETURN_URL",
            "https://app.example.org/connectors/".to_owned(),
        );
        vars.insert("ERA_SLACK_BASE", "http://era-slack:8080/".to_owned());
        vars.insert("ERA_GOOGLE_BASE", "http://era-google:8080".to_owned());
        vars.insert(
            "ERA_DEMO_GARDEN_ID",
            "01JAX3K9T6R2B8V0QZ4M7D1E5G".to_owned(),
        );
        let config = load(&vars).unwrap();
        let c = &config.connectors;
        assert_eq!(
            c.era_demo_garden.map(|g| g.to_string()).as_deref(),
            Some("01JAX3K9T6R2B8V0QZ4M7D1E5G")
        );
        assert!(c.key.is_some());
        let slack = c.slack.as_ref().unwrap();
        assert_eq!(slack.client_id, "123.456");
        assert_eq!(slack.client_secret.expose_secret(), "slack-secret");
        let google = c.google.as_ref().unwrap();
        assert_eq!(google.client_secret.expose_secret(), "google-secret");
        assert_eq!(c.return_url, "https://app.example.org/connectors");
        assert_eq!(c.era_slack_base.as_deref(), Some("http://era-slack:8080"));
        assert_eq!(c.era_google_base.as_deref(), Some("http://era-google:8080"));
        let printed = format!("{config:?}");
        for secret in ["slack-secret", "google-secret", CONNECTOR_KEY] {
            assert!(!printed.contains(secret), "{secret}");
        }
    }

    #[test]
    fn a_partial_vendor_pair_names_its_missing_variable() {
        for (set, missing) in [
            ("SLACK_CLIENT_ID", "SLACK_CLIENT_SECRET"),
            ("SLACK_CLIENT_SECRET", "SLACK_CLIENT_ID"),
            ("GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"),
            ("GOOGLE_CLIENT_SECRET", "GOOGLE_CLIENT_ID"),
        ] {
            let mut vars = base();
            vars.insert("CONNECTOR_KEY", CONNECTOR_KEY.to_owned());
            vars.insert(set, "value".to_owned());
            let err = load(&vars).unwrap_err();
            assert!(
                matches!(err, ConfigError::Invalid { name, .. } if name == missing),
                "{set}: {err}"
            );
            assert!(
                err.to_string().starts_with(&format!("{missing}: ")),
                "{err}"
            );
            assert!(err.to_string().contains("set together or not at all"));
        }
    }

    #[test]
    fn a_malformed_connector_key_is_refused_without_quoting_it() {
        for bad in [
            &CONNECTOR_KEY[..62],
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            "change-me",
        ] {
            let mut vars = base();
            vars.insert("CONNECTOR_KEY", bad.to_owned());
            let err = load(&vars).unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::Invalid {
                        name: "CONNECTOR_KEY",
                        ..
                    }
                ),
                "{bad}: {err}"
            );
            assert!(!err.to_string().contains(bad), "{err}");
        }
    }

    #[test]
    fn connector_urls_are_checked() {
        let mut vars = base();
        vars.insert("CONNECTOR_RETURN_URL", "pdm://connectors?x=1".to_owned());
        assert!(matches!(
            load(&vars),
            Err(ConfigError::Invalid {
                name: "CONNECTOR_RETURN_URL",
                ..
            })
        ));
        for bad in [
            "http://app.example.org",
            "javascript://x",
            "data://x",
            "file:///etc",
            "vbscript://x",
            "HTTPS://x",
            "1pdm://x",
            "pdm://",
            "pdm://a#b",
            "no-scheme",
        ] {
            let mut vars = base();
            vars.insert("CONNECTOR_RETURN_URL", bad.to_owned());
            assert!(
                matches!(
                    load(&vars),
                    Err(ConfigError::Invalid {
                        name: "CONNECTOR_RETURN_URL",
                        ..
                    })
                ),
                "{bad}"
            );
        }
        for good in [
            "https://app.example.org/c",
            "pdm://connectors",
            "exp+pdm://x",
        ] {
            let mut vars = base();
            vars.insert("CONNECTOR_RETURN_URL", good.to_owned());
            assert!(load(&vars).is_ok(), "{good}");
        }
        let mut vars = base();
        vars.insert("ERA_GOOGLE_BASE", "era-google:8080".to_owned());
        assert!(matches!(
            load(&vars),
            Err(ConfigError::Invalid {
                name: "ERA_GOOGLE_BASE",
                ..
            })
        ));
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
