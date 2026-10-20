//! [`DriveSource`] against `wiremock` (docs/EON T-E4.3, T-E4.5): the first sync's start token
//! and recursive listing, the pages of the queue, the changes kept under the folder (moved out,
//! trashed, deleted, nested folders moved in and out), the text of each kind and its 1 MB cut,
//! and a `429` that fails the page so the sync keeps its cursor.

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use domain::connectors::ConnectorAccount;

use super::*;
use crate::{
    google_drive::{DriveConfig, FOLDER_MIME, SPREADSHEET_MIME},
    http::RetryPolicy,
};

const ROOT: &str = "F_ROOT";
const PDF: &str = "application/pdf";
const DOC: &str = "application/vnd.google-apps.document";

fn source(server: &MockServer) -> DriveSource {
    DriveSource::new(DriveClient::new(
        reqwest::Client::new(),
        DriveConfig {
            api_base: server.uri(),
            token_url: format!("{}/token", server.uri()),
            revoke_url: format!("{}/revoke", server.uri()),
            retry: RetryPolicy {
                max_retries: 1,
                base: Duration::ZERO,
                cap: Duration::ZERO,
                max_retry_after: Duration::from_secs(60),
            },
            ..DriveConfig::default()
        },
    ))
}

fn now() -> Timestamp {
    "2026-10-15T12:00:00Z".parse().expect("a timestamp")
}

fn request(cursor: Option<SyncCursor>) -> FarmPageRequest {
    FarmPageRequest {
        account: ConnectorAccount::GoogleDrive {
            email: "farm@example.org".into(),
        },
        source: ConnectorSource {
            id: ROOT.into(),
            name: "Farm records".into(),
        },
        cursor,
        oldest: now().minus(FARM_RECORD_KEEP),
        reread_from: None,
        probe_limit: 2,
    }
}

fn token() -> VendorToken {
    VendorToken::new("ya29.test")
}

fn file(id: &str, mime: &str, parent: &str) -> Value {
    json!({"id": id, "name": format!("name of {id}"), "mimeType": mime, "parents": [parent],
        "modifiedTime": "2026-10-14T08:00:00.000Z",
        "webViewLink": format!("https://docs.google.com/document/d/{id}/edit")})
}

fn change(id: &str, file: Option<Value>) -> Value {
    match file {
        Some(f) => {
            json!({"fileId": id, "removed": false, "time": "2026-10-15T09:00:00.000Z", "file": f})
        }
        None => json!({"fileId": id, "removed": true, "time": "2026-10-15T09:00:00.000Z"}),
    }
}

/// `files.list` of `folder` answers `files`.
async fn listing(server: &MockServer, folder: &str, files: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path("/drive/v3/files"))
        .and(query_param(
            "q",
            format!("'{folder}' in parents and trashed=false"),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "files": files })))
        .mount(server)
        .await;
}

async fn start_token(server: &MockServer, token: &str) {
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes/startPageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "startPageToken": token })))
        .mount(server)
        .await;
}

/// `changes.list` from `from` answers `changes` in two pages, then `next`.
async fn changes(server: &MockServer, from: &str, changes: Vec<Value>, next: &str) {
    let half = changes.len() / 2;
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .and(query_param("pageToken", from))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "changes": changes[..half], "nextPageToken": format!("{from}-2") }),
        ))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .and(query_param("pageToken", format!("{from}-2")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "changes": changes[half..], "newStartPageToken": next })),
        )
        .mount(server)
        .await;
}

/// Every Doc and Sheet exports as `<id> as <format>`, every plain-text file downloads as
/// `<id> downloaded`.
async fn contents(server: &MockServer) {
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            r"^/drive/v3/files/[^/]+/export$",
        ))
        .respond_with(|r: &wiremock::Request| {
            let id = r
                .url
                .path()
                .split('/')
                .nth(4)
                .unwrap_or_default()
                .to_owned();
            let format = r
                .url
                .query_pairs()
                .find(|(k, _)| k == "mimeType")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            ResponseTemplate::new(200).set_body_string(format!("{id} as {format}"))
        })
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(r"^/drive/v3/files/[^/]+$"))
        .and(query_param("alt", "media"))
        .respond_with(|r: &wiremock::Request| {
            let id = r
                .url
                .path()
                .split('/')
                .nth(4)
                .unwrap_or_default()
                .to_owned();
            ResponseTemplate::new(200).set_body_string(format!("{id} downloaded"))
        })
        .mount(server)
        .await;
}

