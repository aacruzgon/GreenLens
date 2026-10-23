// The scan on the Lock Screen and in the Dynamic Island (roadmap F5 item 6; design 5, "Live Activity"): the
// plant, the stage in words, the elapsed time, a nine-segment bar and what comes next. The app updates it from
// its own scan stream (modules/live-activity); the elapsed time ticks on its own (`Text(timerInterval:)`), so the
// app sends an update only when a stage changes. Tapping it opens the scan (pdm://scans/<id>).
import ActivityKit
import SwiftUI
import WidgetKit

struct ScanActivityWidget: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: ScanActivityAttributes.self) { context in
      LockScreenView(attributes: context.attributes, state: context.state)
        .activityBackgroundTint(Color("pdmBackground"))
        .activitySystemActionForegroundColor(Color("pdmText"))
        .widgetURL(scanURL(context.attributes))
    } dynamicIsland: { context in
      let state = context.state
      return DynamicIsland {
        DynamicIslandExpandedRegion(.leading) {
          Label {
            Text(context.attributes.plantName).lineLimit(1)
          } icon: {
            Image(systemName: "leaf.fill").foregroundStyle(Color("islandAccent"))
          }
          .font(.headline)
          .padding(.leading, 4)
        }
        DynamicIslandExpandedRegion(.trailing) {
          ElapsedText(state: state)
            .font(.headline.monospacedDigit())
            .foregroundStyle(Color("islandMuted"))
            .padding(.trailing, 4)
        }
        DynamicIslandExpandedRegion(.bottom) {
          VStack(alignment: .leading, spacing: 8) {
            Text(state.stage)
              .font(.title3.weight(.semibold))
              .foregroundStyle(stageColor(state, island: true))
              .lineLimit(1)
            SegmentBar(segments: state.segments, accent: Color("islandAccent"), track: Color("islandTrack"))
            if let next = state.next {
              Text(next).font(.subheadline.weight(.medium)).foregroundStyle(Color("islandMuted")).lineLimit(1)
            }
          }
          .padding(.horizontal, 4)
        }
      } compactLeading: {
        ProgressRing(state: state)
      } compactTrailing: {
        CompactTrailing(state: state)
      } minimal: {
        ProgressRing(state: state)
      }
      .widgetURL(scanURL(context.attributes))
      .keylineTint(Color("islandAccent"))
    }
  }
}

private func scanURL(_ attributes: ScanActivityAttributes) -> URL? {
  URL(string: "pdm://scans/\(attributes.scanId)")
}

private func stageColor(_ state: ScanActivityAttributes.ContentState, island: Bool) -> Color {
  switch state.mode {
  case "question": return Color(island ? "islandWatch" : "pdmWatch")
  case "failed": return Color(island ? "islandDanger" : "pdmDanger")
  default: return island ? .white : Color("pdmText")
  }
}

/// The time since the run started, ticking with no update; the run's total once it ends; nothing once stopped.
struct ElapsedText: View {
  let state: ScanActivityAttributes.ContentState

  var body: some View {
    if let total = state.total {
      Text(total)
    } else if state.mode == "failed" {
      EmptyView()
    } else {
      Text(timerInterval: state.startedAt...state.startedAt.addingTimeInterval(8 * 60 * 60), countsDown: false)
        .multilineTextAlignment(.trailing)
    }
  }
}

/// Nine segments, one per step of the stage list: done in the accent, the running one half-filled, a skipped
/// step faint, the rest as track.
struct SegmentBar: View {
  let segments: [String]
  let accent: Color
  let track: Color

  var body: some View {
    HStack(spacing: 3) {
      ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
        Capsule()
          .fill(fill(segment))
          .frame(height: 6)
      }
    }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel("\(segments.filter { $0 == "done" || $0 == "skipped" }.count) of \(segments.count) steps done")
  }

  private func fill(_ segment: String) -> Color {
    switch segment {
    case "done": return accent
    case "running": return accent.opacity(0.45)
    case "skipped": return track.opacity(0.4)
    default: return track
    }
  }
}

