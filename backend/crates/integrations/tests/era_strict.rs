//! The clients' OAuth exchanges against Era's simulators in `AUTH_MODE=strict` (docs/EON T-E3.5,
//! E6.4): Slack's `/oauth/v2/authorize` then `oauth.v2.access`; Google's `/o/oauth2/v2/auth` with
//! PKCE then `/token`, a refresh, a reused code, and Drive's scope check. Ignored by default:
//!
//! ```sh
//! docker run -d -p 127.0.0.1:8291:8080 -e SIMCORE_SEED=42 -e SIMCORE_TODAY=2026-10-15 \
//!   -e SIMCORE_FIDELITY=high -e AUTH_MODE=strict <the era-slack image in infra/dev.compose.yml>
//! # the same with the era-drive image on 8292
//! ERA_STRICT_SLACK_BASE=http://127.0.0.1:8291 ERA_STRICT_GOOGLE_BASE=http://127.0.0.1:8292 \
//!   cargo nextest run -p integrations --run-ignored only -E 'test(/era_strict_/)'
//! ```
//!
//! The strict pair holds the generated company (no fixture is injected), and its credentials
//! carry no tenant, so the tests read the default one. Nothing here sets faults.

use integrations::{
    era_sim::{ERA_TIMEOUT, SimKind, strict_base_url},
    farm::FarmClientError,
    google_drive::{DriveClient, DriveConfig, Pkce},
    http::{self, ExposeSecret, HttpConfig, SecretString},
    slack::{SlackClient, SlackConfig},
};
use reqwest::Url;

const REDIRECT_URI: &str = "https://api.example/connectors/oauth/callback";
const DRIVE_READONLY: &str = "https://www.googleapis.com/auth/drive.readonly";

fn no_redirects() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("the HTTP client builds")
}

/// Follows the consent step by hand: the simulator answers `302` to `redirect_uri` with `code`
/// and the `state` it was given.
async fn code_from(url: String, query: &[(&str, &str)], state: &str) -> String {
    let consent = no_redirects()
        .get(url)
        .query(query)
        .send()
        .await
        .expect("the simulator answers");
    assert!(consent.status().is_redirection(), "{}", consent.status());
    let location = consent.headers()["location"]
        .to_str()
        .expect("a text header")
        .to_owned();
    let back = Url::parse(&location).expect("a URL");
    assert!(location.starts_with(REDIRECT_URI), "{location}");
    let pairs: Vec<_> = back.query_pairs().collect();
    assert!(
        pairs.iter().any(|(k, v)| k == "state" && v == state),
        "{location}"
    );
    pairs
        .iter()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .expect("a code in the redirect")
}

fn slack(base: &str) -> SlackClient {
    let http = http::client(&HttpConfig::default()).expect("the HTTP client builds");
    let config = SlackConfig {
        timeout: ERA_TIMEOUT,
        ..SlackConfig::era(base)
    };
    SlackClient::new(http, config)
}

fn drive(base: &str) -> DriveClient {
    DriveClient::new(
        reqwest::Client::new(),
        DriveConfig {
            api_base: base.to_owned(),
            token_url: format!("{base}/token"),
            revoke_url: format!("{base}/o/oauth2/revoke"),
            timeout: ERA_TIMEOUT,
            ..DriveConfig::default()
        },
    )
}

