//! The Drive client against `wiremock` (T-E2.6): every error shape, the retries, pagination,
//! the caps and PKCE. Real time unless a test says otherwise: with paused time tokio would
//! auto-advance to the request's timeout while the mock server answers.

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{body_string_contains, header, method, path, query_param, query_param_is_missing},
};

use super::*;

/// Retries with no back-off, so a test only waits for what the server asks.
const FAST: RetryPolicy = RetryPolicy {
    max_retries: 2,
    base: Duration::ZERO,
    cap: Duration::ZERO,
    max_retry_after: Duration::from_secs(60),
};

fn client(server: &MockServer) -> DriveClient {
    DriveClient::new(
        reqwest::Client::new(),
        DriveConfig {
            api_base: server.uri(),
            token_url: format!("{}/token", server.uri()),
            revoke_url: format!("{}/revoke", server.uri()),
            retry: FAST,
            ..DriveConfig::default()
        },
    )
}

fn token() -> SecretString {
    SecretString::from("ya29.test-token")
}

/// Google's error body.
fn google_error(code: u16, status: &str, reason: &str) -> Value {
    json!({"error": {"code": code, "message": "echoed request text", "status": status,
        "errors": [{"domain": "global", "reason": reason, "message": "echoed request text"}]}})
}

fn refusal(code: u16, status: &str, reason: &str) -> ResponseTemplate {
    ResponseTemplate::new(code).set_body_json(google_error(code, status, reason))
}

async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

/// One `about.get` against a server that always answers `response`.
async fn about_with(response: ResponseTemplate) -> (Result<String, FarmClientError>, usize) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/about"))
        .respond_with(response)
        .mount(&server)
        .await;
    let result = client(&server).about_email(&token()).await;
    let count = requests(&server).await.len();
    (result, count)
}

fn file(id: &str, mime: &str, parent: &str) -> Value {
    json!({"id": id, "name": format!("name of {id}"), "mimeType": mime, "parents": [parent],
        "modifiedTime": "2026-10-15T12:00:00.000Z",
        "webViewLink": format!("https://docs.google.com/document/d/{id}/edit")})
}

// ---------------------------------------------------------------------------------------------
// PKCE and encoding
// ---------------------------------------------------------------------------------------------

