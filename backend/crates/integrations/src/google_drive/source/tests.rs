//! [`DriveSource`] against `wiremock` (docs/EON T-E4.3, T-E4.5): the listing (the chosen
//! folder checked, the start token, the recursive listing, every id listed), the pages of the
//! queue, the changes read a page at a time and kept under the folder (moved out, trashed,
//! deleted, nested folders moved in and out, a shared drive's own change), a refused change
//! token resyncing, the 500-file cap as `too_large`, a file whose read keeps failing, the text
//! of each kind and its 1 MB cut, and a `429` that fails the page so the sync keeps its cursor.

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex, query_param},
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

/// A page request of the run that started at `now() + run × 15 min`.
fn request_in(run: u32, cursor: Option<SyncCursor>) -> FarmPageRequest {
    FarmPageRequest {
        account: ConnectorAccount::GoogleDrive {
            email: "farm@example.org".into(),
        },
        source: ConnectorSource {
            id: ROOT.into(),
            name: "Farm records".into(),
        },
        cursor,
        oldest: now()
            .plus(Duration::from_mins(15) * run)
            .minus(FARM_RECORD_KEEP),
        reread_from: None,
        probe_limit: 2,
    }
}

fn request(cursor: Option<SyncCursor>) -> FarmPageRequest {
    request_in(0, cursor)
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

fn google_error(code: u16, reason: &str) -> ResponseTemplate {
    ResponseTemplate::new(code)
        .set_body_json(json!({"error": {"code": code, "errors": [{"reason": reason}]}}))
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

/// The chosen folder as `files.get` answers it, and the start token `token`.
async fn start_token(server: &MockServer, token: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/drive/v3/files/{ROOT}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(file(ROOT, FOLDER_MIME, "root")))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes/startPageToken"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "startPageToken": token })))
        .mount(server)
        .await;
}

/// `changes.list` from `from` answers `changes` in one page, ending with `next`.
async fn changes(server: &MockServer, from: &str, changes: Vec<Value>, next: &str) {
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .and(query_param("pageToken", from))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "changes": changes, "newStartPageToken": next })),
        )
        .mount(server)
        .await;
}

fn id_of(r: &wiremock::Request) -> String {
    r.url
        .path()
        .split('/')
        .nth(4)
        .unwrap_or_default()
        .to_owned()
}

