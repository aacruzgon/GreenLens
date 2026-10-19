//! `SlackClient` against Era's Slack simulator (docs/EON T-E2.1, T-E2.3, T-E2.5): the Maple Row
//! Farm fixture read back through the client, the refusals Slack sends at HTTP 200, the code
//! exchange, and the simulator's faults. Ignored by default:
//!
//! ```sh
//! docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
//! ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
//!   cargo nextest run -p integrations --run-ignored only -E 'test(/era/) & binary(era_slack)'
//! ```
//!
//! Faults are global to the simulator. The nextest group `era` runs every `era_` test one at a
//! time, so no test sees another's fault. A fault test clears the faults when it ends, also
//! when it fails.

use std::{
    future::Future,
    panic::AssertUnwindSafe,
    time::{Duration, Instant},
};

use futures::FutureExt as _;
use integrations::{
    era_sim::{Faults, SimKind, Tenant, base_url, fixture},
    farm::{FarmClientError, FarmErrorCode},
    http::{self, ExposeSecret, HttpConfig, RetryPolicy, SecretString},
    slack::{SlackClient, SlackConfig, SlackMessage, permalink},
};
use serde_json::{Value, json};

fn client(base: &str, edit: impl FnOnce(&mut SlackConfig)) -> SlackClient {
    let mut config = SlackConfig::era(base);
    edit(&mut config);
    let http = http::client(&HttpConfig::default()).expect("the HTTP client builds");
    SlackClient::new(http, config)
}

async fn farm() -> (Tenant, SecretString) {
    let tenant = Tenant::create(SimKind::Slack, Some(&fixture("maple-row", "slack.json"))).await;
    let token = SecretString::from(tenant.token().await);
    (tenant, token)
}