fn cursor_of(page: &FarmPage) -> Cursor {
    Cursor::parse(Some(&page.cursor)).expect("the adapter's cursor")
}

fn texts(page: &FarmPage) -> BTreeMap<&str, &str> {
    page.items
        .iter()
        .map(|i| (i.vendor_id.as_str(), i.text.as_str()))
        .collect()
}

/// A cursor holding `files` in the root, `D_SUB` in `F_SUB`, with no pass in progress.
fn held(files: &[&str]) -> SyncCursor {
    let mut held: BTreeMap<String, String> = files
        .iter()
        .map(|f| ((*f).to_owned(), ROOT.to_owned()))
        .collect();
    held.insert("D_SUB".into(), "F_SUB".into());
    Cursor {
        token: "100".into(),
        folders: BTreeMap::from([("F_SUB".to_owned(), ROOT.to_owned())]),
        files: held,
        queue: VecDeque::new(),
    }
    .to_sync_cursor()
}

#[tokio::test]
async fn t_e4_3_a_first_sync_takes_the_start_token_then_reads_the_folder_recursively() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    listing(
        &server,
        ROOT,
        vec![
            file("D1", DOC, ROOT),
            file("S1", SPREADSHEET_MIME, ROOT),
            file("T1", "text/plain", ROOT),
            file("P1", PDF, ROOT),
            file("F_SUB", FOLDER_MIME, ROOT),
        ],
    )
    .await;
    listing(&server, "F_SUB", vec![file("D_SUB", DOC, "F_SUB")]).await;
    contents(&server).await;

    let page = source(&server)
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    assert_eq!(
        texts(&page),
        BTreeMap::from([
            ("D1", "D1 as text/plain"),
            ("S1", "S1 as text/csv"),
            ("T1", "T1 downloaded"),
            ("P1", ""),
            ("D_SUB", "D_SUB as text/plain"),
        ])
    );
    assert!(!page.more);
    assert_eq!(page.removed, Vec::<String>::new());
    let d1 = page.items.iter().find(|i| i.vendor_id == "D1").expect("D1");
    assert_eq!(d1.kind, FarmRecordKind::Document);
    assert_eq!(d1.title, "name of D1");
    assert_eq!(d1.link, "https://docs.google.com/document/d/D1/edit");
    assert_eq!(d1.at.to_string(), "2026-10-14T08:00:00Z");
    let cursor = cursor_of(&page);
    assert_eq!(cursor.token, "100");
    assert_eq!(cursor.folders["F_SUB"], ROOT);
    assert_eq!(cursor.files["D_SUB"], "F_SUB");
    assert_eq!(cursor.files.len(), 5);

    // The start token comes before the listing, so nothing changed during it is missed.
    let paths: Vec<String> = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert_eq!(paths[0], "/drive/v3/changes/startPageToken");
    assert_eq!(paths[1], "/drive/v3/files");
}

#[tokio::test]
async fn a_long_folder_is_read_25_files_a_page_from_the_cursor() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    let files: Vec<Value> = (0..30)
        .map(|n| file(&format!("P{n:02}"), PDF, ROOT))
        .collect();
    listing(&server, ROOT, files).await;
    let drive = source(&server);

    let first = drive
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    assert_eq!(first.items.len(), FILES_PER_PAGE);
    assert!(first.more);
    let second = drive
        .read_page(&token(), request(Some(first.cursor.clone())))
        .await
        .expect("a page");
    assert_eq!(second.items.len(), 5);
    assert!(!second.more);
    assert_eq!(second.items[0].vendor_id, "P25");
    let calls = server.received_requests().await.expect("recorded");
    assert_eq!(calls.len(), 2, "listed once, no changes read mid-pass");
}

