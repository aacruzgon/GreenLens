//! The client against `wiremock` (T-E2.6): every error shape, retries, paging, the code
//! exchange and the privacy of `users.info`. No Docker. Paused time only where nothing touches
//! the network: with a mock server in the loop, auto-advance would jump to the request's
//! deadline while the server answers (docs/backend integrations 17).

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{body_string_contains, header, method, path, query_param, query_param_is_missing},
};

use super::*;

const TOKEN: &str = "xoxb-test";

fn token() -> SecretString {
    SecretString::from(TOKEN)
}

/// Retries without back-off, so the tests run in real time without sleeping.
const FAST_RETRY: RetryPolicy = RetryPolicy {
    max_retries: 2,
    base: Duration::ZERO,
    cap: Duration::ZERO,
    max_retry_after: Duration::from_secs(5),
};

fn client_with(server: &MockServer, edit: impl FnOnce(&mut SlackConfig)) -> SlackClient {
    let mut config = SlackConfig::era(server.uri());
    config.timeout = Duration::from_secs(2);
    config.retry = FAST_RETRY;
    edit(&mut config);
    SlackClient::new(reqwest::Client::new(), config)
}

fn client(server: &MockServer) -> SlackClient {
    client_with(server, |_| {})
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn refused(code: &str) -> ResponseTemplate {
    ok(json!({ "ok": false, "error": code }))
}

async fn answering(route: &str, response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path(format!("/api/{route}")))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

async fn requests(server: &MockServer) -> usize {
    server.received_requests().await.map_or(0, |r| r.len())
}

fn auth_test_body() -> Value {
    json!({
        "ok": true, "url": "https://maplerow.slack.com/", "team": "Maple Row Farm",
        "user": "pdm", "team_id": "T_MAPLE", "user_id": "U_BOT", "bot_id": "B_BOT"
    })
}

#[tokio::test]
async fn auth_test_reads_the_workspace_with_the_bearer_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/auth.test"))
        .and(header("authorization", "Bearer xoxb-test"))
        .respond_with(ok(auth_test_body()))
        .expect(1)
        .mount(&server)
        .await;
    let auth = client(&server).auth_test(&token()).await.unwrap();
    assert_eq!(
        auth,
        AuthTest {
            team: "Maple Row Farm".into(),
            team_id: "T_MAPLE".into(),
            url: "https://maplerow.slack.com/".into(),
            user_id: "U_BOT".into(),
            bot_id: Some("B_BOT".into()),
        }
    );
}

#[tokio::test]
async fn an_empty_token_sends_no_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(path("/api/auth.test"))
        .and(|r: &Request| !r.headers.contains_key("authorization"))
        .respond_with(refused("not_authed"))
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .auth_test(&SecretString::from(""))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "not_authed"),
        "{error:?}"
    );
}

#[tokio::test]
async fn ok_false_auth_codes_are_auth_refused_and_never_retried() {
    for code in [
        "not_authed",
        "invalid_auth",
        "token_revoked",
        "token_expired",
        "account_inactive",
        "missing_scope",
        "no_permission",
        "team_access_not_granted",
        "ekm_access_denied",
        "not_allowed_token_type",
        "org_login_required",
    ] {
        let server = answering("auth.test", refused(code)).await;
        let error = client(&server).auth_test(&token()).await.unwrap_err();
        assert!(
            matches!(&error, FarmClientError::AuthRefused { vendor: "slack", code: c } if c == code),
            "{code}: {error:?}"
        );
        assert_eq!(error.code(), crate::farm::FarmErrorCode::AuthRefused);
        assert_eq!(requests(&server).await, 1, "{code}");
    }
}

#[tokio::test]
async fn missing_channels_are_not_found() {
    for code in ["channel_not_found", "not_in_channel"] {
        let server = answering("conversations.history", refused(code)).await;
        let error = client(&server)
            .conversations_history(&token(), "T_MAPLE", "C_GONE", None, None)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, FarmClientError::NotFound { code: c, .. } if c == code),
            "{code}: {error:?}"
        );
        assert_eq!(requests(&server).await, 1, "{code}");
    }
}

#[tokio::test]
async fn any_other_code_is_refused_with_its_status() {
    let server = answering("conversations.list", refused("invalid_cursor")).await;
    let error = client(&server)
        .conversations_list(&token())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::Refused { status: 200, code, .. } if code == "invalid_cursor"),
        "{error:?}"
    );
    assert_eq!(requests(&server).await, 1);

    // Era's read-only answer: a 403 with `error` and no `ok`.
    let server = answering(
        "auth.revoke",
        ResponseTemplate::new(403).set_body_json(json!({
            "error": "writes_disabled", "message": "this simulator is read-only"
        })),
    )
    .await;
    let error = client(&server).auth_revoke(&token()).await.unwrap_err();
    assert!(
        matches!(&error, FarmClientError::Refused { status: 403, code, .. } if code == "writes_disabled"),
        "{error:?}"
    );
}

