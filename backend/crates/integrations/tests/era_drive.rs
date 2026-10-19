//! The Drive client against Era's Google Drive simulator on the Maple Row Farm fixture: T-E2.2,
//! T-E2.4, and the `fail_next` and `error_rate` halves of T-E2.5 (docs/EON). Ignored by default:
//!
//! ```sh
//! docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
//! ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
//!   cargo nextest run -p integrations --run-ignored only -E 'test(/era/) & binary(era_drive)'
//! ```
//!
//! Faults are global to the simulator. The `era_fault_*` tests hold [`sim_lock`] exclusively
//! and every other test here holds it shared, so no test of this binary sees another's fault,
//! even though nextest runs each test in its own process.

use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    time::{Duration, Instant},
};

use integrations::{
    era_sim::{Faults, SimKind, Tenant, fixture},
    farm::{FarmClientError, FarmErrorCode},
    google_drive::{DriveClient, DriveConfig, DriveFile, ExportFormat, FileKind},
    http::{RetryPolicy, SecretString},
};
use serde_json::{Value, json};

/// Locks the Drive simulator against the fault tests of this binary: `exclusive` for a test
/// that sets faults, shared for the rest. Unlocked when the file drops.
async fn sim_lock(exclusive: bool) -> File {
    tokio::task::spawn_blocking(move || {
        let path = std::env::temp_dir().join("pdm-era-drive-faults.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .expect("lock file opens");
        if exclusive {
            file.lock().expect("exclusive lock");
        } else {
            file.lock_shared().expect("shared lock");
        }
        file
    })
    .await
    .expect("lock task")
}

fn maple_row() -> Value {
    fixture("maple-row", "google-drive.json")
}

fn drive(tenant: &Tenant, retry: RetryPolicy, page_size: u32) -> DriveClient {
    DriveClient::new(
        reqwest::Client::new(),
        DriveConfig {
            api_base: tenant.base().to_owned(),
            token_url: format!("{}/token", tenant.base()),
            revoke_url: format!("{}/o/oauth2/revoke", tenant.base()),
            retry,
            page_size,
            ..DriveConfig::default()
        },
    )
}

async fn token(tenant: &Tenant) -> SecretString {
    SecretString::from(tenant.token().await)
}

/// Sets `faults` on the Drive simulator, runs `body`, and resets the faults whether or not
/// `body` panicked.
async fn with_faults(faults: Value, body: impl Future<Output = ()> + Send + 'static) {
    let _lock = sim_lock(true).await;
    Faults::set(SimKind::Drive, faults).await;
    let outcome = tokio::spawn(body).await;
    Faults::reset(SimKind::Drive).await;
    if let Err(error) = outcome {
        match error.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(error) => panic!("test body did not finish: {error}"),
        }
    }
}

/// T-E2.2: the folder, its text, the PDF, and `changes.list` to `newStartPageToken`.
#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_drive_reads_the_farm_folder_and_its_changes() {
    let _lock = sim_lock(false).await;
    let tenant = Tenant::create(SimKind::Drive, Some(&maple_row())).await;
    // Two files a page, so the folder's five files take three pages.
    let client = drive(&tenant, RetryPolicy::STANDARD, 2);
    let t = token(&tenant).await;

    let listing = client.list_folder(&t, "F_FARM").await.unwrap();
    assert!(!listing.truncated);
    assert_eq!(listing.folders, ["F_FARM"]);
    let ids: HashSet<&str> = listing.files.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(
        ids,
        HashSet::from(["D_SPRAY", "D_BEDMAP", "D_SOP", "F_LABEL", "S_PLAN"])
    );
    let kind = |id: &str| {
        listing
            .files
            .iter()
            .find(|f| f.id == id)
            .map(DriveFile::kind)
            .unwrap()
    };
    assert_eq!(kind("D_SPRAY"), FileKind::Document);
    assert_eq!(kind("S_PLAN"), FileKind::Spreadsheet);
    assert_eq!(kind("F_LABEL"), FileKind::Other);
    assert!(listing.files.iter().all(|f| f.parents == ["F_FARM"]
        && f.modified_time.is_some()
        && f.web_view_link.is_some()));
    let list_calls = tenant
        .requests()
        .await
        .iter()
        .filter(|r| r.route.contains("/files") && r.method == "GET")
        .count();
    assert_eq!(list_calls, 3, "five files at two a page");

    let doc = client
        .export(&t, "D_SPRAY", ExportFormat::PlainText)
        .await
        .unwrap();
    assert!(doc.text.starts_with("Spray log 2026"), "{doc:?}");
    assert!(!doc.truncated);

    let csv = client
        .export(&t, "S_PLAN", ExportFormat::Csv)
        .await
        .unwrap();
    let mut lines = csv.text.lines();
    assert_eq!(
        lines.next(),
        Some("bed,crop,variety,sown,transplanted,spacing_cm,first_harvest_expected"),
        "the first tab, Beds"
    );
    assert!(lines.count() >= 6, "one row a bed");

    let pdf = client.download_text(&t, "F_LABEL").await.unwrap_err();
    assert!(
        matches!(&pdf, FarmClientError::Refused { status: 403, code, .. } if code == "fileNotDownloadable"),
        "{pdf:?}"
    );

    let start = client.start_page_token(&t).await.unwrap();
    tenant.state("day2").await;
    // Era's documents say a move drops injections; we saw them kept. Inject again either way.
    tenant.inject(&maple_row()).await;
    let set = drive(&tenant, RetryPolicy::STANDARD, 1000)
        .changes(&t, &start)
        .await
        .unwrap();
    assert!(
        set.pages > 1,
        "Era's company changes too: {} pages",
        set.pages
    );
    assert_ne!(set.new_start_page_token, "");
    assert_ne!(set.new_start_page_token, start);
    let changed: HashSet<&str> = set.changes.iter().map(|c| c.file_id.as_str()).collect();
    for id in ["D_SPRAY", "D_BEDMAP", "D_SOP", "F_LABEL", "S_PLAN"] {
        assert!(
            changed.contains(id),
            "{id} not among {} changes",
            set.changes.len()
        );
    }
    let spray = set
        .changes
        .iter()
        .find(|c| c.file_id == "D_SPRAY")
        .and_then(|c| c.file.as_ref())
        .expect("the change carries the file");
    assert_eq!(spray.parents, ["F_FARM"]);

    tenant.close().await;
}

