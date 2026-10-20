//! E8.2 (docs/EON T-E8.2): a first sync of the fixture makes the vendor calls expected, and no
//! write. Read from the tenant's request journal, which records only the calls made with the
//! credential minted for the run (EON section 3), so nothing else is counted.

use std::collections::BTreeMap;

use serde::Serialize;

use integrations::era_sim::JournalEntry;

/// The Slack Web API methods a sync may call: all reads, all `GET`.
pub const SLACK_READS: [&str; 5] = [
    "/api/auth.test",
    "/api/conversations.list",
    "/api/conversations.history",
    "/api/conversations.replies",
    "/api/users.info",
];

/// What a first sync of the chosen channels must call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlackExpected {
    /// `conversations.history` pages: each channel's top-level messages, a page at a time.
    pub history_pages: usize,
    /// Threads in those channels: each read in full once, and no older-thread check on a
    /// first pass (connectors 6).
    pub threads: usize,
    /// Users in the workspace: `users.info` at most once each.
    pub users: usize,
}

/// The journal's verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalCheck {
    /// Calls by route.
    pub calls: BTreeMap<String, usize>,
    /// What didn't hold; empty when the check passed.
    pub problems: Vec<String>,
}

impl JournalCheck {
    /// Whether every expectation held.
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

fn counts(journal: &[JournalEntry]) -> BTreeMap<String, usize> {
    let mut calls = BTreeMap::new();
    for e in journal {
        *calls.entry(e.route.clone()).or_default() += 1;
    }
    calls
}

/// Every call answered 200 and was a `GET`; a write shows up as another method, or as a
/// refusal (Slack's `403 writes_disabled`, Drive's `403 insufficientPermissions`).
fn reads_only(journal: &[JournalEntry], problems: &mut Vec<String>) {
    for e in journal {
        if e.method != "GET" {
            problems.push(format!("{} {}: not a read", e.method, e.route));
        }
        if e.status != 200 {
            problems.push(format!("{} {}: status {}", e.method, e.route, e.status));
        }
    }
}

/// Checks the Slack tenant's journal after the first sync.
pub fn check_slack(journal: &[JournalEntry], expected: SlackExpected) -> JournalCheck {
    let mut problems = Vec::new();
    reads_only(journal, &mut problems);
    let calls = counts(journal);
    for route in calls.keys() {
        if !SLACK_READS.contains(&route.as_str()) {
            problems.push(format!("{route}: not a method the sync reads"));
        }
    }
    let n = |route: &str| calls.get(route).copied().unwrap_or_default();
    let history = n("/api/conversations.history");
    if history != expected.history_pages {
        problems.push(format!(
            "conversations.history: {history} calls for {} pages",
            expected.history_pages
        ));
    }
    let replies = n("/api/conversations.replies");
    if replies != expected.threads {
        problems.push(format!(
            "conversations.replies: {replies} calls for {} threads",
            expected.threads
        ));
    }
    let users = n("/api/users.info");
    if users > expected.users {
        problems.push(format!(
            "users.info: {users} calls for {} users",
            expected.users
        ));
    }
    JournalCheck { calls, problems }
}

/// Checks the Drive tenant's journal after the first sync: reads only. The exact calls come
/// with the Drive adapter (E4.3).
pub fn check_drive(journal: &[JournalEntry]) -> JournalCheck {
    let mut problems = Vec::new();
    reads_only(journal, &mut problems);
    if journal.is_empty() {
        problems.push("no Drive call".into());
    }
    JournalCheck {
        calls: counts(journal),
        problems,
    }
}

/// What a first sync of `channels` must call, from a Slack inject body and the adapter's
/// page size.
pub fn slack_expected(slack: &serde_json::Value, channels: &[&str], page: u32) -> SlackExpected {
    fn in_channel<'a>(
        messages: &'a [serde_json::Value],
        channel: &'a str,
    ) -> impl Iterator<Item = &'a serde_json::Value> {
        messages.iter().filter(move |m| m["channel"] == channel)
    }
    let messages = slack["messages"].as_array().map_or(&[][..], Vec::as_slice);
    let in_channel = |c| in_channel(messages, c);
    let page = usize::try_from(page).unwrap_or(usize::MAX).max(1);
    SlackExpected {
        // A channel's last page says there is no more; an empty channel still takes one call.
        history_pages: channels
            .iter()
            .map(|c| in_channel(c).count().div_ceil(page).max(1))
            .sum(),
        threads: channels
            .iter()
            .map(|c| {
                in_channel(c)
                    .filter(|m| m["reply"].as_array().is_some_and(|r| !r.is_empty()))
                    .count()
            })
            .sum(),
        users: slack["users"].as_array().map_or(0, Vec::len),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(method: &str, route: &str, status: u16) -> JournalEntry {
        JournalEntry {
            method: method.into(),
            route: route.into(),
            status,
            latency_ms: 1.0,
        }
    }

    fn first_sync() -> Vec<JournalEntry> {
        let mut journal = vec![
            entry("GET", "/api/conversations.history", 200),
            entry("GET", "/api/conversations.history", 200),
            entry("GET", "/api/users.info", 200),
        ];
        journal.extend((0..3).map(|_| entry("GET", "/api/conversations.replies", 200)));
        journal
    }

    const EXPECTED: SlackExpected = SlackExpected {
        history_pages: 2,
        threads: 3,
        users: 5,
    };

    #[test]
    fn a_first_sync_of_reads_passes() {
        let check = check_slack(&first_sync(), EXPECTED);
        assert!(check.passed(), "{:?}", check.problems);
        assert_eq!(check.calls["/api/conversations.replies"], 3);
    }

    #[test]
    fn a_write_fails_whatever_its_answer() {
        let mut journal = first_sync();
        journal.push(entry("POST", "/api/chat.postMessage", 403));
        let check = check_slack(&journal, EXPECTED);
        assert_eq!(
            check.problems,
            [
                "POST /api/chat.postMessage: not a read",
                "POST /api/chat.postMessage: status 403",
                "/api/chat.postMessage: not a method the sync reads",
            ]
        );
    }

    #[test]
    fn a_missing_thread_or_channel_fails() {
        let journal = first_sync();
        let check = check_slack(&journal[1..journal.len() - 1], EXPECTED);
        assert_eq!(
            check.problems,
            [
                "conversations.history: 1 calls for 2 pages",
                "conversations.replies: 2 calls for 3 threads",
            ]
        );
    }

    #[test]
    fn drive_needs_reads_and_at_least_one_call() {
        assert!(check_drive(&[entry("GET", "/drive/v3/files", 200)]).passed());
        assert!(!check_drive(&[]).passed());
        assert!(!check_drive(&[entry("PATCH", "/drive/v3/files/D_SPRAY", 403)]).passed());
    }

    #[test]
    fn a_users_info_per_user_at_most() {
        let mut journal = first_sync();
        journal.extend((0..5).map(|_| entry("GET", "/api/users.info", 200)));
        let check = check_slack(&journal, EXPECTED);
        assert_eq!(check.problems, ["users.info: 6 calls for 5 users"]);
    }

    #[test]
    fn the_fixture_takes_7_pages_and_20_threads() {
        let slack: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/era/maple-row/slack.json"
        ))
        .unwrap();
        // 55 messages in #field-reports and 39 in #crew, 15 a page (Tier 1's page size).
        assert_eq!(
            slack_expected(&slack, &["C_FIELD", "C_CREW"], 15),
            SlackExpected {
                history_pages: 7,
                threads: 20,
                users: 5,
            }
        );
    }
}
