//! When the scan event stream closes ([contracts 4], when the stream closes).

use crate::scan::ScanStatus;

/// What the stream needs to know about the scan after an event is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamState {
    /// The scan's status.
    pub status: ScanStatus,
    /// A question is `open` or `answering`.
    pub question_unresolved: bool,
    /// At least one guided check is `analysing`.
    pub guided_check_analysing: bool,
    /// A pipeline step holds the scan's claim (`pipeline_step` is set): more events are
    /// coming from it, so the scan isn't at rest even if its status is. This lets a late
    /// answer's `question_resolved` or a guided check's `guided_check_done` be sent before the
    /// `score` step's `diagnosis` without the stream closing in between.
    pub step_claimed: bool,
}

impl StreamState {
    /// A resting point: `planned` or `failed`, no question open or `answering`, and no guided
    /// check `analysing`. The server closes the stream after the event that reaches it, and a
    /// stream opened at rest replays and closes.
    pub const fn at_rest(self) -> bool {
        self.status.at_rest()
            && !self.question_unresolved
            && !self.guided_check_analysing
            && !self.step_claimed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(status: ScanStatus, q: bool, g: bool) -> StreamState {
        StreamState {
            status,
            question_unresolved: q,
            guided_check_analysing: g,
            step_claimed: false,
        }
    }

    #[test]
    fn resting_points() {
        assert!(state(ScanStatus::Planned, false, false).at_rest());
        assert!(!state(ScanStatus::Planned, true, false).at_rest());
        assert!(!state(ScanStatus::Planned, false, true).at_rest());
        assert!(state(ScanStatus::Failed, false, false).at_rest());
        assert!(!state(ScanStatus::Failed, false, true).at_rest());
        assert!(!state(ScanStatus::Diagnosed, false, false).at_rest());
        let mut claimed = state(ScanStatus::Planned, false, false);
        claimed.step_claimed = true;
        assert!(!claimed.at_rest());
    }
}