/// The consent step with PKCE and `access_type=offline`; returns the code.
async fn drive_code(base: &str, scope: &str, pkce: &Pkce) -> String {
    code_from(
        format!("{base}/o/oauth2/v2/auth"),
        &[
            ("client_id", "pdm-test"),
            ("redirect_uri", REDIRECT_URI),
            ("response_type", "code"),
            ("scope", scope),
            ("state", "g1"),
            ("code_challenge", pkce.challenge()),
            ("code_challenge_method", Pkce::METHOD),
            ("access_type", "offline"),
        ],
        "g1",
    )
    .await
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_strict_slack_code_exchange_then_a_reused_code_is_refused() {
    let base = strict_base_url(SimKind::Slack);
    let code = code_from(
        format!("{base}/oauth/v2/authorize"),
        &[
            ("client_id", "pdm-test"),
            ("scope", "channels:read,channels:history,users:read"),
            ("redirect_uri", REDIRECT_URI),
            ("state", "s1"),
        ],
        "s1",
    )
    .await;

    let client = slack(&base);
    let secret = SecretString::from("client-secret");
    let access = client
        .oauth_v2_access(&code, REDIRECT_URI, "pdm-test", &secret)
        .await
        .unwrap();
    assert!(access.access_token.expose_secret().starts_with("xoxb-"));
    assert!(!format!("{access:?}").contains(access.access_token.expose_secret()));
    assert!(
        access.scope.contains("channels:history"),
        "{}",
        access.scope
    );
    assert_ne!(access.team_id, "");
    // The strict token is accepted by the Web API.
    let auth = client.auth_test(&access.access_token).await.unwrap();
    assert_eq!(auth.team_id, access.team_id);

    // Slack refuses a spent code at HTTP 200 with `invalid_code`, which is `AuthRefused`: a bad user code.
    let error = client
        .oauth_v2_access(&code, REDIRECT_URI, "pdm-test", &secret)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "invalid_code"),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_strict_slack_an_unknown_code_is_refused() {
    let client = slack(&strict_base_url(SimKind::Slack));
    let error = client
        .oauth_v2_access(
            "not-a-code",
            REDIRECT_URI,
            "pdm-test",
            &SecretString::from("client-secret"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "invalid_code"),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_strict_drive_pkce_exchange_refresh_and_a_reused_code() {
    let base = strict_base_url(SimKind::Drive);
    let client = drive(&base);
    let secret = SecretString::from("client-secret");

    let pkce = Pkce::generate();
    let code = drive_code(&base, DRIVE_READONLY, &pkce).await;
    let grant = client
        .exchange_code(&code, pkce.verifier(), REDIRECT_URI, "pdm-test", &secret)
        .await
        .unwrap();
    assert_eq!(grant.scope.as_deref(), Some(DRIVE_READONLY));
    assert_eq!(grant.expires_in.as_secs(), 3600);
    let refresh = grant
        .refresh_token
        .expect("access_type=offline gives a refresh token");
    client.about_email(&grant.access_token).await.unwrap();

    // A refresh gives a working access token.
    let refreshed = client.refresh(&refresh, "pdm-test", &secret).await.unwrap();
    client.about_email(&refreshed.access_token).await.unwrap();

    // A code works once; the second exchange is `invalid_grant`.
    let error = client
        .exchange_code(&code, pkce.verifier(), REDIRECT_URI, "pdm-test", &secret)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "invalid_grant"),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_strict_drive_a_wrong_pkce_verifier_is_refused() {
    let base = strict_base_url(SimKind::Drive);
    let pkce = Pkce::generate();
    let code = drive_code(&base, DRIVE_READONLY, &pkce).await;
    let other = Pkce::generate();
    let error = drive(&base)
        .exchange_code(
            &code,
            other.verifier(),
            REDIRECT_URI,
            "pdm-test",
            &SecretString::from("client-secret"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "invalid_grant"),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_strict_drive_readonly_lists_a_folder_and_another_scope_is_refused() {
    let base = strict_base_url(SimKind::Drive);
    let client = drive(&base);
    let secret = SecretString::from("client-secret");

    let pkce = Pkce::generate();
    let code = drive_code(&base, DRIVE_READONLY, &pkce).await;
    let readonly = client
        .exchange_code(&code, pkce.verifier(), REDIRECT_URI, "pdm-test", &secret)
        .await
        .unwrap();
    client
        .list_folder(&readonly.access_token, "root")
        .await
        .expect("a drive.readonly token lists a folder");

    let pkce = Pkce::generate();
    let code = drive_code(&base, "https://www.googleapis.com/auth/calendar", &pkce).await;
    let other = client
        .exchange_code(&code, pkce.verifier(), REDIRECT_URI, "pdm-test", &secret)
        .await
        .unwrap();
    let error = client
        .list_folder(&other.access_token, "root")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "insufficientPermissions"),
        "{error:?}"
    );
}