/// Every Doc and Sheet exports as `<id> as <format>`, every plain-text file downloads as
/// `<id> downloaded`.
async fn contents(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/drive/v3/files/[^/]+/export$"))
        .respond_with(|r: &wiremock::Request| {
            let format = r
                .url
                .query_pairs()
                .find(|(k, _)| k == "mimeType")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            ResponseTemplate::new(200).set_body_string(format!("{} as {format}", id_of(r)))
        })
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/drive/v3/files/[^/]+$"))
        .and(query_param("alt", "media"))
        .respond_with(|r: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_string(format!("{} downloaded", id_of(r)))
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

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// A cursor holding `files` in the root, `D_SUB` in `F_SUB`, with no pass in progress.
fn held_cursor(files: &[&str]) -> Cursor {
    let mut held: BTreeMap<String, String> = files
        .iter()
        .map(|f| ((*f).to_owned(), ROOT.to_owned()))
        .collect();
    held.insert("D_SUB".into(), "F_SUB".into());
    Cursor {
        token: "100".into(),
        folders: BTreeMap::from([("F_SUB".to_owned(), ROOT.to_owned())]),
        files: held,
        ..Cursor::default()
    }
}

fn held(files: &[&str]) -> SyncCursor {
    held_cursor(files).to_sync_cursor()
}

fn paths(requests: &[wiremock::Request]) -> Vec<String> {
    requests.iter().map(|r| r.url.path().to_owned()).collect()
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
    assert_eq!(
        sorted(page.listed.clone().expect("a listing lists")),
        ["D1", "D_SUB", "P1", "S1", "T1"],
        "every file listed, for the sync to delete what is no longer there"
    );
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

    // The folder is checked, then the start token comes before the listing, so nothing changed
    // during it is missed.
    let paths = paths(&server.received_requests().await.expect("recorded"));
    assert_eq!(paths[0], format!("/drive/v3/files/{ROOT}"));
    assert_eq!(paths[1], "/drive/v3/changes/startPageToken");
    assert_eq!(paths[2], "/drive/v3/files");
}

#[tokio::test]
async fn a_chosen_folder_gone_trashed_or_not_a_folder_is_source_missing_on_a_first_sync() {
    let answers = [
        google_error(404, "notFound"),
        ResponseTemplate::new(200).set_body_json(
            json!({"id": ROOT, "name": "Farm", "mimeType": FOLDER_MIME, "trashed": true}),
        ),
        ResponseTemplate::new(200).set_body_json(file(ROOT, DOC, "root")),
    ];
    for answer in answers {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/drive/v3/files/{ROOT}")))
            .respond_with(answer)
            .mount(&server)
            .await;
        let error = source(&server)
            .read_page(&token(), request(None))
            .await
            .expect_err("no folder to read");
        assert!(
            matches!(&error, FarmSourceError::SourceMissing { source_id, .. } if source_id == ROOT),
            "{error:?}"
        );
        let calls = server.received_requests().await.expect("recorded");
        assert_eq!(calls.len(), 1, "nothing read after the folder");
    }
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
    assert_eq!(second.listed, None, "listed on the first page only");
    assert_eq!(second.items[0].vendor_id, "P25");
    let calls = server.received_requests().await.expect("recorded");
    assert_eq!(calls.len(), 3, "listed once, no changes read mid-pass");
}

#[tokio::test]
async fn a_folder_over_500_files_is_too_large() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    let files: Vec<Value> = (0..=MAX_FILES)
        .map(|n| file(&format!("P{n:03}"), PDF, ROOT))
        .collect();
    listing(&server, ROOT, files).await;
    let error = source(&server)
        .read_page(&token(), request(None))
        .await
        .expect_err("over the cap");
    assert!(
        matches!(&error, FarmSourceError::TooLarge { source_id, .. } if source_id == ROOT),
        "{error:?}"
    );

    // A change that adds the 501st file stops the same way.
    let server = MockServer::start().await;
    changes(
        &server,
        "100",
        vec![change("N_LAST", Some(file("N_LAST", PDF, ROOT)))],
        "200",
    )
    .await;
    let full: Vec<String> = (1..MAX_FILES).map(|n| format!("P{n:03}")).collect();
    let full: Vec<&str> = full.iter().map(String::as_str).collect();
    let error = source(&server)
        .read_page(&token(), request(Some(held(&full))))
        .await
        .expect_err("over the cap");
    assert!(
        matches!(error, FarmSourceError::TooLarge { .. }),
        "{error:?}"
    );
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
            // A shared drive's own change has no file id: skipped, and the page still parses.
            json!({"changeType": "drive", "driveId": "0AFARM", "removed": false,
                "time": "2026-10-15T09:00:00.000Z"}),
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
    assert_eq!(sorted(page.removed.clone()), ["D2", "D3", "D4"]);
    assert_eq!(page.listed, None);
    let cursor = cursor_of(&page);
    assert_eq!(cursor.token, "200", "the last page's newStartPageToken");
    assert!(!cursor.paging);
    assert_eq!(
        cursor.files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["D1", "D_SUB", "N1"]
    );
    assert!(!page.more);
}

#[tokio::test]
async fn a_change_backlog_is_read_one_page_at_a_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/changes"))
        .and(query_param("pageToken", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "changes": [change("D1", Some(file("D1", DOC, ROOT)))],
            "nextPageToken": "150"
        })))
        .mount(&server)
        .await;
    changes(
        &server,
        "150",
        vec![change("D2", Some(file("D2", DOC, ROOT)))],
        "200",
    )
    .await;
    contents(&server).await;
    let drive = source(&server);

    let first = drive
        .read_page(&token(), request(Some(held(&["D1", "D2"]))))
        .await
        .expect("a page");
    assert_eq!(texts(&first), BTreeMap::from([("D1", "D1 as text/plain")]));
    assert!(first.more, "the backlog goes on");
    let cursor = cursor_of(&first);
    assert_eq!(cursor.token, "150", "the next page's token, saved");
    assert!(cursor.paging);

    let second = drive
        .read_page(&token(), request(Some(first.cursor)))
        .await
        .expect("a page");
    assert_eq!(texts(&second), BTreeMap::from([("D2", "D2 as text/plain")]));
    assert!(!second.more);
    let cursor = cursor_of(&second);
    assert_eq!(cursor.token, "200");
    assert!(!cursor.paging);
}