/// Every message of a channel, newest first, and the number of pages it took.
async fn history(
    slack: &SlackClient,
    token: &SecretString,
    team: &str,
    channel: &str,
    oldest: Option<&str>,
) -> (Vec<SlackMessage>, usize) {
    let mut messages = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = slack
            .conversations_history(token, team, channel, oldest, cursor.as_deref())
            .await
            .unwrap_or_else(|e| panic!("history of {channel}: {e:?}"));
        pages += 1;
        messages.extend(page.messages);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return (messages, pages);
        }
    }
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_slack_reads_the_maple_row_farm() {
    let (tenant, token) = farm().await;
    let slack = client(tenant.base(), |c| c.list_limit = 5);

    let auth = slack.auth_test(&token).await.unwrap();
    assert!(
        !auth.team.is_empty() && !auth.team_id.is_empty(),
        "{auth:?}"
    );
    assert!(auth.url.ends_with('/'), "{auth:?}");
    assert_eq!(auth.user_id, "user-bot");

    // Only member channels, across pages of 5.
    let channels = slack.conversations_list(&token).await.unwrap();
    for (id, name) in [("C_FIELD", "field-reports"), ("C_CREW", "crew")] {
        let channel = channels
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} not listed: {channels:?}"));
        assert_eq!(channel.name, name);
    }
    let journal = tenant.requests().await;
    let list_calls = journal
        .iter()
        .filter(|r| r.route == "/api/conversations.list" && r.status == 200)
        .count();
    assert!(list_calls >= 2, "one page only: {list_calls}");

    // History by cursor, Tier 1 pages of 15, newest first.
    let (field, pages) = history(&slack, &token, &auth.team_id, "C_FIELD", None).await;
    assert_eq!(field.len(), 55);
    assert_eq!(pages, 4);
    let (crew, _) = history(&slack, &token, &auth.team_id, "C_CREW", None).await;
    assert_eq!(crew.len(), 39);
    let ts: Vec<f64> = field.iter().map(|m| m.ts.parse().unwrap()).collect();
    assert!(ts.windows(2).all(|w| w[0] > w[1]), "newest first");

    // Since a `ts`: only the newer messages (`oldest` is exclusive).
    let oldest = field[10].ts.clone();
    let (newer, _) = history(&slack, &token, &auth.team_id, "C_FIELD", Some(&oldest)).await;
    assert_eq!(
        newer.iter().map(|m| &m.ts).collect::<Vec<_>>(),
        field[..10].iter().map(|m| &m.ts).collect::<Vec<_>>()
    );

    // Threads: the parent, then its replies; 29 replies over 20 threads.
    let (mut threads, mut replies) = (0, 0);
    for (channel, messages) in [("C_FIELD", &field), ("C_CREW", &crew)] {
        for parent in messages.iter().filter(|m| m.reply_count > 0) {
            assert!(parent.latest_reply.is_some(), "{parent:?}");
            let thread = slack
                .conversations_replies(&token, &auth.team_id, channel, &parent.ts)
                .await
                .unwrap();
            assert_eq!(thread[0].ts, parent.ts);
            assert_eq!(
                thread.len(),
                usize::try_from(parent.reply_count).unwrap() + 1
            );
            assert!(thread[1..].iter().all(SlackMessage::is_reply));
            assert_eq!(thread.last().map(|m| &m.ts), parent.latest_reply.as_ref());
            threads += 1;
            replies += thread.len() - 1;
        }
    }
    assert_eq!((threads, replies), (20, 29));

    // The author's names, never the email the simulator sends.
    let maria = slack.users_info(&token, "U_MARIA").await.unwrap();
    assert_eq!(maria.display_name.as_deref(), Some("Maria"));
    assert_eq!(maria.real_name.as_deref(), Some("Maria Lopez"));
    assert_eq!(maria.label(), "Maria");
    assert!(!format!("{maria:?}").contains('@'));

    // Links from `auth.test`'s URL (the simulator's is always http://127.0.0.1:8080/).
    let newest = &field[0].ts;
    assert_eq!(
        permalink(&auth.url, "C_FIELD", newest, None),
        format!("{}archives/C_FIELD/p{}", auth.url, newest.replace('.', ""))
    );

    tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_slack_refusals_at_http_200_are_auth_refused() {
    let slack = client(&base_url(SimKind::Slack), |_| {});
    // No token at all: `not_authed`. Open mode accepts any `xoxb-` token, so the wrong token
    // is one without the prefix: `invalid_auth`.
    for (token, expected) in [("", "not_authed"), ("not-a-slack-token", "invalid_auth")] {
        let error = slack
            .auth_test(&SecretString::from(token))
            .await
            .unwrap_err();
        assert!(
            matches!(&error, FarmClientError::AuthRefused { vendor: "slack", code } if code == expected),
            "{token:?}: {error:?}"
        );
        assert_eq!(error.code(), FarmErrorCode::AuthRefused);
    }
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_slack_a_missing_channel_is_not_found() {
    let (tenant, token) = farm().await;
    let slack = client(tenant.base(), |_| {});
    let error = slack
        .conversations_history(&token, "T", "C_NOPE", None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::NotFound { code, .. } if code == "channel_not_found"),
        "{error:?}"
    );
    tenant.close().await;
}

/// Decision 2: the OAuth exchange is tested at the client level. Open mode redirects straight
/// back with a code, as strict mode does.
#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_slack_oauth_code_exchange() {
    let base = base_url(SimKind::Slack);
    let redirect_uri = "https://api.example/connectors/oauth/callback";
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let consent = browser
        .get(format!("{base}/oauth/v2/authorize"))
        .query(&[
            ("client_id", "pdm-test"),
            ("scope", "channels:read,channels:history,users:read"),
            ("redirect_uri", redirect_uri),
            ("state", "s1"),
        ])
        .send()
        .await
        .unwrap();
    assert!(consent.status().is_redirection(), "{}", consent.status());
    let location = consent.headers()["location"].to_str().unwrap().to_owned();
    let back = reqwest::Url::parse(&location).unwrap();
    let code = back
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .expect("a code in the redirect");

    let slack = client(&base, |_| {});
    let secret = SecretString::from("client-secret");
    let access = slack
        .oauth_v2_access(&code, redirect_uri, "pdm-test", &secret)
        .await
        .unwrap();
    assert!(access.access_token.expose_secret().starts_with("xoxb-"));
    assert!(!format!("{access:?}").contains(access.access_token.expose_secret()));
    let auth = slack.auth_test(&access.access_token).await.unwrap();
    assert_eq!(auth.team_id, access.team_id);

    // A code works once.
    let error = slack
        .oauth_v2_access(&code, redirect_uri, "pdm-test", &secret)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "invalid_code"),
        "{error:?}"
    );
}

