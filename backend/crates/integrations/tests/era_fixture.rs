//! T-E0.2 automated: the Maple Row Farm fixture reads back from Era's simulators with the counts
//! `tests/fixtures/era/maple-row/README.md` records. Ignored by default:
//!
//! ```sh
//! docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
//! ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
//!   cargo nextest run -p integrations --run-ignored only -E 'test(/era/)'
//! ```

use integrations::era_sim::{SimKind, Tenant, fixture};
use serde_json::Value;

async fn get(tenant: &Tenant, path: &str, query: &[(&str, &str)]) -> Value {
    let response = reqwest::Client::new()
        .get(format!("{}{path}", tenant.base()))
        .bearer_auth(tenant.token().await)
        .query(query)
        .send()
        .await
        .expect("simulator unreachable");
    let status = response.status();
    let text = response.text().await.expect("body");
    assert!(status.is_success(), "GET {path}: {status}: {text}");
    serde_json::from_str(&text).expect("JSON body")
}

async fn slack(tenant: &Tenant, method: &str, query: &[(&str, &str)]) -> Value {
    get(tenant, &format!("/api/{method}"), query).await
}

/// Every item under `key` of a Slack method, paging by cursor.
async fn paged(tenant: &Tenant, method: &str, key: &str, query: &[(&str, &str)]) -> Vec<Value> {
    let mut items = Vec::new();
    let mut cursor = String::new();
    loop {
        let mut query = query.to_vec();
        if !cursor.is_empty() {
            query.push(("cursor", &cursor));
        }
        let page = slack(tenant, method, &query).await;
        assert_eq!(page["ok"], true, "{page}");
        items.extend(page[key].as_array().expect("array").clone());
        page["response_metadata"]["next_cursor"]
            .as_str()
            .unwrap_or_default()
            .clone_into(&mut cursor);
        if cursor.is_empty() {
            return items;
        }
    }
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_maple_row_slack_reads_back() {
    let tenant = Tenant::create(SimKind::Slack, Some(&fixture("maple-row", "slack.json"))).await;

    let channels = paged(
        &tenant,
        "conversations.list",
        "channels",
        &[
            ("types", "public_channel,private_channel"),
            ("limit", "200"),
        ],
    )
    .await;
    for id in ["C_FIELD", "C_CREW"] {
        let channel = channels
            .iter()
            .find(|c| c["id"] == id)
            .unwrap_or_else(|| panic!("{id} not listed"));
        assert_eq!(channel["is_member"], true, "{id}");
    }

    let mut counts = Vec::new();
    let (mut threads, mut replies) = (0, 0);
    for channel in ["C_FIELD", "C_CREW"] {
        let messages = paged(
            &tenant,
            "conversations.history",
            "messages",
            &[("channel", channel), ("limit", "50")],
        )
        .await;
        counts.push(messages.len());
        for message in messages
            .iter()
            .filter(|m| m["reply_count"].as_u64().unwrap_or(0) > 0)
        {
            let ts = message["ts"].as_str().expect("ts");
            let thread = paged(
                &tenant,
                "conversations.replies",
                "messages",
                &[("channel", channel), ("ts", ts), ("limit", "200")],
            )
            .await;
            threads += 1;
            // The first message is the parent.
            replies += thread.len() - 1;
        }
    }
    assert_eq!(counts, [55, 39]);
    assert_eq!((threads, replies), (20, 29));

    // The journal saw the calls the token made.
    assert!(
        tenant
            .requests()
            .await
            .iter()
            .any(|r| r.route == "/api/conversations.history" && r.status == 200)
    );
    tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_maple_row_channels_are_isolated_per_tenant() {
    let farm = Tenant::create(SimKind::Slack, Some(&fixture("maple-row", "slack.json"))).await;
    let other = Tenant::create(SimKind::Slack, None).await;
    let found = slack(&other, "conversations.info", &[("channel", "C_FIELD")]).await;
    assert_eq!(found["ok"], false);
    assert_eq!(found["error"], "channel_not_found");
    let found = slack(&farm, "conversations.info", &[("channel", "C_FIELD")]).await;
    assert_eq!(found["ok"], true, "{found}");
    farm.close().await;
    other.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_maple_row_drive_reads_back() {
    let tenant = Tenant::create(
        SimKind::Drive,
        Some(&fixture("maple-row", "google-drive.json")),
    )
    .await;

    let list = get(
        &tenant,
        "/drive/v3/files",
        &[
            ("q", "'F_FARM' in parents"),
            ("fields", "files(id,name,mimeType)"),
            ("pageSize", "100"),
        ],
    )
    .await;
    let files = list["files"].as_array().expect("files");
    assert_eq!(files.len(), 5, "{list}");
    let count = |mime: &str| files.iter().filter(|f| f["mimeType"] == mime).count();
    assert_eq!(count("application/vnd.google-apps.document"), 3);
    assert_eq!(count("application/vnd.google-apps.spreadsheet"), 1);
    assert_eq!(count("application/pdf"), 1);

    let response = reqwest::Client::new()
        .get(format!("{}/drive/v3/files/S_PLAN/export", tenant.base()))
        .bearer_auth(tenant.token().await)
        .query(&[("mimeType", "text/csv")])
        .send()
        .await
        .expect("simulator unreachable");
    assert!(response.status().is_success());
    let csv = response.text().await.expect("csv");
    assert!(csv.lines().count() > 1, "first tab has rows: {csv}");
    tenant.close().await;
}