#[tokio::test]
async fn ok_false_without_a_code_or_with_a_long_one_is_never_success() {
    let server = answering("auth.test", ok(json!({ "ok": false }))).await;
    let error = client(&server).auth_test(&token()).await.unwrap_err();
    assert!(
        matches!(&error, FarmClientError::Refused { code, .. } if code == "unrecognised"),
        "{error:?}"
    );

    // A code that isn't a short word could carry text from the body; it is replaced.
    let server = answering(
        "auth.test",
        refused("Bed 3: maria@maplerowfarm.example sprayed copper"),
    )
    .await;
    let error = client(&server).auth_test(&token()).await.unwrap_err();
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("maria"), "{shown}");
    assert!(
        matches!(&error, FarmClientError::Refused { code, .. } if code == "unrecognised"),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_429_waits_for_retry_after_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/api/auth.test"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .set_body_json(json!({ "ok": false, "error": "ratelimited" })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/auth.test"))
        .respond_with(ok(auth_test_body()))
        .mount(&server)
        .await;
    let start = std::time::Instant::now();
    let auth = client(&server).auth_test(&token()).await.unwrap();
    assert_eq!(auth.team_id, "T_MAPLE");
    assert!(
        start.elapsed() >= Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(requests(&server).await, 2);
}

#[tokio::test]
async fn a_429_that_persists_is_rate_limited_with_its_retry_after() {
    let server = answering(
        "conversations.history",
        ResponseTemplate::new(429).insert_header("retry-after", "0"),
    )
    .await;
    let error = client(&server)
        .conversations_history(&token(), "T_MAPLE", "C_FIELD", None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            FarmClientError::RateLimited { retry_after: Some(d), .. } if d.is_zero()
        ),
        "{error:?}"
    );
    assert_eq!(requests(&server).await, 3);
}

#[tokio::test]
async fn a_retry_after_over_the_policy_gives_up_at_once() {
    let server = answering(
        "auth.test",
        ResponseTemplate::new(429).insert_header("retry-after", "60"),
    )
    .await;
    let error = client(&server).auth_test(&token()).await.unwrap_err();
    assert!(
        matches!(
            error,
            FarmClientError::RateLimited { retry_after: Some(d), .. } if d == Duration::from_secs(60)
        ),
        "{error:?}"
    );
    assert_eq!(error.code(), crate::farm::FarmErrorCode::RateLimited);
    assert_eq!(requests(&server).await, 1);
}

#[tokio::test]
async fn ratelimited_at_http_200_is_a_rate_limit() {
    let server = answering("auth.test", refused("ratelimited")).await;
    let error = client_with(&server, |c| c.retry = RetryPolicy::NONE)
        .auth_test(&token())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            FarmClientError::RateLimited {
                retry_after: None,
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_500_is_retried_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/api/auth.test"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "ok": false, "error": "internal_error"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/auth.test"))
        .respond_with(ok(auth_test_body()))
        .mount(&server)
        .await;
    client(&server).auth_test(&token()).await.unwrap();
    assert_eq!(requests(&server).await, 2);
}

#[tokio::test]
async fn server_errors_that_persist_are_unavailable() {
    for response in [
        ResponseTemplate::new(503),
        refused("internal_error"),
        refused("fatal_error"),
    ] {
        let server = answering("users.info", response).await;
        let error = client(&server)
            .users_info(&token(), "U_MARIA")
            .await
            .unwrap_err();
        assert!(
            matches!(error, FarmClientError::Unavailable { source: None, .. }),
            "{error:?}"
        );
        assert_eq!(error.code(), crate::farm::FarmErrorCode::VendorUnavailable);
        assert_eq!(requests(&server).await, 3);
    }
}

#[tokio::test]
async fn a_malformed_body_is_malformed_and_not_retried() {
    for response in [
        ResponseTemplate::new(200).set_body_string("<html>gateway</html>"),
        ok(json!({ "ok": true })),
        ok(json!({ "messages": [] })),
    ] {
        let server = answering("auth.test", response).await;
        let error = client(&server).auth_test(&token()).await.unwrap_err();
        assert!(
            matches!(error, FarmClientError::Malformed { vendor: "slack" }),
            "{error:?}"
        );
        assert_eq!(requests(&server).await, 1);
    }
}