#[test]
fn pkce_matches_rfc_7636_appendix_b() {
    let pair = Pkce::from_verifier(SecretString::from(
        "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
    ))
    .unwrap();
    assert_eq!(
        pair.challenge(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert_eq!(Pkce::METHOD, "S256");
}

#[test]
fn generated_pkce_verifiers_are_43_unreserved_characters_and_differ() {
    let a = Pkce::generate();
    let b = Pkce::generate();
    let v = a.verifier().expose_secret();
    assert_eq!(v.len(), 43);
    assert!(Pkce::from_verifier(SecretString::from(v.to_owned())).is_ok());
    assert_eq!(
        Pkce::from_verifier(SecretString::from(v.to_owned()))
            .unwrap()
            .challenge(),
        a.challenge()
    );
    assert_ne!(v, b.verifier().expose_secret());
    assert!(!format!("{a:?}").contains(v), "Debug hides the verifier");
}

#[test]
fn pkce_rejects_verifiers_rfc_7636_forbids() {
    for bad in [
        "a".repeat(42),
        "a".repeat(129),
        format!("{}+", "a".repeat(42)),
    ] {
        assert_eq!(
            Pkce::from_verifier(SecretString::from(bad)).unwrap_err(),
            InvalidVerifier
        );
    }
    assert!(Pkce::from_verifier(SecretString::from("a~.-_".repeat(25))).is_ok());
}

#[test]
fn form_bodies_and_queries_are_escaped() {
    assert_eq!(
        form(&[
            ("redirect_uri", "https://x.test/cb?a=1"),
            ("code", "4/0A b")
        ]),
        "redirect_uri=https%3A%2F%2Fx.test%2Fcb%3Fa%3D1&code=4%2F0A%20b"
    );
    assert_eq!(quote(r"it's\here"), r"it\'s\\here");
    assert_eq!(segment("a/b c"), "a%2Fb%20c");
}

#[test]
fn only_short_codes_are_read_from_error_bodies() {
    let body = |v: Value| serde_json::to_vec(&v).unwrap();
    assert_eq!(
        wire::error_codes(&body(google_error(
            403,
            "PERMISSION_DENIED",
            "fileNotDownloadable"
        ))),
        ["fileNotDownloadable"]
    );
    assert_eq!(
        wire::error_codes(&body(json!({"error": {"code": 500, "status": "INTERNAL"}}))),
        ["INTERNAL"]
    );
    assert_eq!(
        wire::error_codes(&body(
            json!({"error": "invalid_grant", "error_description": "x"})
        )),
        ["invalid_grant"]
    );
    let none: [String; 0] = [];
    assert_eq!(
        wire::error_codes(&body(google_error(403, "X", "Bed 3: rings on the leaves"))),
        none
    );
    assert_eq!(wire::error_codes(b"<html>busy</html>"), none);
}

#[test]
fn text_is_cut_on_a_character_boundary() {
    // "é" is two bytes; a cut after its first byte drops it.
    assert_eq!(into_text(vec![b'a', 0xC3]), "a");
    assert_eq!(into_text(vec![b'a', 0xFF, b'b']), "a\u{FFFD}b");
}

#[test]
fn a_character_straddling_the_limit_is_dropped_not_replaced() {
    // An invalid byte early on, then "é" cut after its first byte at exactly 1 MB.
    let mut bytes = vec![b'a', 0xFF];
    bytes.resize(MAX_TEXT_BYTES - 1, b'b');
    bytes.push(0xC3);
    assert_eq!(bytes.len(), MAX_TEXT_BYTES);
    let text = into_text(bytes);
    assert!(text.len() <= MAX_TEXT_BYTES, "{}", text.len());
    assert!(text.starts_with("a\u{FFFD}b"));
    assert!(text.ends_with('b'), "the cut character became U+FFFD");
    assert_eq!(text.matches('\u{FFFD}').count(), 1);
}

// ---------------------------------------------------------------------------------------------
// Error shapes and retries
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn about_sends_the_token_and_reads_the_email() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/about"))
        .and(query_param("fields", "user(emailAddress)"))
        .and(header("authorization", "Bearer ya29.test-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"user": {"emailAddress": "maria@example.test"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).about_email(&token()).await.unwrap(),
        "maria@example.test"
    );
}

#[tokio::test]
async fn unauthorized_is_auth_refused_and_not_retried() {
    let (result, count) = about_with(refusal(401, "UNAUTHENTICATED", "authError")).await;
    let error = result.unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { vendor: "google_drive", code } if code == "authError"),
        "{error:?}"
    );
    assert_eq!(error.code(), crate::farm::FarmErrorCode::AuthRefused);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn not_found_is_not_found_and_not_retried() {
    let (result, count) = about_with(refusal(404, "NOT_FOUND", "notFound")).await;
    assert!(
        matches!(result, Err(FarmClientError::NotFound { ref code, .. }) if code == "notFound")
    );
    assert_eq!(count, 1);
}

#[tokio::test]
async fn other_forbidden_reasons_are_refused_and_never_retried() {
    for reason in [
        "fileNotDownloadable",
        "domainPolicy",
        // On one file, not a scope call: the file's own sharing.
        "insufficientPermissions",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(refusal(403, "PERMISSION_DENIED", reason))
            .mount(&server)
            .await;
        let result = client(&server).download_text(&token(), "F_1").await;
        assert!(
            matches!(result, Err(FarmClientError::Refused { status: 403, ref code, .. }) if code == reason),
            "{reason}"
        );
        assert_eq!(requests(&server).await.len(), 1, "{reason} was retried");
    }
    let (result, _) = about_with(refusal(403, "PERMISSION_DENIED", "domainPolicy")).await;
    assert!(matches!(
        result,
        Err(FarmClientError::Refused { status: 403, .. })
    ));
}

/// Google's answer to a token without the scope: `insufficientPermissions` with an
/// `ErrorInfo` detail.
fn scope_refusal() -> ResponseTemplate {
    ResponseTemplate::new(403).set_body_json(json!({"error": {"code": 403,
        "message": "Request had insufficient authentication scopes.",
        "status": "PERMISSION_DENIED",
        "errors": [{"message": "Insufficient Permission", "domain": "global",
                    "reason": "insufficientPermissions"}],
        "details": [{"@type": "type.googleapis.com/google.rpc.ErrorInfo",
                     "reason": "ACCESS_TOKEN_SCOPE_INSUFFICIENT", "domain": "googleapis.com"}]}}))
}

#[tokio::test]
async fn a_missing_scope_is_auth_refused_and_not_retried() {
    let (result, count) = about_with(scope_refusal()).await;
    let error = result.unwrap_err();
    assert!(
        matches!(&error, FarmClientError::AuthRefused { code, .. } if code == "insufficientPermissions"),
        "{error:?}"
    );
    assert_eq!(error.code(), crate::farm::FarmErrorCode::AuthRefused);
    assert_eq!(count, 1);
    // A scope call with only the legacy reason.
    let (result, _) =
        about_with(refusal(403, "PERMISSION_DENIED", "insufficientPermissions")).await;
    assert!(matches!(result, Err(FarmClientError::AuthRefused { .. })));
    // The `ErrorInfo` reason is unambiguous on any call.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(scope_refusal())
        .mount(&server)
        .await;
    let result = client(&server)
        .export(&token(), "D_1", ExportFormat::PlainText)
        .await;
    assert!(matches!(result, Err(FarmClientError::AuthRefused { .. })));
}

#[tokio::test]
async fn rate_limit_forbidden_reasons_are_retried_then_rate_limited() {
    for reason in ["userRateLimitExceeded", "rateLimitExceeded"] {
        let (result, count) = about_with(refusal(403, "PERMISSION_DENIED", reason)).await;
        let error = result.unwrap_err();
        assert!(
            matches!(
                error,
                FarmClientError::RateLimited {
                    retry_after: None,
                    ..
                }
            ),
            "{reason}: {error:?}"
        );
        assert_eq!(error.code(), crate::farm::FarmErrorCode::RateLimited);
        assert_eq!(count, 3, "{reason}: the first attempt and two retries");
    }
}

#[tokio::test]
async fn a_rate_limit_forbidden_then_success_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(refusal(403, "PERMISSION_DENIED", "userRateLimitExceeded"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"startPageToken": "3919"})))
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).start_page_token(&token()).await.unwrap(),
        "3919"
    );
    assert_eq!(requests(&server).await.len(), 2);
}

