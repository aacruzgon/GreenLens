//! The Slack and Google Drive clients against our hosted Era company, Holling Holdings (docs/EON
//! T-E7.1, E7.2): Slack lists the bot's channels and reads one page of a channel's history; Drive
//! lists the folders, lists a folder and exports a Google Doc as text. Ignored by default, and
//! not run by the `era` job (its filters leave out `era_hosted_`); the nightly `era-hosted`
//! workflow runs them (docs/infra section 2):
//!
//! ```sh
//! set -a; . infra/env/.env.era; set +a   # SLACK_BASE_URL, GOOGLE_DRIVE_BASE_URL, ERA_TENANT_TOKEN
//! cd backend && cargo nextest run -p integrations --run-ignored only --no-capture \
//!   -E 'test(/(^|::)era_hosted_/)'
//! ```
//!
//! **Every request costs one of the account's 2,000 calls a month,** so each test makes as few as
//! it can and prints how many it made. The hosted company has no admin API: no inject, no faults,
//! no journal, and its data is Era's own, not our farm. The token is the company's: a tenant token
//! (7 days) or one minted with `era client` credentials (1 hour).

#![expect(
    clippy::print_stderr,
    reason = "the nightly job's log records the calls each run cost"
)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use integrations::{
    era_sim::ERA_TIMEOUT,
    google_drive::{DriveClient, DriveConfig, ExportFormat, FileKind},
    http::{HttpConfig, SecretString},
    slack::{SlackClient, SlackConfig},
};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// The folder the Drive test lists, as an owner would choose one: Holling Holdings' engineering
/// runbooks, a dozen Google Docs and no subfolder, so the listing is one `files.list` call. The
/// company's Drive is far larger than `era plan` said (over 1,000 folders and 1,000 Docs on Oct
/// 20), so the test never lists from the root. Era names folders by their path, and the company
/// is generated from a fixed recipe, so the id holds until the company is rebuilt.
const FOLDER: &str = "folder-support-knowledge-base-engineering-runbooks";

/// The pacing key `conversations.history` takes. `SlackConfig::era` turns pacing off, so it only
/// has to be stable, and the test saves the `auth.test` call that would give the team id.
const PACING_KEY: &str = "era-hosted";

/// A variable of the `era-hosted` environment. Panics when it is unset, so a misconfigured job
/// fails rather than passes by doing nothing.
fn var(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value.trim_end_matches('/').to_owned(),
        _ => panic!(
            "{name} is not set: load infra/env/.env.era, or run in the `era-hosted` GitHub \
             environment (docs/EON E6.5)"
        ),
    }
}

/// Counts DNS lookups. The client keeps no idle connections, so each request opens its own
/// connection and looks the host up once: the count is the number of requests, retries
/// included, which is what Era meters.
struct CountingResolver(Arc<AtomicUsize>);

impl Resolve for CountingResolver {
    fn resolve(&self, name: Name) -> Resolving {
        self.0.fetch_add(1, Ordering::Relaxed);
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<_> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// The HTTP client the tests give the vendor clients, and its request count, printed when the
/// test ends, also when it fails.
struct Hosted {
    system: &'static str,
    http: reqwest::Client,
    calls: Arc<AtomicUsize>,
    token: SecretString,
}

impl Hosted {
    fn new(system: &'static str) -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let config = HttpConfig::default();
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .user_agent(config.user_agent)
            .pool_max_idle_per_host(0)
            .http1_only()
            .dns_resolver(Arc::new(CountingResolver(Arc::clone(&calls))))
            .build()
            .expect("the HTTP client builds");
        Self {
            system,
            http,
            calls,
            token: SecretString::from(var("ERA_TENANT_TOKEN")),
        }
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        eprintln!(
            "era-hosted {}: {} metered calls",
            self.system,
            self.calls.load(Ordering::Relaxed)
        );
    }
}

#[tokio::test]
#[ignore = "needs our hosted Era company; each request is metered"]
async fn era_hosted_slack_lists_channels_and_reads_history() {
    let hosted = Hosted::new("slack");
    let slack = SlackClient::new(
        hosted.http.clone(),
        SlackConfig {
            timeout: ERA_TIMEOUT,
            ..SlackConfig::era(var("SLACK_BASE_URL"))
        },
    );

    let mut channels = slack.conversations_list(&hosted.token).await.unwrap();
    assert!(!channels.is_empty(), "the bot is in no channel");
    channels.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    let page = slack
        .conversations_history(&hosted.token, PACING_KEY, &channels[0].id, None, None)
        .await
        .unwrap();
    assert!(
        !page.messages.is_empty(),
        "the first channel has no messages"
    );
    eprintln!(
        "era-hosted slack: {} channels, {} messages on the first page",
        channels.len(),
        page.messages.len()
    );
}

#[tokio::test]
#[ignore = "needs our hosted Era company; each request is metered"]
async fn era_hosted_drive_lists_a_folder_and_exports_a_doc() {
    let hosted = Hosted::new("drive");
    let drive = DriveClient::new(
        hosted.http.clone(),
        DriveConfig {
            api_base: var("GOOGLE_DRIVE_BASE_URL"),
            timeout: ERA_TIMEOUT,
            ..DriveConfig::default()
        },
    );

    // The source picker's call: one page of 1,000 folders is already past its cap of 500.
    let folders = drive.folders(&hosted.token).await.unwrap();
    assert!(!folders.is_empty(), "the company has no folder");

    let listing = drive.list_folder(&hosted.token, FOLDER).await.unwrap();
    let doc = listing
        .files
        .iter()
        .find(|f| f.kind() == FileKind::Document)
        .unwrap_or_else(|| panic!("no Google Doc in {FOLDER}"));
    let text = drive
        .export(&hosted.token, &doc.id, ExportFormat::PlainText)
        .await
        .unwrap();
    assert!(!text.text.trim().is_empty(), "the exported Doc is empty");
    eprintln!(
        "era-hosted drive: {} folders for the picker, {} files in the folder, a Doc of {} bytes",
        folders.len(),
        listing.files.len(),
        text.text.len()
    );
}
