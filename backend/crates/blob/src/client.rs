//! The S3 client ([storage 5]).

use std::time::Duration;

use aws_sdk_s3::{
    Client,
    config::{BehaviorVersion, Builder, Credentials, Region, timeout::TimeoutConfig},
};

/// The client's per-attempt operation timeout ([storage 4]: "the client's 120-second operation
/// timeout", a last resort for a hung call; never a shorter task timeout around a delete).
pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

/// How the API reaches `RustFS` ([storage 1], [storage 2]).
#[derive(Debug, Clone)]
pub struct BlobConfig {
    /// `http://rustfs:9000` on the Docker network.
    pub endpoint: String,
    /// `PDMAPI01`.
    pub access_key: String,
    /// Its secret, from the environment.
    pub secret_key: String,
    /// `pdm-images`.
    pub bucket: String,
    /// Must match `RUSTFS_REGION`: `us-east-1`.
    pub region: String,
}

/// Storage 5's client, with the behaviour version pinned to `v2026_01_12` instead of
/// `latest()`: `latest()` moves with the SDK, and `v2026_01_12` turns on retries by default and a
/// 3.1-second connect timeout (research `http-s3.md` §9), so an SDK upgrade can't change retry or
/// timeout behaviour silently. Static credentials skip the default provider chain (no IMDS or
/// environment lookups); path-style addressing is needed unless `RUSTFS_SERVER_DOMAINS` is set.
/// Each attempt has a 120-second operation timeout ([`OPERATION_TIMEOUT`], storage 4).
/// One client per process: it is `Clone + Send + Sync + 'static` and pools its connections.
pub fn storage_client(config: &BlobConfig) -> Client {
    Client::from_conf(
        Builder::new()
            .behavior_version(BehaviorVersion::v2026_01_12())
            .region(Region::new(config.region.clone()))
            .endpoint_url(&config.endpoint)
            .credentials_provider(Credentials::new(
                &config.access_key,
                &config.secret_key,
                None,
                None,
                "rustfs",
            ))
            .force_path_style(true)
            .timeout_config(
                TimeoutConfig::builder()
                    .operation_attempt_timeout(OPERATION_TIMEOUT)
                    .build(),
            )
            .build(),
    )
}
