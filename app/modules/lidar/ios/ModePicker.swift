import ExpoModulesCore
import UIKit

// Capture's mode bar as Apple's own control (design 3.1 and 6): iOS 26's segmented control, whose thumb is the
// system's Liquid Glass, on interactive glass like the Camera app's mode picker and home's tab bar. iOS draws the
// glass, the thumb and its motion, and honours Reduce Transparency and Reduce Motion; nothing here imitates them.
// Like the tab bar's selection, the thumb turns to the glass lens while touched and rests as a platter.
// Setting `selectedSegmentTintColor` would swap the glass thumb for a flat fill, so it stays nil. Wider than the
// screen, the modes scroll and the chosen one is kept centred.

public final class LidarModePickerModule: Module {
  public func definition() -> ModuleDefinition {
    Name("ModePicker")

    View(ModePickerView.self) {
      Events("onSelect")

      Prop("items") { (view: ModePickerView, items: [ModePickerItem]) in
        view.setItems(items)
      }
    }
  }
}

struct ModePickerItem: Record {
  @Field var kind: String = ""
  @Field var label: String = ""
  /// none, stored, busy or problem (features/capture model/draft ModeMark).
  @Field var mark: String = "none"
  @Field var selected: Bool = false
}

final class ModePickerView: ExpoView {
  let onSelect = EventDispatcher()

  private let glass = UIVisualEffectView()
  private let scroll = UIScrollView()
  private let control = UISegmentedControl()
  private var items: [ModePickerItem] = []
  private var titles: [String] = []
  private var centred = -1
  private static let inset: CGFloat = 4
  private static let font = UIFont.systemFont(ofSize: 15, weight: .medium)

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    // Capture is always dark (design 3.1).
    overrideUserInterfaceStyle = .dark
    backgroundColor = .clear
    scroll.showsHorizontalScrollIndicator = false
    scroll.alwaysBounceHorizontal = false
    control.apportionsSegmentWidthsByContent = true
    control.setTitleTextAttributes([.foregroundColor: UIColor.white, .font: Self.font, .kern: 0.6], for: .normal)
    // The Camera app's yellow for the chosen mode.
    control.setTitleTextAttributes([.foregroundColor: UIColor.systemYellow, .font: Self.font, .kern: 0.6], for: .selected)
    control.addAction(UIAction { [weak self] _ in self?.chose() }, for: .valueChanged)
    scroll.addSubview(control)
    glass.contentView.addSubview(scroll)
    addSubview(glass)
    applyMaterial()
    NotificationCenter.default.addObserver(
      self, selector: #selector(transparencyChanged),
      name: UIAccessibility.reduceTransparencyStatusDidChangeNotification, object: nil)
  }

  func setItems(_ next: [ModePickerItem]) {
    let nextTitles = next.map(Self.title)
    if nextTitles != titles {
      titles = nextTitles
      control.removeAllSegments()
      for (i, title) in titles.enumerated() {
        control.insertSegment(withTitle: title, at: i, animated: false)
      }
      centred = -1
      setNeedsLayout()
    }
    items = next
    let index = next.firstIndex(where: { $0.selected }) ?? UISegmentedControl.noSegment
    if control.selectedSegmentIndex != index { control.selectedSegmentIndex = index }
    centreChosen(animated: centred >= 0)
  }

  /// The label in capitals, after its mark: a dot once stored, a ring while uploading, ! for a retake.
  private static func title(_ item: ModePickerItem) -> String {
    let label = item.label.uppercased()
    switch item.mark {
    case "stored": return "● \(label)"
    case "busy": return "○ \(label)"
    case "problem": return "! \(label)"
    default: return label
    }
  }

  private func chose() {
    let index = control.selectedSegmentIndex
    guard items.indices.contains(index) else { return }
    onSelect(["kind": items[index].kind])
    centreChosen(animated: true)
  }

  // MARK: Glass

  /// Interactive glass on iOS 26; a solid dark capsule (camera.pill) under Reduce Transparency or before iOS 26.
  private func applyMaterial() {
    if #available(iOS 26.0, *), !UIAccessibility.isReduceTransparencyEnabled {
      let effect = UIGlassEffect(style: .regular)
      effect.isInteractive = true
      glass.effect = effect
      glass.backgroundColor = nil
      glass.layer.borderWidth = 0
      glass.cornerConfiguration = .capsule()
    } else {
      glass.effect = nil
      glass.backgroundColor = UIColor(red: 28 / 255, green: 28 / 255, blue: 30 / 255, alpha: 1)
      glass.layer.borderWidth = 1
      glass.layer.borderColor = UIColor.white.withAlphaComponent(0.1).cgColor
      glass.clipsToBounds = true
      setNeedsLayout()
    }
  }

  @objc private func transparencyChanged() {
    applyMaterial()
  }

  // MARK: Layout

  override func layoutSubviews() {
    super.layoutSubviews()
    let inset = Self.inset
    control.sizeToFit()
    let content = CGSize(width: control.bounds.width + inset * 2, height: bounds.height)
    control.frame = CGRect(x: inset, y: inset, width: control.bounds.width, height: bounds.height - inset * 2)
    let width = min(bounds.width, content.width)
    glass.frame = CGRect(x: (bounds.width - width) / 2, y: 0, width: width, height: bounds.height)
    if glass.effect == nil { glass.layer.cornerRadius = bounds.height / 2 }
    scroll.frame = glass.contentView.bounds
    scroll.contentSize = content
    if centred != control.selectedSegmentIndex { centreChosen(animated: false) }
  }

  /// Keeps the chosen mode in the middle of the bar, as the Camera app does (mockups app.js centreMode). The
  /// control sizes each segment to its title plus an equal margin, so a segment's centre follows from the titles.
  private func centreChosen(animated: Bool) {
    let index = control.selectedSegmentIndex
    guard titles.indices.contains(index), scroll.bounds.width > 0, control.bounds.width > 0 else { return }
    let attributes: [NSAttributedString.Key: Any] = [.font: Self.font, .kern: 0.6]
    let widths = titles.map { ($0 as NSString).size(withAttributes: attributes).width }
    let margin = max(0, control.bounds.width - widths.reduce(0, +)) / CGFloat(widths.count)
    let start = widths.prefix(index).reduce(0) { $0 + $1 + margin }
    let middle = Self.inset + start + (widths[index] + margin) / 2
    let maxOffset = max(0, scroll.contentSize.width - scroll.bounds.width)
    let x = min(max(0, middle - scroll.bounds.width / 2), maxOffset)
    scroll.setContentOffset(CGPoint(x: x, y: 0), animated: animated && !UIAccessibility.isReduceMotionEnabled)
    centred = index
  }
}