#[tokio::test]
async fn a_timeout_is_retried_then_unavailable() {
    let server = answering(
        "auth.test",
        ok(auth_test_body()).set_delay(Duration::from_secs(2)),
    )
    .await;
    let error = client_with(&server, |c| {
        c.timeout = Duration::from_millis(100);
        c.retry.max_retries = 1;
    })
    .auth_test(&token())
    .await
    .unwrap_err();
    let FarmClientError::Unavailable {
        source: Some(source),
        ..
    } = &error
    else {
        panic!("{error:?}");
    };
    assert!(source.timeout);
    // The transport error carries no URL anywhere in its chain.
    let mut chain = error.to_string();
    let mut next = std::error::Error::source(&error);
    while let Some(e) = next {
        chain = format!("{chain}: {e}");
        next = e.source();
    }
    assert!(!chain.contains("127.0.0.1"), "{chain}");
    assert!(!chain.contains("auth.test"), "{chain}");
    assert_eq!(requests(&server).await, 2);
}

#[tokio::test]
async fn conversations_list_reads_every_page_and_keeps_member_channels() {
    let server = MockServer::start().await;
    Mock::given(path("/api/conversations.list"))
        .and(query_param("types", "public_channel,private_channel"))
        .and(query_param("exclude_archived", "true"))
        .and(query_param("limit", "2"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ok(json!({
            "ok": true,
            "channels": [
                { "id": "C_GENERAL", "name": "general", "is_member": false },
                { "id": "C_FIELD", "name": "field-reports", "is_private": false, "is_member": true }
            ],
            "response_metadata": { "next_cursor": "page2" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/conversations.list"))
        .and(query_param("cursor", "page2"))
        .respond_with(ok(json!({
            "ok": true,
            "channels": [{ "id": "G_CREW", "name": "crew", "is_private": true, "is_member": true }],
            "response_metadata": { "next_cursor": "" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let channels = client_with(&server, |c| c.list_limit = 2)
        .conversations_list(&token())
        .await
        .unwrap();
    assert_eq!(
        channels,
        [
            SlackChannel {
                id: "C_FIELD".into(),
                name: "field-reports".into(),
                is_private: false
            },
            SlackChannel {
                id: "G_CREW".into(),
                name: "crew".into(),
                is_private: true
            },
        ]
    );
}

#[tokio::test]
async fn conversations_history_pages_by_cursor_from_oldest() {
    let server = MockServer::start().await;
    Mock::given(path("/api/conversations.history"))
        .and(query_param("channel", "C_FIELD"))
        .and(query_param("limit", "15"))
        .and(query_param("oldest", "1791792000.000100"))
        .and(query_param("cursor", "abc"))
        .respond_with(ok(json!({
            "ok": true,
            "messages": [
                { "type": "message", "user": "U_MARIA", "text": "Bed 3: rings on the lower leaves",
                  "ts": "1791792100.000200", "thread_ts": "1791792100.000200", "reply_count": 2,
                  "reply_users": ["U_DEV"], "latest_reply": "1791792300.000400" },
                { "type": "message", "subtype": "channel_join", "user": "U_SAM", "text": "joined",
                  "ts": "1791792050.000150" }
            ],
            "has_more": true,
            "response_metadata": { "next_cursor": "def" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let page = client(&server)
        .conversations_history(
            &token(),
            "T_MAPLE",
            "C_FIELD",
            Some("1791792000.000100"),
            Some("abc"),
        )
        .await
        .unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("def"));
    let parent = &page.messages[0];
    assert_eq!(parent.ts, "1791792100.000200");
    assert_eq!(parent.user.as_deref(), Some("U_MARIA"));
    assert_eq!(parent.reply_count, 2);
    assert_eq!(parent.latest_reply.as_deref(), Some("1791792300.000400"));
    assert!(!parent.is_reply());
    assert_eq!(page.messages[1].subtype.as_deref(), Some("channel_join"));
    assert_eq!(page.messages[1].reply_count, 0);
    // The text stays out of `Debug`.
    assert!(!format!("{parent:?}").contains("rings"));
}

#[tokio::test]
async fn conversations_replies_reads_every_page() {
    let server = MockServer::start().await;
    Mock::given(path("/api/conversations.replies"))
        .and(query_param("channel", "C_FIELD"))
        .and(query_param("ts", "100.000001"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ok(json!({
            "ok": true,
            "messages": [
                { "ts": "100.000001", "thread_ts": "100.000001", "reply_count": 2, "user": "U_A", "text": "parent" },
                { "ts": "101.000001", "thread_ts": "100.000001", "user": "U_B", "text": "one" }
            ],
            "response_metadata": { "next_cursor": "next" }
        })))
        .mount(&server)
        .await;
    Mock::given(path("/api/conversations.replies"))
        .and(query_param("cursor", "next"))
        .respond_with(ok(json!({
            "ok": true,
            "messages": [{ "ts": "102.000001", "thread_ts": "100.000001", "user": "U_C", "text": "two" }]
        })))
        .mount(&server)
        .await;
    let thread = client(&server)
        .conversations_replies(&token(), "T_MAPLE", "C_FIELD", "100.000001")
        .await
        .unwrap();
    let ts: Vec<_> = thread.iter().map(|m| m.ts.as_str()).collect();
    assert_eq!(ts, ["100.000001", "101.000001", "102.000001"]);
    assert!(!thread[0].is_reply());
    assert!(thread[1].is_reply() && thread[2].is_reply());
}

#[tokio::test]
async fn users_info_keeps_the_names_and_never_the_email() {
    let server = answering(
        "users.info",
        ok(json!({
            "ok": true,
            "user": {
                "id": "U_MARIA", "name": "maria", "real_name": "Maria Lopez",
                "profile": {
                    "real_name": "Maria Lopez", "display_name": "Maria",
                    "email": "maria@maplerowfarm.example", "phone": "+1 555 0100"
                }
            }
        })),
    )
    .await;
    let user = client(&server)
        .users_info(&token(), "U_MARIA")
        .await
        .unwrap();
    assert_eq!(
        user,
        SlackUser {
            id: "U_MARIA".into(),
            name: Some("maria".into()),
            real_name: Some("Maria Lopez".into()),
            display_name: Some("Maria".into()),
        }
    );
    assert_eq!(user.label(), "Maria");
    let debug = format!("{user:?}");
    assert!(
        !debug.contains('@') && !debug.contains("maplerowfarm"),
        "{debug}"
    );
    assert!(!debug.contains("Maria"), "names stay out of Debug: {debug}");
}

#[tokio::test]
async fn users_info_label_falls_back_past_an_empty_display_name() {
    let server = answering(
        "users.info",
        ok(json!({
            "ok": true,
            "user": { "id": "U_DEV", "name": "dev", "real_name": "Dev Patel",
                      "profile": { "display_name": "", "real_name": "" } }
        })),
    )
    .await;
    let user = client(&server).users_info(&token(), "U_DEV").await.unwrap();
    assert_eq!(user.display_name, None);
    assert_eq!(user.label(), "Dev Patel");
}

#[tokio::test]
async fn auth_revoke_posts_with_the_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth.revoke"))
        .and(header("authorization", "Bearer xoxb-test"))
        .respond_with(ok(json!({ "ok": true, "revoked": true })))
        .expect(1)
        .mount(&server)
        .await;
    client(&server).auth_revoke(&token()).await.unwrap();
}

#[tokio::test]
async fn oauth_v2_access_posts_a_form_with_the_client_secret() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/oauth.v2.access"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains("code=c0de"))
        .and(body_string_contains(
            "redirect_uri=https%3A%2F%2Fapi.example%2Fconnectors%2Foauth%2Fcallback",
        ))
        .and(body_string_contains("client_id=123.456"))
        .and(body_string_contains("client_secret=s%26cr%3Dt"))
        .and(|r: &Request| !r.headers.contains_key("authorization"))
        .respond_with(ok(json!({
            "ok": true, "access_token": "xoxb-granted", "token_type": "bot",
            "scope": "channels:read,channels:history", "bot_user_id": "U_BOT", "app_id": "A_PDM",
            "team": { "id": "T_MAPLE", "name": "Maple Row Farm" }, "enterprise": null,
            "authed_user": { "id": "U_MARIA" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let access = client(&server)
        .oauth_v2_access(
            "c0de",
            "https://api.example/connectors/oauth/callback",
            "123.456",
            &SecretString::from("s&cr=t"),
        )
        .await
        .unwrap();
    assert_eq!(access.access_token.expose_secret(), "xoxb-granted");
    assert_eq!(access.team_id, "T_MAPLE");
    assert_eq!(access.team_name, "Maple Row Farm");
    assert_eq!(access.bot_user_id.as_deref(), Some("U_BOT"));
    assert!(!format!("{access:?}").contains("xoxb-granted"));
}

#[tokio::test]
async fn oauth_v2_access_is_not_retried_after_a_5xx() {
    let server = answering("oauth.v2.access", ResponseTemplate::new(502)).await;
    let error = client(&server)
        .oauth_v2_access("c0de", "https://x", "1", &SecretString::from("s"))
        .await
        .unwrap_err();
    assert!(
        matches!(error, FarmClientError::Unavailable { .. }),
        "{error:?}"
    );
    assert_eq!(requests(&server).await, 1);

    let server = answering("oauth.v2.access", refused("invalid_code")).await;
    let error = client(&server)
        .oauth_v2_access("used", "https://x", "1", &SecretString::from("s"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, FarmClientError::Refused { code, .. } if code == "invalid_code"),
        "{error:?}"
    );
}

#[test]
fn permalinks_follow_slacks_format() {
    let url = "https://maplerow.slack.com/";
    assert_eq!(
        permalink(url, "C_FIELD", "1791792000.000100", None),
        "https://maplerow.slack.com/archives/C_FIELD/p1791792000000100"
    );
    assert_eq!(
        permalink(
            url,
            "C_FIELD",
            "1791792000.000100",
            Some("1791792000.000100")
        ),
        "https://maplerow.slack.com/archives/C_FIELD/p1791792000000100",
        "a parent links plainly"
    );
    assert_eq!(
        permalink(
            "https://maplerow.slack.com",
            "C_FIELD",
            "1791792100.000200",
            Some("1791792000.000100")
        ),
        "https://maplerow.slack.com/archives/C_FIELD/p1791792100000200\
         ?thread_ts=1791792000.000100&cid=C_FIELD"
    );
}

#[test]
fn form_encodes_reserved_characters() {
    assert_eq!(
        form(&[("a", "x y&z"), ("b", "é/=")]),
        "a=x+y%26z&b=%C3%A9%2F%3D"
    );
}

#[test]
fn defaults_are_the_vendors() {
    let real = SlackConfig::default();
    assert_eq!(real.base_url, "https://slack.com");
    assert_eq!(real.history_interval, Duration::from_secs(60));
    assert_eq!(real.history_limit, 15);
    let era = SlackConfig::era("http://127.0.0.1:8091/");
    assert_eq!(era.base_url, "http://127.0.0.1:8091");
    assert!(era.history_interval.is_zero());
    assert!(era.retry.base.is_zero());
    assert_eq!(era.retry.max_retry_after, Duration::from_secs(60));
}

#[tokio::test(start_paused = true)]
async fn pacers_are_shared_per_team_and_method() {
    let client = SlackClient::new(reqwest::Client::new(), SlackConfig::default());
    let a = client.pacer("T_A", "conversations.history").await.unwrap();
    let again = client.pacer("T_A", "conversations.history").await.unwrap();
    let replies = client.pacer("T_A", "conversations.replies").await.unwrap();
    let other = client.pacer("T_B", "conversations.history").await.unwrap();
    assert!(Arc::ptr_eq(&a, &again));
    assert!(!Arc::ptr_eq(&a, &replies));
    assert!(!Arc::ptr_eq(&a, &other));

    let start = tokio::time::Instant::now();
    a.wait().await;
    other.wait().await;
    replies.wait().await;
    assert_eq!(start.elapsed(), Duration::ZERO, "different keys don't wait");
    again.wait().await;
    assert_eq!(
        start.elapsed(),
        Duration::from_secs(60),
        "Tier 1: a minute apart"
    );

    let era = SlackClient::new(reqwest::Client::new(), SlackConfig::era("http://sim"));
    assert!(era.pacer("T_A", "conversations.history").await.is_none());
}

/// `judge` and `http::retry` together, without a network: a `429` waits exactly its
/// `Retry-After` before the next attempt.
#[tokio::test(start_paused = true)]
async fn a_429_is_honoured_for_exactly_its_retry_after() {
    let start = tokio::time::Instant::now();
    let policy = RetryPolicy {
        max_retry_after: Duration::from_secs(60),
        ..FAST_RETRY
    };
    let body = auth_test_body().to_string();
    let mut at = Vec::new();
    let result: Result<wire::AuthTest, FarmClientError> = http::retry(policy, |n| {
        at.push(start.elapsed());
        let body = body.clone();
        async move {
            if n == 0 {
                judge(
                    "auth.test",
                    true,
                    StatusCode::TOO_MANY_REQUESTS,
                    Some(Duration::from_secs(7)),
                    br#"{"ok":false,"error":"ratelimited"}"#,
                )
            } else {
                judge("auth.test", true, StatusCode::OK, None, body.as_bytes())
            }
        }
    })
    .await;
    assert_eq!(result.unwrap().team_id, "T_MAPLE");
    assert_eq!(at, [Duration::ZERO, Duration::from_secs(7)]);
}
