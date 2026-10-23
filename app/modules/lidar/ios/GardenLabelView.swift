import UIKit

// A plant's label over the 3D garden (docs/design/garden-3d.md 1, "Labels, selection and accessibility on Metal"):
// a solid chip with the status shape and the name (and the height in Growth). The selected one is outlined, not
// filled: nothing tinted sits on the 3D view (design 2). It is a UIKit view, so it gets SF Pro and Dynamic Type.

final class GardenLabelView: UIView {
  private let shape = CAShapeLayer()
  private let text = UILabel()
  private var status: GardenStatus = .unknown
  private var palette = GardenPalette.light
  private(set) var selected = false

  private static let shapeSide: CGFloat = 9
  private static let padH: CGFloat = 8
  private static let gap: CGFloat = 5

  override init(frame: CGRect) {
    super.init(frame: frame)
    isUserInteractionEnabled = false
    layer.masksToBounds = false
    layer.addSublayer(shape)
    text.font = UIFont.preferredFont(forTextStyle: .caption1)
    text.adjustsFontForContentSizeCategory = true
    text.numberOfLines = 1
    addSubview(text)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func configure(text value: String, status: GardenStatus, selected: Bool, palette: GardenPalette) {
    self.status = status
    self.selected = selected
    self.palette = palette
    text.text = value
    text.textColor = palette.text
    backgroundColor = palette.surface
    layer.borderColor = (selected ? palette.text : palette.border).cgColor
    layer.borderWidth = selected ? 2 : 1
    accessibilityLabel = value
    drawShape()
    sizeToFitContent()
  }

  /// Called by GardenView when Dynamic Type changes (the label's font follows on its own).
  func sizeToFitContent() {
    let textSize = text.sizeThatFits(CGSize(width: 240, height: 100))
    let height = max(22, ceil(textSize.height) + 6)
    let width = Self.padH + Self.shapeSide + Self.gap + ceil(textSize.width) + Self.padH
    let centre = center
    bounds = CGRect(x: 0, y: 0, width: width, height: height)
    center = centre
    layer.cornerRadius = height / 2
    shape.frame = CGRect(x: Self.padH, y: (height - Self.shapeSide) / 2, width: Self.shapeSide, height: Self.shapeSide)
    text.frame = CGRect(x: Self.padH + Self.shapeSide + Self.gap, y: (height - ceil(textSize.height)) / 2, width: ceil(textSize.width), height: ceil(textSize.height))
  }

  /// healthy ● circle, watch ▲ triangle, sick ◆ diamond, dead □ hollow square (design 6). Unknown is a hollow circle.
  private func drawShape() {
    let s = Self.shapeSide
    let colour = palette.status(status).cgColor
    let path = UIBezierPath()
    shape.lineWidth = 0
    shape.fillColor = colour
    shape.strokeColor = nil
    switch status {
    case .healthy:
      path.append(UIBezierPath(ovalIn: CGRect(x: 0.4, y: 0.4, width: s - 0.8, height: s - 0.8)))
    case .watch:
      path.move(to: CGPoint(x: s / 2, y: 0.3))
      path.addLine(to: CGPoint(x: s - 0.1, y: s - 0.6))
      path.addLine(to: CGPoint(x: 0.1, y: s - 0.6))
      path.close()
    case .sick:
      path.move(to: CGPoint(x: s / 2, y: 0))
      path.addLine(to: CGPoint(x: s, y: s / 2))
      path.addLine(to: CGPoint(x: s / 2, y: s))
      path.addLine(to: CGPoint(x: 0, y: s / 2))
      path.close()
    case .dead:
      path.append(UIBezierPath(roundedRect: CGRect(x: 1.4, y: 1.4, width: s - 2.8, height: s - 2.8), cornerRadius: 1))
      shape.fillColor = nil
      shape.strokeColor = colour
      shape.lineWidth = 1.6
    case .unknown:
      path.append(UIBezierPath(ovalIn: CGRect(x: 1, y: 1, width: s - 2, height: s - 2)))
      shape.fillColor = nil
      shape.strokeColor = colour
      shape.lineWidth = 1.4
    }
    shape.path = path.cgPath
  }
}
