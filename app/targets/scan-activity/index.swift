import SwiftUI
import WidgetKit

// The extension's entry point: the scan's Live Activity is its only widget.
@main
struct ScanActivityBundle: WidgetBundle {
  var body: some Widget {
    ScanActivityWidget()
  }
}