/// Paused time: the request has no deadline timer and the client keeps no idle connection,
/// so the only timer is the retry's wait, which auto-advance runs at once.
#[tokio::test(start_paused = true)]
async fn too_many_requests_honours_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            refusal(429, "RESOURCE_EXHAUSTED", "userRateLimitExceeded")
                .insert_header("retry-after", "30"),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"startPageToken": "3919"})))
        .mount(&server)
        .await;
    let mut drive = DriveClient::new(
        reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap(),
        DriveConfig {
            api_base: server.uri(),
            retry: FAST,
            ..DriveConfig::default()
        },
    );
    drive.request_timeout = None;
    let start = tokio::time::Instant::now();
    assert_eq!(drive.start_page_token(&token()).await.unwrap(), "3919");
    assert_eq!(start.elapsed(), Duration::from_secs(30));
    assert_eq!(requests(&server).await.len(), 2);
}

#[tokio::test]
async fn too_many_requests_past_the_retries_is_rate_limited_with_its_wait() {
    // A wait over `max_retry_after` gives up at once.
    let (result, count) = about_with(
        refusal(429, "RESOURCE_EXHAUSTED", "userRateLimitExceeded")
            .insert_header("retry-after", "3600"),
    )
    .await;
    assert!(matches!(
        result,
        Err(FarmClientError::RateLimited {
            retry_after: Some(d),
            ..
        }) if d == Duration::from_secs(3600)
    ));
    assert_eq!(count, 1);
}

