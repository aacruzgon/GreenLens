// The scan's Live Activity: what the app sends and the widget extension draws (roadmap F5 item 6).
//
// ActivityKit matches an activity to its widget by this type's name, so the app's module and the extension each
// compile their own copy: modules/live-activity/ios/ScanActivityAttributes.swift and
// targets/scan-activity/ScanActivityAttributes.swift. The two files must stay identical; a Jest test
// (modules/live-activity/__tests__/attributes.test.ts) fails when they drift.
import ActivityKit
import Foundation

struct ScanActivityAttributes: ActivityAttributes {
  struct ContentState: Codable, Hashable {
    /// "running", "question", "planned" or "failed".
    var mode: String
    /// The stage in words: "Checking telltale signs", "Waiting for your answer", "Plan ready".
    var stage: String
    /// "Next: weighing the evidence"; nil when nothing follows.
    var next: String?
    /// Nine entries, one per step of the stage list: "waiting", "running", "done" or "skipped".
    var segments: [String]
    /// When the run started; the elapsed time ticks from it without an update.
    var startedAt: Date
    /// The run's total once it ends ("took 18 s"); replaces the ticking time.
    var total: String?
  }

  /// The scan, for the link back into the app.
  var scanId: String
  /// The plant's name, "Tomato 2".
  var plantName: String
}