#[tokio::test]
async fn t_e4_5_later_syncs_keep_only_the_changes_under_the_folder() {
    let server = MockServer::start().await;
    changes(
        &server,
        "100",
        vec![
            // Edited in place: read again.
            change("D1", Some(file("D1", DOC, ROOT))),
            // Elsewhere in the account, never held: ignored.
            change("D_OUT", Some(file("D_OUT", DOC, "F_ELSE"))),
            // Moved out of the folder: removed.
            change("D2", Some(file("D2", DOC, "F_ELSE"))),
            // Trashed: removed.
            change(
                "D3",
                Some(
                    json!({"id": "D3", "name": "D3", "mimeType": DOC, "parents": [ROOT],
                    "trashed": true}),
                ),
            ),
            // Deleted: removed.
            change("D4", None),
            // Created in the nested folder: added.
            change("N1", Some(file("N1", "text/plain", "F_SUB"))),
            // Deleted elsewhere, never held: nothing.
            change("X9", None),
        ],
        "200",
    )
    .await;
    contents(&server).await;

    let page = source(&server)
        .read_page(&token(), request(Some(held(&["D1", "D2", "D3", "D4"]))))
        .await
        .expect("a page");
    assert_eq!(
        texts(&page),
        BTreeMap::from([("D1", "D1 as text/plain"), ("N1", "N1 downloaded")])
    );
    let mut removed = page.removed.clone();
    removed.sort();
    assert_eq!(removed, ["D2", "D3", "D4"]);
    let cursor = cursor_of(&page);
    assert_eq!(cursor.token, "200", "the last page's newStartPageToken");
    assert_eq!(
        cursor.files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["D1", "D_SUB", "N1"]
    );
    assert!(!page.more);
}

#[tokio::test]
async fn a_folder_moved_out_takes_its_files_and_one_moved_in_is_listed() {
    let server = MockServer::start().await;
    changes(
        &server,
        "100",
        vec![
            // The nested folder leaves; its file brings no change of its own.
            change("F_SUB", Some(file("F_SUB", FOLDER_MIME, "F_ELSE"))),
            // A folder with a folder inside moves in.
            change("F_NEW", Some(file("F_NEW", FOLDER_MIME, ROOT))),
        ],
        "200",
    )
    .await;
    listing(
        &server,
        "F_NEW",
        vec![
            file("D_NEW", DOC, "F_NEW"),
            file("F_DEEP", FOLDER_MIME, "F_NEW"),
        ],
    )
    .await;
    listing(&server, "F_DEEP", vec![file("D_DEEP", DOC, "F_DEEP")]).await;
    contents(&server).await;

    let page = source(&server)
        .read_page(&token(), request(Some(held(&["D1"]))))
        .await
        .expect("a page");
    assert_eq!(page.removed, ["D_SUB"]);
    assert_eq!(
        texts(&page),
        BTreeMap::from([
            ("D_DEEP", "D_DEEP as text/plain"),
            ("D_NEW", "D_NEW as text/plain"),
        ])
    );
    let cursor = cursor_of(&page);
    assert_eq!(
        cursor.folders,
        BTreeMap::from([
            ("F_DEEP".to_owned(), "F_NEW".to_owned()),
            ("F_NEW".to_owned(), ROOT.to_owned()),
        ])
    );
    assert_eq!(cursor.files["D_DEEP"], "F_DEEP");
    assert!(!cursor.files.contains_key("D_SUB"));
}

