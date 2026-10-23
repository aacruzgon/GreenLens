// The app's side of the scan's Live Activity (roadmap F5 item 6): start, update and end it from the scan's own
// stream while the app runs. There is no push token: the app is signed with a free Apple ID, which can't use
// APNs, so every update is local.
//
// A Live Activity is shown while the app is in the background, which is when the scan screen closes its stream
// (docs/frontend/README.md 10). While an activity is running, the module holds a background task when the app
// leaves the foreground, so the stream can keep the activity current for the ~30 s iOS allows; the JavaScript
// side closes the stream before that time runs out. Every content carries a stale date, 30 s after the app left
// the foreground, so a scan that finished meanwhile never shows as running with a ticking clock.
import ActivityKit
import ExpoModulesCore
import UIKit
import os

struct ScanAttributesRecord: Record {
  @Field var scanId: String = ""
  @Field var plantName: String = ""
}

struct ScanStateRecord: Record {
  @Field var mode: String = "running"
  @Field var stage: String = ""
  @Field var next: String? = nil
  @Field var segments: [String] = []
  /// Milliseconds since 1970, as JavaScript's Date.now().
  @Field var startedAt: Double = 0
  @Field var total: String? = nil

  var content: ScanActivityAttributes.ContentState {
    ScanActivityAttributes.ContentState(
      mode: mode,
      stage: stage,
      next: next,
      segments: segments,
      startedAt: Date(timeIntervalSince1970: startedAt / 1000),
      total: total
    )
  }
}

struct AlertRecord: Record {
  @Field var title: String = ""
  @Field var body: String = ""
}

struct DismissalRecord: Record {
  /// "default", "immediate" or "after".
  @Field var policy: String = "default"
  /// With "after": seconds from now.
  @Field var afterSeconds: Double? = nil

  var value: ActivityUIDismissalPolicy {
    switch policy {
    case "immediate": return .immediate
    case "after": return .after(Date().addingTimeInterval(afterSeconds ?? 0))
    default: return .default
    }
  }
}

enum LiveActivityError: Error, CustomStringConvertible {
  case notFound(String)
  var description: String {
    switch self {
    case .notFound(let id): return "No Live Activity with id \(id)."
    }
  }
}

public class LiveActivityModule: Module {
  private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
  /// When the app left the foreground; nil while it is in front. Set on the main thread, read on the module's queue.
  private let backgroundSince = OSAllocatedUnfairLock<Date?>(initialState: nil)

  /// After the app leaves the foreground, the stream keeps the activity current for the background grace (25 s in
  /// useScanEvents) and then stops, so the content goes stale a little after that and the widget says to open
  /// the app instead of showing a run that may have ended (HIG, Live Activities).
  private static let staleAfterBackground: TimeInterval = 30
  /// In front, the stream updates on every stage. This only covers an app that died without ending the activity.
  private static let staleInForeground: TimeInterval = 15 * 60

  private func staleDate() -> Date {
    if let since = backgroundSince.withLock({ $0 }) { return since.addingTimeInterval(Self.staleAfterBackground) }
    return Date().addingTimeInterval(Self.staleInForeground)
  }

  /// The last content the app sent per activity, with a count that moves on every send, so a restamp never puts
  /// back an older stage than one the stream sent meanwhile.
  private let sent = OSAllocatedUnfairLock<[String: (state: ScanActivityAttributes.ContentState, generation: Int)]>(initialState: [:])

  private func record(_ id: String, _ state: ScanActivityAttributes.ContentState) {
    sent.withLock { $0[id] = (state, ($0[id]?.generation ?? 0) + 1) }
  }

  /// Every running activity's last content again, with a new stale date. If the app sent newer content while the
  /// restamp was on its way, that content goes again after it, so the newest is what stays.
  private func restamp() {
    Task {
      for activity in Activity<ScanActivityAttributes>.activities where activity.activityState == .active {
        guard let before = sent.withLock({ $0[activity.id] }) else { continue }
        await activity.update(ActivityContent(state: before.state, staleDate: staleDate()))
        if let after = sent.withLock({ $0[activity.id] }), after.generation != before.generation {
          await activity.update(ActivityContent(state: after.state, staleDate: staleDate()))
        }
      }
    }
  }