#[tokio::test]
async fn a_server_error_then_success_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(refusal(500, "INTERNAL", "backendError"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"user": {"emailAddress": "a@b.test"}})),
        )
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).about_email(&token()).await.unwrap(),
        "a@b.test"
    );
    assert_eq!(requests(&server).await.len(), 2);
}

#[tokio::test]
async fn server_errors_past_the_retries_are_unavailable() {
    let (result, count) = about_with(refusal(503, "UNAVAILABLE", "backendError")).await;
    let error = result.unwrap_err();
    assert!(matches!(
        error,
        FarmClientError::Unavailable { source: None, .. }
    ));
    assert_eq!(error.code(), crate::farm::FarmErrorCode::VendorUnavailable);
    assert_eq!(count, 3);
}

#[tokio::test]
async fn a_malformed_body_is_malformed_and_not_retried() {
    let (result, count) =
        about_with(ResponseTemplate::new(200).set_body_json(json!({"user": {"name": "no email"}})))
            .await;
    assert!(matches!(result, Err(FarmClientError::Malformed { .. })));
    assert_eq!(count, 1);
    let (result, _) = about_with(ResponseTemplate::new(200).set_body_string("<html>")).await;
    assert!(matches!(result, Err(FarmClientError::Malformed { .. })));
}

#[tokio::test]
async fn an_unparseable_error_body_maps_by_status() {
    let (result, _) =
        about_with(ResponseTemplate::new(403).set_body_string("<html>no</html>")).await;
    assert!(
        matches!(result, Err(FarmClientError::Refused { status: 403, ref code, .. }) if code == "http_403")
    );
}