/// T-E2.4: no token is `401` in Google's shape, `auth_refused`; a `403` that isn't a rate
/// limit is never retried.
#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_drive_refusals_map_and_are_not_retried() {
    let _lock = sim_lock(false).await;
    let tenant = Tenant::create(SimKind::Drive, Some(&maple_row())).await;
    let client = drive(&tenant, RetryPolicy::STANDARD, 1000);

    let error = client
        .about_email(&SecretString::from(String::new()))
        .await
        .unwrap_err();
    assert!(
        matches!(
            &error,
            FarmClientError::AuthRefused {
                vendor: "google_drive",
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(error.code(), FarmErrorCode::AuthRefused);

    let t = token(&tenant).await;
    let email = client.about_email(&t).await.unwrap();
    assert!(email.contains('@'));

    let error = client.download_text(&t, "F_LABEL").await.unwrap_err();
    assert!(matches!(
        error,
        FarmClientError::Refused { status: 403, .. }
    ));
    let refused = tenant
        .requests()
        .await
        .iter()
        .filter(|r| r.status == 403)
        .count();
    assert_eq!(refused, 1, "the 403 was retried");

    let missing = client.export(&t, "NOPE", ExportFormat::PlainText).await;
    assert!(matches!(missing, Err(FarmClientError::NotFound { .. })));

    tenant.close().await;
}

/// T-E2.5: `fail_next` gives `429` with `Retry-After`, which the client waits out, then
/// succeeds.
#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_fail_next_is_waited_out() {
    let tenant = Tenant::create(SimKind::Drive, Some(&maple_row())).await;
    let t = token(&tenant).await;
    let client = drive(&tenant, RetryPolicy::STANDARD, 1000);
    with_faults(
        json!({"fail_next": 1, "retry_after_seconds": 2}),
        async move {
            let start = Instant::now();
            let token = client.start_page_token(&t).await.unwrap();
            assert_ne!(token, "");
            assert!(
                start.elapsed() >= Duration::from_secs(2),
                "Retry-After honoured: {:?}",
                start.elapsed()
            );
            tenant.close().await;
        },
    )
    .await;
}

/// T-E2.5: `error_rate: 1` gives `500` on every call: retried, then `vendor_unavailable`.
#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_error_rate_is_unavailable() {
    let tenant = Tenant::create(SimKind::Drive, Some(&maple_row())).await;
    let t = token(&tenant).await;
    let quick = RetryPolicy {
        max_retries: 2,
        base: Duration::from_millis(50),
        cap: Duration::from_millis(200),
        max_retry_after: Duration::from_secs(5),
    };
    let client = drive(&tenant, quick, 1000);
    with_faults(json!({"error_rate": 1}), async move {
        let error = client.list_folder(&t, "F_FARM").await.unwrap_err();
        assert!(
            matches!(error, FarmClientError::Unavailable { source: None, .. }),
            "{error:?}"
        );
        assert_eq!(error.code(), FarmErrorCode::VendorUnavailable);
        tenant.close().await;
    })
    .await;
}
