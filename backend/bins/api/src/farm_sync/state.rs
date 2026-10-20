//! Where one source's sync stands, stored as its [`SyncCursor`] in `connector.cursor`. The
//! adapter's own cursor is opaque here and kept inside, next to what the sync tracks itself: the
//! last finished re-read, the re-read pass in progress with the vendor ids it has seen, and
//! whether its last turn ended in a vendor error, and the version of the embedded text its records
//! were written with. The rotation over the connector's sources
//! ([`Rotation`]) is stored beside them, under [`SYNC_STATE_KEY`](domain::ports::SYNC_STATE_KEY).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use domain::{ports::SyncCursor, wire::Timestamp};

use super::chunk::EMBEDDED_TEXT_VERSION;

/// One source's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceState {
    /// The [`EMBEDDED_TEXT_VERSION`] its records were embedded under; 0 in a state written
    /// before it existed.
    #[serde(default)]
    pub text_version: u32,
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
    /// Its last turn ended in a vendor error, and no page has come since: the connector isn't
    /// caught up, and keeps its `last_error`, until one does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failing: bool,
}

impl Default for SourceState {
    fn default() -> Self {
        Self {
            text_version: EMBEDDED_TEXT_VERSION,
            vendor: None,
            reread_at: None,
            more: false,
            reread: None,
            failing: false,
        }
    }
}

/// The rotation over a connector's sources, stored under
/// [`SYNC_STATE_KEY`](domain::ports::SYNC_STATE_KEY) in its cursors. A round gives every chosen
/// source a turn: the run asks it for at least one page, whether it then finishes its pass,
/// runs out of budget mid-pass or fails. Each run first gives a turn to the sources that
/// haven't had one in the round, in order from [`next`](Self::next); the round is over once
/// every chosen source has had its turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rotation {
    /// The round in progress, from 1.
    pub round: u32,
    /// The source the next run starts from; the first source when unset or no longer chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// The sources that have had their turn in the round in progress.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub turned: BTreeSet<String>,
}

impl Default for Rotation {
    fn default() -> Self {
        Self {
            round: 1,
            next: None,
            turned: BTreeSet::new(),
        }
    }
}

impl Rotation {
    /// The stored rotation; without one, or with one this module didn't write, a fresh round
    /// from the first source.
    pub fn parse(cursor: Option<&SyncCursor>) -> Self {
        cursor
            .and_then(|c| serde_json::from_str::<Self>(&c.0).ok())
            .filter(|r| r.round > 0)
            .unwrap_or_default()
    }

    /// The rotation to store.
    pub fn to_cursor(&self) -> SyncCursor {
        SyncCursor(serde_json::to_string(self).expect("numbers and strings serialise"))
    }
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
    /// starts afresh, which reads again but loses no record. So does a state from another
    /// [`EMBEDDED_TEXT_VERSION`]: the fresh pass returns every item, and each record's new hash
    /// embeds it again (EON gap G12).
    pub fn parse(cursor: Option<&SyncCursor>) -> Self {
        cursor
            .and_then(|c| serde_json::from_str::<Self>(&c.0).ok())
            .filter(|s| s.text_version == EMBEDDED_TEXT_VERSION)
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
            text_version: EMBEDDED_TEXT_VERSION,
            vendor: Some(r#"{"newest":"1.0"}"#.into()),
            reread_at: Some(t),
            more: false,
            reread: None,
            failing: false,
        };
        let cursor = state.to_cursor();
        assert_eq!(
            cursor.0,
            r#"{"text_version":1,"vendor":"{\"newest\":\"1.0\"}","reread_at":"2026-10-15T12:00:00Z"}"#
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
        let failing = SourceState {
            failing: true,
            ..SourceState::default()
        };
        assert_eq!(
            failing.to_cursor().0,
            r#"{"text_version":1,"failing":true}"#
        );
        assert_eq!(SourceState::parse(Some(&failing.to_cursor())), failing);
    }

    #[test]
    fn a_rotation_round_trips_and_anything_else_starts_a_fresh_round() {
        let rotation = Rotation {
            round: 4,
            next: Some("C2".into()),
            turned: ["C1".to_owned(), "C3".to_owned()].into(),
        };
        let cursor = rotation.to_cursor();
        assert_eq!(cursor.0, r#"{"round":4,"next":"C2","turned":["C1","C3"]}"#);
        assert_eq!(Rotation::parse(Some(&cursor)), rotation);
        assert_eq!(Rotation::default().to_cursor().0, r#"{"round":1}"#);
        // None stored, an unreadable one, or one from an earlier shape: a fresh round.
        let fresh = Rotation::default();
        assert_eq!(Rotation::parse(None), fresh);
        assert_eq!(Rotation::parse(Some(&SyncCursor("{}".into()))), fresh);
        assert_eq!(Rotation::parse(Some(&SyncCursor("junk".into()))), fresh);
        assert_eq!(
            Rotation::parse(Some(&SyncCursor(r#"{"round":0,"next":"C2"}"#.into()))),
            fresh
        );
    }

    #[test]
    fn an_unknown_cursor_starts_afresh() {
        assert_eq!(
            SourceState::parse(Some(&SyncCursor("raw-page-token".into()))),
            SourceState::default()
        );
        assert_eq!(SourceState::parse(None), SourceState::default());
    }

    #[test]
    fn a_state_from_another_embedded_text_starts_afresh() {
        // Written before the author and title joined the embedded text: no version.
        let before = SyncCursor(r#"{"vendor":"p9","reread_at":"2026-10-15T12:00:00Z"}"#.into());
        assert_eq!(SourceState::parse(Some(&before)), SourceState::default());
        assert_eq!(SourceState::default().vendor_cursor(), None);
        let current = SyncCursor(r#"{"text_version":1,"vendor":"p9"}"#.into());
        assert_eq!(
            SourceState::parse(Some(&current)).vendor_cursor(),
            Some(SyncCursor("p9".into()))
        );
    }
}
