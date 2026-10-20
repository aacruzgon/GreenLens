//! Where one source's sync stands, stored as its [`SyncCursor`] in `connector.cursor`. The
//! adapter's own cursor is opaque here and kept inside, next to what the sync tracks itself: the
//! last finished re-read, the re-read pass in progress with the vendor ids it has seen, and the
//! round in which it last finished a pass. The rotation over the connector's sources
//! ([`Rotation`]) is stored beside them, under [`SYNC_STATE_KEY`](domain::ports::SYNC_STATE_KEY).

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
    /// The connector's round in which this source last finished a pass; 0 before its first.
    /// It has finished its pass in the round in progress ([`Rotation::round`]) when this is
    /// that round.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub round: u32,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
const fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The rotation over a connector's sources, stored under [`SYNC_STATE_KEY`](domain::ports::SYNC_STATE_KEY) in its cursors.
/// Each run reads, in order from [`next`](Self::next), the sources that haven't finished their
/// pass in the round in progress, and moves `next` past the last source it asked for a page.
/// The round ends when every chosen source has finished its pass in it.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rotation {
    /// The round in progress, from 1. A source finishing its pass takes this round.
    #[serde(default)]
    pub round: u32,
    /// The source the next run starts from; the first source when unset or no longer chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

impl Rotation {
    /// The stored rotation. Without one (a connector not synced yet, or synced before the
    /// rotation was stored), the round in progress is the one after the lowest round its
    /// sources finished, so the sources behind the others are read first, from the first.
    pub fn parse(cursor: Option<&SyncCursor>, lowest: u32) -> Self {
        cursor
            .and_then(|c| serde_json::from_str::<Self>(&c.0).ok())
            .filter(|r| r.round > 0)
            .unwrap_or(Self {
                round: lowest.saturating_add(1),
                next: None,
            })
    }

    /// The rotation to store.
    pub fn to_cursor(&self) -> SyncCursor {
        SyncCursor(serde_json::to_string(self).expect("a number and a string serialise"))
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
            round: 0,
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
        let later = SourceState {
            round: 3,
            ..SourceState::default()
        };
        assert_eq!(later.to_cursor().0, r#"{"round":3}"#);
        assert_eq!(SourceState::parse(Some(&later.to_cursor())), later);
    }

    #[test]
    fn a_rotation_round_trips_and_an_old_connector_starts_from_its_lowest_round() {
        let rotation = Rotation {
            round: 4,
            next: Some("C2".into()),
        };
        let cursor = rotation.to_cursor();
        assert_eq!(cursor.0, r#"{"round":4,"next":"C2"}"#);
        assert_eq!(Rotation::parse(Some(&cursor), 0), rotation);
        // Stored before the rotation: the sources at the lowest round are the ones left.
        let fresh = Rotation {
            round: 3,
            next: None,
        };
        assert_eq!(Rotation::parse(None, 2), fresh);
        assert_eq!(Rotation::parse(Some(&SyncCursor("{}".into())), 2), fresh);
        assert_eq!(Rotation::parse(Some(&SyncCursor("junk".into())), 2), fresh);
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