#[tokio::test]
async fn a_timeout_is_unavailable_with_its_cause() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let drive = DriveClient::new(
        reqwest::Client::new(),
        DriveConfig {
            api_base: server.uri(),
            timeout: Duration::from_millis(100),
            retry: RetryPolicy::NONE,
            ..DriveConfig::default()
        },
    );
    let error = drive.about_email(&token()).await.unwrap_err();
    assert!(
        matches!(
            &error,
            FarmClientError::Unavailable {
                source: Some(TransportError { timeout: true, .. }),
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn errors_and_their_debug_never_carry_the_body() {
    let (result, _) =
        about_with(refusal(403, "PERMISSION_DENIED", "insufficientPermissions")).await;
    let error = result.unwrap_err();
    assert!(!format!("{error:?}").contains("echoed"));
    assert!(!error.to_string().contains("echoed"));
}

// ---------------------------------------------------------------------------------------------
// Listing, text and changes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_folder_lists_recursively_across_pages() {
    let server = MockServer::start().await;
    let q = |id: &str| format!("'{id}' in parents and trashed=false");
    // The root: two pages; the second holds a subfolder.
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .and(query_param("q", q("F_ROOT")))
        .and(query_param("pageSize", "2"))
        .and(query_param("supportsAllDrives", "true"))
        .and(query_param("includeItemsFromAllDrives", "true"))
        .and(query_param(
            "fields",
            "nextPageToken,files(id,name,mimeType,parents,modifiedTime,webViewLink,size,trashed)",
        ))
        .and(query_param_is_missing("pageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "nextPageToken": "p2",
            "files": [file("D_1", DOCUMENT_MIME, "F_ROOT"), file("S_1", SPREADSHEET_MIME, "F_ROOT")]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .and(query_param("q", q("F_ROOT")))
        .and(query_param("pageToken", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "files": [file("F_SUB", FOLDER_MIME, "F_ROOT"),
                      {"id": "T_1", "name": "notes.txt", "mimeType": "text/plain",
                       "parents": ["F_ROOT"], "size": "12"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .and(query_param("q", q("F_SUB")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "files": [file("P_1", "application/pdf", "F_SUB"),
                      // A folder seen twice is listed once.
                      file("F_ROOT", FOLDER_MIME, "F_SUB")]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let drive = DriveClient::new(
        reqwest::Client::new(),
        DriveConfig {
            api_base: server.uri(),
            page_size: 2,
            retry: FAST,
            ..DriveConfig::default()
        },
    );
    let listing = drive.list_folder(&token(), "F_ROOT").await.unwrap();
    let ids: Vec<_> = listing.files.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["D_1", "S_1", "T_1", "P_1"]);
    assert_eq!(listing.folders, ["F_ROOT", "F_SUB"]);
    assert!(!listing.truncated);
    let kinds: Vec<_> = listing.files.iter().map(DriveFile::kind).collect();
    assert_eq!(
        kinds,
        [
            FileKind::Document,
            FileKind::Spreadsheet,
            FileKind::PlainText,
            FileKind::Other
        ]
    );
    assert_eq!(listing.files[2].size, Some(12));
    assert_eq!(
        listing.files[0].modified_time.unwrap().to_rfc3339(),
        "2026-10-15T12:00:00+00:00"
    );
}

#[tokio::test]
async fn the_source_picker_lists_every_folder_by_name_across_pages() {
    let server = MockServer::start().await;
    let q = format!("mimeType='{FOLDER_MIME}' and trashed=false");
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .and(header("authorization", "Bearer ya29.test-token"))
        .and(query_param("q", q.as_str()))
        .and(query_param("supportsAllDrives", "true"))
        .and(query_param("includeItemsFromAllDrives", "true"))
        .and(query_param_is_missing("pageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "nextPageToken": "p2",
            "files": [{"id": "F_2", "name": "Spray logs", "mimeType": FOLDER_MIME},
                      // Drive never sends a file for this query; the client keeps folders only.
                      file("D_1", DOCUMENT_MIME, "F_2")]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .and(query_param("q", q.as_str()))
        .and(query_param("pageToken", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "files": [{"id": "F_1", "name": "Farm records", "mimeType": FOLDER_MIME}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let folders = client(&server).folders(&token()).await.unwrap();
    let names: Vec<_> = folders
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_str()))
        .collect();
    assert_eq!(names, [("F_1", "Farm records"), ("F_2", "Spray logs")]);
}

#[tokio::test]
async fn the_source_picker_refuses_a_page_cycle() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "nextPageToken": "again", "files": []
        })))
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server).folders(&token()).await,
        Err(FarmClientError::Malformed { .. })
    ));
}

#[tokio::test]
async fn a_listing_stops_at_500_files() {
    let server = MockServer::start().await;
    let many: Vec<_> = (0..600)
        .map(|i| file(&format!("D_{i}"), DOCUMENT_MIME, "F_ROOT"))
        .collect();
    Mock::given(method("GET"))
        .and(query_param_is_missing("pageToken"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"nextPageToken": "more", "files": many})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let listing = client(&server)
        .list_folder(&token(), "F_ROOT")
        .await
        .unwrap();
    assert_eq!(listing.files.len(), MAX_FILES);
    assert!(listing.truncated);
}

#[tokio::test]
async fn exactly_500_files_are_not_truncated() {
    let server = MockServer::start().await;
    let five_hundred: Vec<_> = (0..500)
        .map(|i| file(&format!("D_{i}"), DOCUMENT_MIME, "F_ROOT"))
        .collect();
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"files": five_hundred})))
        .mount(&server)
        .await;
    let listing = client(&server)
        .list_folder(&token(), "F_ROOT")
        .await
        .unwrap();
    assert_eq!(listing.files.len(), 500);
    assert!(!listing.truncated);
}