  public func definition() -> ModuleDefinition {
    Name("LiveActivity")

    OnCreate {
      let center = NotificationCenter.default
      center.addObserver(
        self, selector: #selector(self.didEnterBackground),
        name: UIApplication.didEnterBackgroundNotification, object: nil)
      center.addObserver(
        self, selector: #selector(self.willEnterForeground),
        name: UIApplication.willEnterForegroundNotification, object: nil)
    }

    OnDestroy {
      NotificationCenter.default.removeObserver(self)
      self.endBackgroundTask()
    }

    /// Whether this device and the person's settings allow Live Activities now.
    Function("isSupported") { () -> Bool in
      ActivityAuthorizationInfo().areActivitiesEnabled
    }

    /// Starts the scan's activity and returns its id. One scan at a time: any other scan activity is ended first
    /// (one left over from a run the app didn't finish, such as after a crash).
    AsyncFunction("start") { (attrs: ScanAttributesRecord, state: ScanStateRecord) -> String in
      for old in Activity<ScanActivityAttributes>.activities {
        await old.end(nil, dismissalPolicy: .immediate)
      }
      let attributes = ScanActivityAttributes(scanId: attrs.scanId, plantName: attrs.plantName)
      let activity = try Activity.request(
        attributes: attributes,
        content: ActivityContent(state: state.content, staleDate: self.staleDate()),
        pushType: nil
      )
      self.record(activity.id, state.content)
      return activity.id
    }

    /// New content; with an alert, the Lock Screen lights up and the Dynamic Island expands.
    AsyncFunction("update") { (id: String, state: ScanStateRecord, alert: AlertRecord?) in
      guard let activity = Self.find(id) else { throw LiveActivityError.notFound(id) }
      self.record(id, state.content)
      let content = ActivityContent(state: state.content, staleDate: self.staleDate())
      if let alert {
        let config = AlertConfiguration(
          title: LocalizedStringResource(stringLiteral: alert.title),
          body: LocalizedStringResource(stringLiteral: alert.body),
          sound: .default
        )
        await activity.update(content, alertConfiguration: config)
      } else {
        await activity.update(content)
      }
    }

    /// Ends it with its final content, which stays on the Lock Screen until the dismissal policy removes it.
    AsyncFunction("end") { (id: String, state: ScanStateRecord, dismissal: DismissalRecord) in
      self.sent.withLock { _ = $0.removeValue(forKey: id) }
      if let activity = Self.find(id) {
        await activity.end(ActivityContent(state: state.content, staleDate: nil), dismissalPolicy: dismissal.value)
      }
      if !Self.anyRunning() {
        await MainActor.run { self.endBackgroundTask() }
      }
    }
  }

  private static func find(_ id: String) -> Activity<ScanActivityAttributes>? {
    Activity<ScanActivityAttributes>.activities.first { $0.id == id }
  }

  private static func anyRunning() -> Bool {
    Activity<ScanActivityAttributes>.activities.contains { $0.activityState == .active }
  }

  @objc private func didEnterBackground() {
    backgroundSince.withLock { $0 = Date() }
    guard Self.anyRunning() else { return }
    restamp()
    guard backgroundTask == .invalid else { return }
    backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "pdm.scan-activity") { [weak self] in
      self?.endBackgroundTask()
    }
  }

  @objc private func willEnterForeground() {
    backgroundSince.withLock { $0 = nil }
    endBackgroundTask()
    // The stream resumes and sends the current stage; until then the content isn't stale any more.
    if Self.anyRunning() { restamp() }
  }

  private func endBackgroundTask() {
    guard backgroundTask != .invalid else { return }
    UIApplication.shared.endBackgroundTask(backgroundTask)
    backgroundTask = .invalid
  }
}
