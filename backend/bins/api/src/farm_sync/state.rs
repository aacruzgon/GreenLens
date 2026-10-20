//! Where one source's sync stands, stored as its [`SyncCursor`] in `connector.cursor`. The
//! adapter's own cursor is opaque here and kept inside, next to what the sync tracks itself: the
//! last finished re-read and the re-read pass in progress, with the vendor ids it has seen.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use domain::{ports::SyncCursor, wire::Timestamp};

/// One source's state.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceState {
    /// The adapter's cursor after the last page it returned; `None` before the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// When a pass covering the re-read window last finished: the first sync, then each daily
    /// re-read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reread_at: Option<Timestamp>,
    /// The last page said another follows: a pass is in progress.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub more: bool,
    /// The re-read pass in progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reread: Option<Reread>,
}

/// A re-read pass in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reread {
    /// Where it started reading: 7 days before it began.
    pub from: Timestamp,
    /// The vendor ids its pages returned, sent or kept. At its end, a record in the window
    /// that isn't here is gone at the vendor.
    #[serde(default)]
    pub seen: BTreeSet<String>,
}

impl SourceState {
    /// The stored state; a cursor this module didn't write (none yet, or an older shape)
    /// starts afresh, which reads again but loses no record.
    pub fn parse(cursor: Option<&SyncCursor>) -> Self {
        cursor
            .and_then(|c| serde_json::from_str(&c.0).ok())
            .unwrap_or_default()
    }

    /// The state to store.
    pub fn to_cursor(&self) -> SyncCursor {
        SyncCursor(serde_json::to_string(self).expect("strings and timestamps serialise"))
    }

    /// The vendor's cursor for the next page.
    pub fn vendor_cursor(&self) -> Option<SyncCursor> {
        self.vendor.clone().map(SyncCursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_round_trips_and_stays_small() {
        let t: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        let state = SourceState {
            vendor: Some(r#"{"newest":"1.0"}"#.into()),
            reread_at: Some(t),
            more: false,
            reread: None,
        };
        let cursor = state.to_cursor();
        assert_eq!(
            cursor.0,
            r#"{"vendor":"{\"newest\":\"1.0\"}","reread_at":"2026-10-15T12:00:00Z"}"#
        );
        assert_eq!(SourceState::parse(Some(&cursor)), state);
        let reread = SourceState {
            more: true,
            reread: Some(Reread {
                from: t,
                seen: ["C1/1.0".to_owned()].into(),
            }),
            ..state
        };
        assert_eq!(SourceState::parse(Some(&reread.to_cursor())), reread);
    }

    #[test]
    fn an_unknown_cursor_starts_afresh() {
        assert_eq!(
            SourceState::parse(Some(&SyncCursor("raw-page-token".into()))),
            SourceState::default()
        );
        assert_eq!(SourceState::parse(None), SourceState::default());
    }
}