#[tokio::test]
async fn exports_and_downloads_are_cut_at_1_mb() {
    let server = MockServer::start().await;
    // 999,999 ASCII bytes, then "é" across the 1,000,000-byte line, then more.
    let mut big = "a".repeat(MAX_TEXT_BYTES - 1);
    big.push('é');
    big.push_str(&"b".repeat(5000));
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_BIG/export"))
        .and(query_param("mimeType", "text/plain"))
        .respond_with(ResponseTemplate::new(200).set_body_string(big))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/S_PLAN/export"))
        .and(query_param("mimeType", "text/csv"))
        .respond_with(ResponseTemplate::new(200).set_body_string("bed,crop\n3,tomato\n"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/T_1"))
        .and(query_param("alt", "media"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(MAX_TEXT_BYTES)))
        .mount(&server)
        .await;
    let drive = client(&server);
    let cut = drive
        .export(&token(), "D_BIG", ExportFormat::PlainText)
        .await
        .unwrap();
    assert!(cut.truncated);
    assert_eq!(
        cut.text.len(),
        MAX_TEXT_BYTES - 1,
        "the split character is dropped"
    );
    assert!(!format!("{cut:?}").contains("aaa"), "Debug hides the text");
    let csv = drive
        .export(&token(), "S_PLAN", ExportFormat::Csv)
        .await
        .unwrap();
    assert_eq!(csv.text, "bed,crop\n3,tomato\n");
    assert!(!csv.truncated);
    let exact = drive.download_text(&token(), "T_1").await.unwrap();
    assert_eq!(exact.text.len(), MAX_TEXT_BYTES);
    assert!(!exact.truncated, "exactly 1 MB isn't cut");
}

#[tokio::test]
async fn changes_are_read_to_the_new_start_page_token() {
    let server = MockServer::start().await;
    let change = |id: &str| {
        json!({"fileId": id, "removed": false, "time": "2026-10-15T12:00:00.000Z",
        "file": file(id, DOCUMENT_MIME, "F_FARM")})
    };
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .and(query_param("pageToken", "100"))
        .and(query_param("includeRemoved", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "nextPageToken": "101", "changes": [change("D_1"), change("D_2")]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .and(query_param("pageToken", "101"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "newStartPageToken": "102",
            "changes": [{"fileId": "D_3", "removed": true}]})))
        .expect(1)
        .mount(&server)
        .await;
    let set = client(&server).changes(&token(), "100").await.unwrap();
    assert_eq!(set.pages, 2);
    assert_eq!(set.new_start_page_token, "102");
    let ids: Vec<_> = set.changes.iter().map(|c| c.file_id.as_str()).collect();
    assert_eq!(ids, ["D_1", "D_2", "D_3"]);
    assert!(set.changes[2].removed && set.changes[2].file.is_none());
}

#[tokio::test]
async fn a_change_page_without_a_way_on_is_malformed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"nextPageToken": "100", "changes": []})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server).changes(&token(), "100").await,
        Err(FarmClientError::Malformed { .. })
    ));
}

#[tokio::test]
async fn a_change_page_cycle_is_malformed() {
    let server = MockServer::start().await;
    for (from, to) in [("100", "200"), ("200", "100")] {
        Mock::given(method("GET"))
            .and(path("/drive/v3/changes"))
            .and(query_param("pageToken", from))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"nextPageToken": to, "changes": []})),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    assert!(matches!(
        client(&server).changes(&token(), "100").await,
        Err(FarmClientError::Malformed { .. })
    ));
}

#[tokio::test]
async fn a_folder_page_cycle_is_malformed() {
    let server = MockServer::start().await;
    // The first page (no token) points to p1, p1 to p2, and p2 back to p1.
    for (from, to) in [(None, "p1"), (Some("p1"), "p2"), (Some("p2"), "p1")] {
        let mock = Mock::given(method("GET")).and(path("/drive/v3/files"));
        let mock = match from {
            Some(t) => mock.and(query_param("pageToken", t)),
            None => mock.and(query_param_is_missing("pageToken")),
        };
        mock.respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"nextPageToken": to, "files": []})),
        )
        .expect(1)
        .mount(&server)
        .await;
    }
    assert!(matches!(
        client(&server).list_folder(&token(), "F_ROOT").await,
        Err(FarmClientError::Malformed { .. })
    ));
}