#[tokio::test]
async fn the_chosen_folder_trashed_is_source_missing() {
    let server = MockServer::start().await;
    changes(
        &server,
        "100",
        vec![
            change("D1", Some(file("D1", DOC, ROOT))),
            change(
                ROOT,
                Some(json!({"id": ROOT, "name": "Farm", "mimeType": FOLDER_MIME,
                    "parents": ["root"], "trashed": true})),
            ),
        ],
        "200",
    )
    .await;
    let error = source(&server)
        .read_page(&token(), request(Some(held(&["D1"]))))
        .await
        .expect_err("the folder is gone");
    assert!(
        matches!(&error, FarmSourceError::SourceMissing { source_id, .. } if source_id == ROOT),
        "{error:?}"
    );
}

#[tokio::test]
async fn text_is_cut_at_1_mb() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    listing(&server, ROOT, vec![file("D_BIG", DOC, ROOT)]).await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_BIG/export"))
        .respond_with(ResponseTemplate::new(200).set_body_string("spray ".repeat(200_000)))
        .mount(&server)
        .await;
    let page = source(&server)
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    assert_eq!(page.items[0].text.len(), MAX_TEXT_BYTES);
}

#[tokio::test]
async fn a_file_drive_will_not_hand_over_is_kept_by_name_and_one_gone_is_removed() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    listing(
        &server,
        ROOT,
        vec![
            file("T_LOCKED", "text/plain", ROOT),
            file("D_GONE", DOC, ROOT),
        ],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/T_LOCKED"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": {"code": 403,
            "errors": [{"reason": "fileNotDownloadable"}]}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_GONE/export"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"error": {"code": 404,
            "errors": [{"reason": "notFound"}]}})),
        )
        .mount(&server)
        .await;
    let page = source(&server)
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    assert_eq!(texts(&page), BTreeMap::from([("T_LOCKED", "")]));
    assert_eq!(page.removed, ["D_GONE"]);
    assert!(!cursor_of(&page).files.contains_key("D_GONE"));
}

#[tokio::test]
async fn a_429_fails_the_page_with_its_retry_after() {
    let server = MockServer::start().await;
    changes(
        &server,
        "100",
        vec![change("D1", Some(file("D1", DOC, ROOT)))],
        "200",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D1/export"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    let error = source(&server)
        .read_page(&token(), request(Some(held(&["D1"]))))
        .await
        .expect_err("throttled");
    assert!(
        matches!(
            error,
            FarmSourceError::RateLimited {
                retry_after_s: Some(120),
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_cursor_round_trips_and_anything_else_starts_a_first_sync() {
    let cursor = Cursor {
        token: "100".into(),
        folders: BTreeMap::from([("F_SUB".to_owned(), ROOT.to_owned())]),
        files: BTreeMap::from([("D1".to_owned(), ROOT.to_owned())]),
        queue: VecDeque::from([Queued {
            id: "D1".into(),
            name: "Spray log".into(),
            mime: DOC.into(),
            at: Some(now()),
            link: None,
        }]),
    };
    assert_eq!(Cursor::parse(Some(&cursor.to_sync_cursor())), Some(cursor));
    assert_eq!(Cursor::parse(None), None);
    assert_eq!(
        Cursor::parse(Some(&SyncCursor(r#"{"newest":"1.0"}"#.into()))),
        None,
        "a Slack cursor"
    );
}

#[test]
fn the_latest_change_of_a_file_wins_and_a_cycle_reaches_nothing() {
    let older = serde_json::from_value::<Change>(json!({"fileId": "D1", "removed": true,
        "time": "2026-10-15T08:00:00.000Z"}))
    .expect("a change");
    let newer = serde_json::from_value::<Change>(change("D1", Some(file("D1", DOC, ROOT))))
        .expect("a change");
    let set = [newer.clone(), older];
    assert_eq!(latest(&set)["D1"], &newer);

    let cycle = BTreeMap::from([
        ("A".to_owned(), "B".to_owned()),
        ("B".to_owned(), "A".to_owned()),
    ]);
    assert!(!reaches(ROOT, "A", &cycle));
    let chain = BTreeMap::from([
        ("A".to_owned(), "B".to_owned()),
        ("B".to_owned(), ROOT.to_owned()),
    ]);
    assert!(reaches(ROOT, "A", &chain));
}
