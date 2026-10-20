//! The Slack sync against Era's Slack simulator, on the Maple Row Farm fixture (docs/EON
//! T-E4.2, T-E4.4, T-E4.6, T-E4.7, T-E4.8, T-E4.11): the embedded store, a counting embedder and
//! the clock pinned to 2026-10-15T12:00:00Z; the client's token is the tenant's minted
//! credential. Ignored by default; with the simulators up:
//!
//! ```sh
//! docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
//! ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
//!   cargo nextest run -p api --run-ignored only -E 'test(/(^|::)era_/)'
//! ```
//!
//! Faults are global to the simulator: the nextest group `era` runs these one at a time, and a
//! fault test clears the Slack simulator's faults before and after, also when it fails. Only the
//! Slack simulator's faults are used here.

use std::{
    collections::BTreeMap,
    future::Future,
    io,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::FutureExt as _;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt::MakeWriter;

use domain::{
    authz::{Viewer, check_membership_of},
    connectors::{ConnectorEndpoint, ConnectorStatus, SyncErrorCode},
    id::{ConnectorId, GardenId},
    ports::{Clock as _, Embedding, FarmRecordFilter, FarmRecordMatch, FarmRecordStore},
    testing::{FixedClock, SequentialIds},
};
use integrations::{
    era_sim::{Faults, JournalEntry, SimKind, Tenant, fixture},
    http::{self, HttpConfig, RetryPolicy},
    slack::{SlackClient, SlackConfig, SlackSource, permalink},
};
use store::SurrealStore;

use super::{
    FarmSources, FarmSync, RunEnd, SYNC_EVERY, SyncConfig,
    tests::{CountingEmbedder, connector, garden, key, pinned, store},
};

const HISTORY: &str = "/api/conversations.history";
const REPLIES: &str = "/api/conversations.replies";

/// A synced farm: the tenant with the fixture, a garden with one `era` Slack connector over
/// the chosen channels, and the sync pointed at the tenant.
struct Farm {
    tenant: Tenant,
    store: Arc<SurrealStore>,
    clock: Arc<FixedClock>,
    embedder: Arc<CountingEmbedder>,
    sync: FarmSync,
    garden: GardenId,
    viewer: domain::authz::Membership<Viewer>,
    connector: ConnectorId,
}

async fn farm(channels: &[&str], edit: impl FnOnce(&mut SlackConfig)) -> Farm {
    let tenant = Tenant::create(SimKind::Slack, Some(&fixture("maple-row", "slack.json"))).await;
    let token = tenant.token().await;
    let mut config = SlackConfig::era(tenant.base());
    edit(&mut config);
    let http = http::client(&HttpConfig::default()).expect("the HTTP client builds");
    let source = Arc::new(SlackSource::new(SlackClient::new(http, config)));
    let store = store().await;
    let ids = SequentialIds::default();
    let owner = garden(&store, &ids, "maple").await;
    let clock = Arc::new(FixedClock::at(pinned()));
    let id = connector(
        &store,
        &ids,
        owner,
        ConnectorEndpoint::Era,
        channels,
        &token,
        clock.now(),
    )
    .await;
    // The account the seeding gives an `era` connector: links point at the simulator.
    store
        .db()
        .query("UPDATE $c SET account.url = $url;")
        .bind(("c", store::ids::rid(&id)))
        .bind(("url", format!("{}/", tenant.base())))
        .await
        .expect("account updated")
        .check()
        .expect("account updated");
    let embedder = Arc::new(CountingEmbedder::default());
    let sync = FarmSync::new(
        store.clone(),
        embedder.clone(),
        clock.clone(),
        key(),
        FarmSources::default().with(ConnectorEndpoint::Era, source),
        SyncConfig::default(),
    );
    let viewer = check_membership_of(store.as_ref(), owner.0, owner.1)
        .await
        .expect("the owner views");
    Farm {
        tenant,
        store,
        clock,
        embedder,
        sync,
        garden: owner.1,
        viewer,
        connector: id,
    }
}

impl Farm {
    async fn run(&self) -> Vec<(ConnectorId, RunEnd)> {
        self.sync
            .run_due(&CancellationToken::new())
            .await
            .expect("due connectors listed")
    }

    async fn count(&self) -> u32 {
        self.store
            .garden_record_count(self.garden)
            .await
            .expect("count")
    }

    /// Every record, by vendor id (the counting embedder's vectors are all equal).
    async fn records(&self) -> BTreeMap<String, FarmRecordMatch> {
        self.store
            .search_records(
                self.viewer,
                &Embedding(vec![1.0, 0.0, 0.0, 0.0]),
                FarmRecordFilter::default(),
                1_000,
            )
            .await
            .expect("search")
            .into_iter()
            .map(|m| (m.hit.citation.record_id.vendor_id.to_string(), m))
            .collect()
    }

    async fn calls(&self, route: &str) -> usize {
        count(&self.tenant.requests().await, route)
    }

    async fn row(&self) -> domain::ports::ConnectorRow {
        self.store
            .connector_row(self.connector)
            .await
            .expect("read")
            .expect("exists")
    }
}

fn count(journal: &[JournalEntry], route: &str) -> usize {
    journal
        .iter()
        .filter(|r| r.route == route && r.status == 200)
        .count()
}

/// The fixture's messages in one channel.
fn messages(channel: &str) -> Vec<Value> {
    fixture("maple-row", "slack.json")["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter(|m| m["channel"] == channel)
        .cloned()
        .collect()
}

/// The fixture's user names, as the sync keeps them: the display name.
fn names() -> BTreeMap<String, String> {
    fixture("maple-row", "slack.json")["users"]
        .as_array()
        .expect("users")
        .iter()
        .map(|u| {
            (
                u["id"].as_str().expect("id").to_owned(),
                u["profile"]["display_name"]
                    .as_str()
                    .expect("display name")
                    .to_owned(),
            )
        })
        .collect()
}

/// Runs `test` with the Slack simulator's faults set, clearing them before and after, also
/// when the test panics.
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
async fn era_slack_first_sync_stores_the_chosen_channel_with_authors_and_links() {
    let farm = farm(&["C_FIELD"], |_| {}).await;
    assert_eq!(farm.run().await, [(farm.connector, RunEnd::Synced)]);

    let expected = messages("C_FIELD");
    let names = names();
    let records = farm.records().await;
    assert_eq!(records.len(), expected.len(), "one record per message");
    assert_eq!(farm.count().await, 55);
    assert!(
        records.keys().all(|v| v.starts_with("C_FIELD/")),
        "nothing from the unchosen channel"
    );
    let base = format!("{}/", farm.tenant.base());
    for m in &expected {
        let ts = m["ts"].as_str().expect("ts");
        let record = &records[&format!("C_FIELD/{ts}")];
        let c = &record.hit.citation;
        assert_eq!(c.title, "#c_field");
        assert_eq!(
            c.author.as_deref(),
            Some(names[m["user"].as_str().expect("user")].as_str())
        );
        assert_eq!(c.link, permalink(&base, "C_FIELD", ts, None));
        let secs: i64 = ts.split('.').next().expect("secs").parse().expect("secs");
        assert_eq!(c.at.unix_seconds(), secs);
        let text = &record.hit.text;
        assert!(text.starts_with(m["text"].as_str().expect("text")));
        // A thread's replies are joined to it, each after its author's name.
        for reply in m["reply"].as_array().into_iter().flatten() {
            let line = format!(
                "{}: {}",
                names[reply["user"].as_str().expect("user")],
                reply["text"].as_str().expect("text")
            );
            assert!(text.contains(&line), "{ts} lacks a reply");
        }
    }
    let row = farm.row().await;
    assert_eq!(row.connector.synced_at, Some(pinned()));
    assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));
    assert_eq!(farm.embedder.texts(), 55);
    farm.tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_slack_later_syncs_read_only_what_is_new() {
    let farm = farm(&["C_FIELD", "C_CREW"], |_| {}).await;
    farm.run().await;
    assert_eq!(farm.count().await, 94);
    let embedded = farm.embedder.texts();

    // Nothing new: one `conversations.history` call per channel, no replies, no embedding.
    let (history, replies) = (farm.calls(HISTORY).await, farm.calls(REPLIES).await);
    farm.clock.advance(SYNC_EVERY);
    assert_eq!(farm.run().await, [(farm.connector, RunEnd::Synced)]);
    assert_eq!(farm.calls(HISTORY).await - history, 2);
    assert_eq!(farm.calls(REPLIES).await - replies, 0);
    assert_eq!(
        farm.embedder.texts(),
        embedded,
        "unchanged text keeps its vector (T-E4.7)"
    );

    // A new message arrives on the next sync.
    farm.tenant
        .inject(&json!({"messages": [{
            "channel": "C_FIELD", "user": "U_JUNE", "ts": "1792015200.000100",
            "text": "Garlic bed 4 mulched with straw."
        }]}))
        .await;
    farm.clock.advance(SYNC_EVERY);
    farm.run().await;
    assert_eq!(farm.count().await, 95);
    assert_eq!(farm.embedder.texts(), embedded + 1);

    // A new reply to a thread of the last 7 days shows in its parent's `latest_reply`, which
    // the daily re-read reads: only that thread's replies are fetched.
    let parent = "1792008000.012200";
    farm.tenant
        .inject(&json!({"messages": [{
            "channel": "C_CREW", "user": "U_MARIA", "ts": parent,
            "text": "Thanks everyone for a great season. Wrap-up lunch Friday at noon.",
            "reply": [{"user": "U_DEV", "ts": "1792016000.000100", "text": "I'll bring the cider."}]
        }]}))
        .await;
    let replies = farm.calls(REPLIES).await;
    farm.clock.advance(Duration::from_hours(24));
    farm.run().await;
    assert_eq!(farm.calls(REPLIES).await - replies, 1);
    let records = farm.records().await;
    assert!(
        records[&format!("C_CREW/{parent}")]
            .hit
            .text
            .contains("Dev: I'll bring the cider.")
    );
    assert_eq!(farm.count().await, 95, "the thread is still one record");
    farm.tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_slack_daily_reread_deletes_a_message_gone_at_the_vendor() {
    let farm = farm(&["C_FIELD", "C_CREW"], |_| {}).await;
    farm.run().await;
    assert_eq!(farm.count().await, 94);

    // The tenant again, without one message of the last 7 days.
    let gone = "1791829800.012100";
    let mut slack = fixture("maple-row", "slack.json");
    slack["messages"]
        .as_array_mut()
        .expect("messages")
        .retain(|m| m["ts"] != gone);
    farm.tenant.reset().await;
    farm.tenant.inject(&slack).await;

    // Within the day nothing is deleted; the re-read a day later deletes it.
    farm.clock.advance(SYNC_EVERY);
    farm.run().await;
    assert_eq!(farm.count().await, 94);
    farm.clock.advance(Duration::from_hours(24));
    assert_eq!(farm.run().await, [(farm.connector, RunEnd::Synced)]);
    assert_eq!(farm.count().await, 93);
    assert!(
        !farm
            .records()
            .await
            .contains_key(&format!("C_FIELD/{gone}"))
    );
    farm.tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_a_throttled_first_sync_resumes_without_duplicates() {
    // The client gives up on a `Retry-After` over 10 s, as a run must not wait a minute.
    let farm = farm(&["C_FIELD", "C_CREW"], |c| {
        c.retry = RetryPolicy {
            max_retry_after: Duration::from_secs(10),
            ..c.retry
        };
    })
    .await;
    // `quota_limit` lets the first calls through and then answers `429` with `retry-after: 60`,
    // so the pass stops midway; `fail_next` would stop it at its first call.
    with_faults(json!({ "quota_limit": 25 }), async {
        let ends = farm.run().await;
        assert_eq!(
            ends,
            [(farm.connector, RunEnd::Failed(SyncErrorCode::RateLimited))]
        );
    })
    .await;
    let row = farm.row().await;
    assert_eq!(row.last_error, Some(SyncErrorCode::RateLimited));
    assert_eq!(row.next_sync_at, pinned().plus(Duration::from_secs(60)));
    assert_eq!(row.connector.synced_at, None);
    let partial = farm.count().await;
    assert!(
        partial > 0 && partial < 94,
        "{partial} records after the throttle"
    );
    let history = farm.calls(HISTORY).await;

    farm.clock.advance(Duration::from_secs(60));
    assert_eq!(farm.run().await, [(farm.connector, RunEnd::Synced)]);
    assert_eq!(farm.count().await, 94, "every message once");
    assert_eq!(farm.records().await.len(), 94);
    // 4 pages of `#field-reports` and 3 of `#crew`, plus one call for the channel whose pass
    // had finished (nothing new). Only the page the throttle interrupted after its history
    // call (on a reply or a name) is read again.
    let total = farm.calls(HISTORY).await;
    assert!(
        (8..=9).contains(&total),
        "{total} history pages, {history} before the resume"
    );
    assert_eq!(farm.row().await.last_error, None);
    farm.tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_fail_next_is_waited_out_within_the_run() {
    let farm = farm(&["C_FIELD"], |_| {}).await;
    with_faults(json!({ "fail_next": 1, "retry_after_seconds": 1 }), async {
        assert_eq!(farm.run().await, [(farm.connector, RunEnd::Synced)]);
    })
    .await;
    assert_eq!(farm.count().await, 55);
    farm.tenant.close().await;
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_an_outage_is_vendor_unavailable_and_retried_15_minutes_later() {
    let farm = farm(&["C_FIELD"], |_| {}).await;
    with_faults(json!({ "error_rate": 1 }), async {
        let ends = farm.run().await;
        assert_eq!(
            ends,
            [(
                farm.connector,
                RunEnd::Failed(SyncErrorCode::VendorUnavailable)
            )]
        );
    })
    .await;
    let row = farm.row().await;
    assert_eq!(row.last_error, Some(SyncErrorCode::VendorUnavailable));
    assert_eq!(row.next_sync_at, pinned().plus(SYNC_EVERY));
    assert_eq!(row.connector.status, ConnectorStatus::Active);
    assert_eq!(farm.run().await.len(), 0, "not before 15 minutes");
    farm.clock.advance(SYNC_EVERY);
    assert_eq!(farm.run().await, [(farm.connector, RunEnd::Synced)]);
    assert_eq!(farm.count().await, 55);
    farm.tenant.close().await;
}

/// Collects every log line.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
#[ignore = "needs Era's simulators"]
async fn era_fault_logs_and_records_hold_no_email_and_no_vendor_body() {
    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(logs.clone())
        .finish();
    let _default = tracing::subscriber::set_default(subscriber);

    let farm = farm(&["C_FIELD", "C_CREW"], |_| {}).await;
    farm.run().await;
    with_faults(json!({ "error_rate": 1 }), async {
        farm.clock.advance(Duration::from_hours(24));
        farm.run().await;
    })
    .await;
    farm.clock.advance(SYNC_EVERY);
    farm.run().await;
    assert_eq!(farm.count().await, 94);

    let logged = String::from_utf8(logs.0.lock().expect("log buffer").clone()).expect("UTF-8");
    assert!(logged.contains("farm sync"), "the capture works");
    let slack = fixture("maple-row", "slack.json");
    let emails: Vec<&str> = slack["users"]
        .as_array()
        .expect("users")
        .iter()
        .map(|u| u["profile"]["email"].as_str().expect("email"))
        .collect();
    assert_eq!(emails.len(), 5);
    let records = farm.records().await;
    for email in &emails {
        assert!(!logged.contains(email), "an email in the logs");
        for r in records.values() {
            let c = &r.hit.citation;
            assert!(
                !r.hit.text.contains(email) && !c.author.as_deref().unwrap_or("").contains(email)
            );
        }
    }
    // No message text, which is what a vendor body would carry, reaches a log line.
    for m in slack["messages"].as_array().expect("messages") {
        let text = m["text"].as_str().expect("text");
        assert!(!logged.contains(text), "a message's text in the logs");
    }
    assert!(!logged.contains("\"ok\":"), "a Slack body in the logs");
    farm.tenant.close().await;
}