// ---------------------------------------------------------------------------------------------
// OAuth
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_code_exchange_sends_the_verifier_and_reads_the_tokens() {
    let server = MockServer::start().await;
    let pkce = Pkce::generate();
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=4%2F0Acode"))
        .and(body_string_contains(format!(
            "code_verifier={}",
            pkce.verifier().expose_secret()
        )))
        .and(body_string_contains(
            "redirect_uri=https%3A%2F%2Fapi.example.test%2Fconnectors%2Foauth%2Fcallback",
        ))
        .and(body_string_contains("client_id=client-1"))
        .and(body_string_contains("client_secret=shh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "ya29.new", "expires_in": 3599, "refresh_token": "1//refresh",
            "scope": SCOPE, "token_type": "Bearer"})))
        .expect(1)
        .mount(&server)
        .await;
    let grant = client(&server)
        .exchange_code(
            "4/0Acode",
            pkce.verifier(),
            "https://api.example.test/connectors/oauth/callback",
            "client-1",
            &SecretString::from("shh"),
        )
        .await
        .unwrap();
    assert_eq!(grant.access_token.expose_secret(), "ya29.new");
    assert_eq!(
        grant
            .refresh_token
            .as_ref()
            .map(ExposeSecret::expose_secret),
        Some("1//refresh")
    );
    assert_eq!(grant.expires_in, Duration::from_secs(3599));
    assert_eq!(grant.scope.as_deref(), Some(SCOPE));
    assert!(
        !format!("{grant:?}").contains("ya29"),
        "Debug hides the tokens"
    );
}

#[tokio::test]
async fn a_reused_code_is_auth_refused_and_the_exchange_is_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("code=used"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(
                json!({"error": "invalid_grant", "error_description": "Bad Request"}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("code=busy"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let drive = client(&server);
    let secret = SecretString::from("shh");
    let verifier = Pkce::generate();
    let used = drive
        .exchange_code(
            "used",
            verifier.verifier(),
            "https://x.test/cb",
            "c",
            &secret,
        )
        .await;
    assert!(
        matches!(used, Err(FarmClientError::AuthRefused { ref code, .. }) if code == "invalid_grant")
    );
    let busy = drive
        .exchange_code(
            "busy",
            verifier.verifier(),
            "https://x.test/cb",
            "c",
            &secret,
        )
        .await;
    assert!(matches!(busy, Err(FarmClientError::Unavailable { .. })));
    assert_eq!(requests(&server).await.len(), 2, "one request per exchange");
}

#[tokio::test]
async fn a_refused_refresh_is_auth_refused() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=1%2F%2Fold"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .refresh(&SecretString::from("1//old"), "c", &SecretString::from("s"))
        .await
        .unwrap_err();
    assert_eq!(error.code(), crate::farm::FarmErrorCode::AuthRefused);
}

#[tokio::test]
async fn a_refresh_gives_a_new_access_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "ya29.fresh", "expires_in": 3599, "token_type": "Bearer"}),
        ))
        .mount(&server)
        .await;
    let grant = client(&server)
        .refresh(&SecretString::from("1//r"), "c", &SecretString::from("s"))
        .await
        .unwrap();
    assert_eq!(grant.access_token.expose_secret(), "ya29.fresh");
    assert!(grant.refresh_token.is_none());
}

#[tokio::test]
async fn revoking_a_token_google_no_longer_knows_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/revoke"))
        .and(body_string_contains("token=1%2F%2Fgood"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/revoke"))
        .and(body_string_contains("token=gone"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_token"})))
        .expect(1)
        .mount(&server)
        .await;
    let drive = client(&server);
    drive.revoke(&SecretString::from("1//good")).await.unwrap();
    drive.revoke(&SecretString::from("gone")).await.unwrap();
}