#[tokio::test]
async fn a_change_token_drive_refuses_lists_the_folder_again() {
    for refusal in [google_error(400, "invalid"), google_error(404, "notFound")] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/drive/v3/changes"))
            .respond_with(refusal)
            .mount(&server)
            .await;
        start_token(&server, "500").await;
        listing(&server, ROOT, vec![file("D1", DOC, ROOT)]).await;
        contents(&server).await;
        let page = source(&server)
            .read_page(&token(), request(Some(held(&["D1", "D_OLD"]))))
            .await
            .expect("a resync");
        // Nothing is deleted on the adapter's word: the listing says what is there, and the
        // sync deletes the rest after storing the page.
        assert_eq!(page.removed, Vec::<String>::new());
        assert_eq!(page.listed, Some(vec!["D1".to_owned()]));
        assert_eq!(texts(&page), BTreeMap::from([("D1", "D1 as text/plain")]));
        let cursor = cursor_of(&page);
        assert_eq!(cursor.token, "500", "a new start token");
        assert!(cursor.folders.is_empty(), "the tree as listed now");
    }
}

#[tokio::test]
async fn a_cursor_it_cannot_read_lists_the_folder_again() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    listing(&server, ROOT, vec![file("D1", DOC, ROOT)]).await;
    contents(&server).await;
    let page = source(&server)
        .read_page(
            &token(),
            request(Some(SyncCursor(r#"{"an":"older shape"}"#.into()))),
        )
        .await
        .expect("a listing");
    assert_eq!(page.listed, Some(vec!["D1".to_owned()]));
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

#[test]
fn a_change_with_no_file_that_is_not_a_removal_is_ignored() {
    let mut cursor = held_cursor(&["D1"]);
    let before = cursor.clone();
    let bare = |id: &str| {
        serde_json::from_value::<Change>(
            json!({"fileId": id, "removed": false, "time": "2026-10-15T09:00:00.000Z"}),
        )
        .expect("a change")
    };
    // The chosen folder, a held folder and a held file.
    let applied = cursor
        .apply(ROOT, &[bare(ROOT), bare("F_SUB"), bare("D1")])
        .expect("applied");
    assert_eq!(applied, Applied::default());
    assert_eq!(cursor, before);
}

#[test]
fn a_listed_file_outside_the_tree_is_left_out() {
    let mut cursor = held_cursor(&[]);
    let stray: DriveFile =
        serde_json::from_value(file("D_STRAY", DOC, "F_UNKNOWN")).expect("a file");
    cursor.add_listed(ROOT, &stray).expect("under the cap");
    assert!(!cursor.files.contains_key("D_STRAY"));
    assert!(cursor.queue.is_empty());
}

#[tokio::test]
async fn text_is_cut_at_1_mb() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    listing(
        &server,
        ROOT,
        vec![file("D_BIG", DOC, ROOT), file("D_FIELD", DOC, ROOT)],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_BIG/export"))
        .respond_with(ResponseTemplate::new(200).set_body_string("spray ".repeat(200_000)))
        .mount(&server)
        .await;
    // Three bytes a character: the cut drops the character it would split, and no U+FFFD.
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_FIELD/export"))
        .respond_with(ResponseTemplate::new(200).set_body_string("畑".repeat(400_000)))
        .mount(&server)
        .await;
    let page = source(&server)
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    let text = texts(&page);
    assert_eq!(text["D_BIG"].len(), MAX_TEXT_BYTES);
    assert_eq!(text["D_FIELD"].len(), MAX_TEXT_BYTES - 1);
    assert!(text["D_FIELD"].chars().all(|c| c == '畑'));
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
            file("T_BINARY", "text/plain", ROOT),
            file("D_GONE", DOC, ROOT),
        ],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/T_LOCKED"))
        .and(query_param("alt", "media"))
        .respond_with(google_error(403, "fileNotDownloadable"))
        .mount(&server)
        .await;
    // A binary file labelled text/plain: mostly bytes no text has.
    let bytes: Vec<u8> = (0..=u8::MAX).cycle().take(4_000).collect();
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/T_BINARY"))
        .and(query_param("alt", "media"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_GONE/export"))
        .respond_with(google_error(404, "notFound"))
        .mount(&server)
        .await;
    let page = source(&server)
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    assert_eq!(
        texts(&page),
        BTreeMap::from([("T_BINARY", ""), ("T_LOCKED", "")])
    );
    assert_eq!(page.removed, ["D_GONE"]);
    assert!(!cursor_of(&page).files.contains_key("D_GONE"));
}

#[tokio::test]
async fn a_file_whose_read_keeps_failing_holds_up_nothing_and_is_skipped_after_5_runs() {
    let server = MockServer::start().await;
    start_token(&server, "100").await;
    listing(
        &server,
        ROOT,
        vec![file("D_BAD", DOC, ROOT), file("D_OK", DOC, ROOT)],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/D_BAD/export"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&server)
        .await;
    contents(&server).await;
    let drive = source(&server);

    // Run 0: the bad file goes to the back, the next one is read.
    let page = drive
        .read_page(&token(), request(None))
        .await
        .expect("a page");
    assert_eq!(
        texts(&page),
        BTreeMap::from([("D_OK", "D_OK as text/plain")])
    );
    assert!(page.more, "the bad file waits for the next run");
    let queued = cursor_of(&page).queue;
    let mut cursor = page.cursor;
    assert_eq!(queued.len(), 1);
    assert_eq!((queued[0].tries, queued[0].failed_at), (1, Some(now())));
    // In the same run, only it is left: the page fails and the run stops.
    let error = drive
        .read_page(&token(), request(Some(cursor.clone())))
        .await
        .expect_err("only files that failed in this run");
    assert_eq!(error.code().as_str(), "vendor_unavailable");

    // Each later run tries it once; the fifth failure skips it.
    for run in 1..u32::from(MAX_TRIES) {
        let page = drive
            .read_page(&token(), request_in(run, Some(cursor)))
            .await
            .expect("a page with the failure counted");
        assert_eq!(page.items, Vec::new());
        cursor = page.cursor;
    }
    let left = Cursor::parse(Some(&cursor)).expect("a cursor");
    assert!(left.queue.is_empty(), "skipped after {MAX_TRIES} runs");
    assert!(left.files.contains_key("D_BAD"), "its record stays");
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
fn a_cursor_round_trips_and_anything_else_starts_a_listing() {
    let cursor = Cursor {
        token: "100".into(),
        paging: true,
        folders: BTreeMap::from([("F_SUB".to_owned(), ROOT.to_owned())]),
        files: BTreeMap::from([("D1".to_owned(), ROOT.to_owned())]),
        queue: VecDeque::from([Queued {
            id: "D1".into(),
            name: "Spray log".into(),
            mime: DOC.into(),
            at: Some(now()),
            link: None,
            tries: 2,
            failed_at: Some(now()),
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