/// The compact and minimal presentations: a ring that fills a ninth per step, around a leaf.
struct ProgressRing: View {
  let state: ScanActivityAttributes.ContentState

  private var fraction: Double {
    let ticked = state.segments.filter { $0 == "done" || $0 == "skipped" }.count
    return state.segments.isEmpty ? 0 : Double(ticked) / Double(state.segments.count)
  }

  var body: some View {
    let tint = state.mode == "question" ? Color("islandWatch") : state.mode == "failed" ? Color("islandDanger") : Color("islandAccent")
    ZStack {
      Circle().stroke(Color("islandTrack"), lineWidth: 2.5)
      Circle()
        .trim(from: 0, to: fraction)
        .stroke(tint, style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
        .rotationEffect(.degrees(-90))
      Image(systemName: "leaf.fill")
        .font(.system(size: 9, weight: .semibold))
        .foregroundStyle(tint)
    }
    .frame(width: 22, height: 22)
    .accessibilityLabel("Scan, \(state.stage)")
  }
}

/// The compact trailing view: the elapsed time while it runs; a sign once it needs an answer, is ready or stopped.
struct CompactTrailing: View {
  let state: ScanActivityAttributes.ContentState

  var body: some View {
    switch state.mode {
    case "question":
      Image(systemName: "questionmark.circle.fill").foregroundStyle(Color("islandWatch"))
    case "planned":
      Image(systemName: "checkmark.circle.fill").foregroundStyle(Color("islandAccent"))
    case "failed":
      Image(systemName: "exclamationmark.circle.fill").foregroundStyle(Color("islandDanger"))
    default:
      ElapsedText(state: state)
        .font(.caption.weight(.semibold).monospacedDigit())
        .foregroundStyle(Color("islandAccent"))
        .frame(maxWidth: 40)
    }
  }
}

struct LockScreenView: View {
  let attributes: ScanActivityAttributes
  let state: ScanActivityAttributes.ContentState

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack(alignment: .firstTextBaseline) {
        Label {
          Text(attributes.plantName).lineLimit(1)
        } icon: {
          Image(systemName: "leaf.fill").foregroundStyle(Color("pdmAccent"))
        }
        .font(.headline)
        .foregroundStyle(Color("pdmText"))
        Spacer()
        ElapsedText(state: state)
          .font(.headline.monospacedDigit())
          .foregroundStyle(Color("pdmMuted"))
      }
      Text(state.stage)
        .font(.title3.weight(.semibold))
        .foregroundStyle(stageColor(state, island: false))
        .lineLimit(1)
      SegmentBar(segments: state.segments, accent: Color("pdmAccent"), track: Color("pdmTrack"))
      if let next = state.next {
        Text(next)
          .font(.subheadline.weight(.medium))
          .foregroundStyle(Color("pdmMuted"))
          .lineLimit(1)
      }
    }
    .padding(16)
  }
}

// Xcode's canvas: the three states the scan passes through. Widget previews need iOS 17; the extension runs
// from 16.4 like the app.
@available(iOS 17.0, *)
#Preview("Lock Screen", as: .content, using: ScanActivityAttributes(scanId: "scan_1", plantName: "Tomato 1")) {
  ScanActivityWidget()
} contentStates: {
  ScanActivityAttributes.ContentState(
    mode: "running", stage: "Checking telltale signs", next: "Next: weighing the evidence",
    segments: ["done", "done", "done", "skipped", "done", "done", "running", "waiting", "waiting"],
    startedAt: Date().addingTimeInterval(-9), total: nil)
  ScanActivityAttributes.ContentState(
    mode: "question", stage: "Waiting for your answer", next: "Next: weighing the evidence",
    segments: ["done", "done", "done", "skipped", "done", "done", "running", "waiting", "waiting"],
    startedAt: Date().addingTimeInterval(-12), total: nil)
  ScanActivityAttributes.ContentState(
    mode: "planned", stage: "Plan ready", next: nil,
    segments: ["done", "done", "done", "skipped", "done", "done", "done", "done", "done"],
    startedAt: Date().addingTimeInterval(-18), total: "in 18 s")
}