/// Runs `test` with `faults` set on the Slack simulator, and always clears them afterwards.
async fn with_faults(faults: Value, test: impl Future<Output = ()>) {
    Faults::reset(SimKind::Slack).await;
    Faults::set(SimKind::Slack, faults).await;
    let outcome = AssertUnwindSafe(test).catch_unwind().await;
    Faults::reset(SimKind::Slack).await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_fail_next_is_retried_after_retry_after() {
    // Set up first, so only the call under test meets the fault.
    let (tenant, token) = farm().await;
    with_faults(json!({ "fail_next": 1, "retry_after_seconds": 1 }), async {
        let slack = client(tenant.base(), |_| {});
        let start = Instant::now();
        let auth = slack.auth_test(&token).await.unwrap();
        assert_eq!(auth.user_id, "user-bot");
        assert!(
            start.elapsed() >= Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        tenant.close().await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_throttle_is_honoured_until_it_lifts() {
    let (tenant, token) = farm().await;
    with_faults(
        json!({ "throttle": true, "retry_after_seconds": 1 }),
        async {
            let slack = client(tenant.base(), |c| c.retry.max_retries = 5);
            let start = Instant::now();
            let lift = async {
                tokio::time::sleep(Duration::from_millis(1_500)).await;
                Faults::reset(SimKind::Slack).await;
            };
            let (auth, ()) = tokio::join!(slack.auth_test(&token), lift);
            assert_eq!(auth.unwrap().user_id, "user-bot");
            // Attempts at 0, 1 and 2 s: the first two throttled, each wait the `Retry-After`.
            assert!(
                start.elapsed() >= Duration::from_secs(2),
                "{:?}",
                start.elapsed()
            );
            tenant.close().await;
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_error_rate_is_unavailable() {
    let (tenant, token) = farm().await;
    with_faults(json!({ "error_rate": 1 }), async {
        let slack = client(tenant.base(), |_| {});
        let error = slack.users_info(&token, "U_MARIA").await.unwrap_err();
        assert!(
            matches!(error, FarmClientError::Unavailable { .. }),
            "{error:?}"
        );
        assert_eq!(error.code(), FarmErrorCode::VendorUnavailable);
        tenant.close().await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_quota_limit_paces_to_a_minute() {
    // Set up first, so only the call under test meets the fault.
    let (tenant, token) = farm().await;
    with_faults(json!({ "quota_limit": 1 }), async {
        // Gives up on a `Retry-After` over 10 s instead of sleeping the minute.
        let slack = client(tenant.base(), |c| {
            c.retry = RetryPolicy {
                max_retry_after: Duration::from_secs(10),
                ..c.retry
            };
        });
        slack.auth_test(&token).await.unwrap();
        let start = Instant::now();
        let error = slack.auth_test(&token).await.unwrap_err();
        assert!(
            matches!(
                error,
                FarmClientError::RateLimited { retry_after: Some(d), .. } if d == Duration::from_secs(60)
            ),
            "{error:?}"
        );
        assert_eq!(error.code(), FarmErrorCode::RateLimited);
        assert!(start.elapsed() < Duration::from_secs(5), "it didn't wait");
        tenant.close().await;
    })
    .await;
}
