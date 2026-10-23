import ExpoModulesCore
import UIKit
import simd

// GardenView's props as JavaScript sends them (docs/design/garden-3d.md 1, "Hosting it in React Native"), and the
// plain values the scene works from. JavaScript computes each plant's values for the chosen date; the view tweens
// to them and never rebuilds geometry for a date change.

struct GardenBedRecord: Record {
  @Field var id: String = ""
  @Field var name: String?
  /// Ground plane, metres: `[[x, y], ...]`, as in `GardenMap.beds[].polygon_m`.
  @Field var polygon_m: [[Double]] = []
}

struct GardenPlantRecord: Record {
  @Field var id: String = ""
  @Field var name: String = ""
  @Field var x_m: Double = 0
  @Field var y_m: Double = 0
  @Field var canopy_radius_m: Double?
  @Field var height_m: Double?
  /// healthy, watch, sick, dead or unknown.
  @Field var status: String = "unknown"
  @Field var tender: Bool = false
  /// The label's text; without it the label is the name, plus the height in Growth.
  @Field var label: String?
  /// The bed it stands in; a plant with no bed (or an unknown one) stands in a pot.
  @Field var bed_id: String?
}

enum GardenStatus: String {
  case healthy, watch, sick, dead, unknown

  var word: String {
    switch self {
    case .healthy: return "healthy"
    case .watch: return "watch"
    case .sick: return "sick"
    case .dead: return "dead"
    case .unknown: return "status unknown"
    }
  }
}

enum GardenMode: String {
  case health, growth, frost
}

struct GardenBed: Equatable {
  let id: String
  let name: String?
  /// (x, y) on the map's ground plane, metres.
  let polygon: [SIMD2<Float>]

  init(_ record: GardenBedRecord) {
    id = record.id
    name = record.name
    polygon = record.polygon_m.compactMap { p in
      p.count >= 2 ? SIMD2(Float(p[0]), Float(p[1])) : nil
    }
  }
}

struct GardenPlant: Equatable {
  let id: String
  let name: String
  let x: Float
  let y: Float
  let canopyRadius: Float?
  let height: Float?
  let status: GardenStatus
  let tender: Bool
  let label: String?
  let bedId: String?

  init(_ record: GardenPlantRecord) {
    id = record.id
    name = record.name
    x = Float(record.x_m)
    y = Float(record.y_m)
    canopyRadius = record.canopy_radius_m.map(Float.init)
    height = record.height_m.map(Float.init)
    status = GardenStatus(rawValue: record.status) ?? .unknown
    tender = record.tender
    label = record.label
    bedId = record.bed_id
  }

  /// What decides the geometry: a change here rebuilds the plant; anything else only tweens.
  var placementKey: String { "\(id)|\(x)|\(y)|\(bedId ?? "-")" }

  /// Shown height and canopy radius, with defaults where nothing was measured yet.
  var shownHeight: Float { max(0.05, height ?? 0.3) }
  var shownRadius: Float { max(0.06, canopyRadius ?? 0.15) }

  /// "Tomato 1, sick, 62 cm tall" (garden-3d 1, accessibility).
  var spokenText: String {
    var parts = [name, status.word]
    if let height { parts.append("\(Int((height * 100).rounded())) cm tall") }
    if tender { parts.append("tender") }
    return parts.joined(separator: ", ")
  }
}

/// The showroom's colours (design.md, the 3D garden: the showroom), per scheme. Status colours match
/// app/src/ui/palette.json.
struct GardenPalette {
  let background: UIColor
  let plinth: UIColor
  let plaster: UIColor
  let soil: UIColor
  let pot: UIColor
  let cover: UIColor
  let info: UIColor
  let text: UIColor
  let textMuted: UIColor
  let surface: UIColor
  let border: UIColor
  let healthy: UIColor
  let watch: UIColor
  let sick: UIColor
  let dead: UIColor
  /// Species-ish greens, picked by the plant's first word.
  let greens: [UIColor]

  func status(_ s: GardenStatus) -> UIColor {
    switch s {
    case .healthy: return healthy
    case .watch: return watch
    case .sick: return sick
    case .dead: return dead
    case .unknown: return textMuted
    }
  }

  func green(for name: String) -> UIColor {
    let key = name.lowercased().split(separator: " ").first.map(String.init) ?? name.lowercased()
    switch key {
    case "tomato": return greens[0]
    case "pepper": return greens[1]
    case "basil": return greens[2]
    case "lettuce": return greens[3]
    case "kale": return greens[4]
    default:
      let sum = key.unicodeScalars.reduce(0) { $0 &+ Int($1.value) }
      return greens[sum % greens.count]
    }
  }

  static func of(_ scheme: String) -> GardenPalette {
    scheme == "dark" ? dark : light
  }

  static let light = GardenPalette(
    background: UIColor(hex: 0xF6F7F2),
    plinth: UIColor(hex: 0xDCE6D2),
    plaster: UIColor(hex: 0xEFEBE3),
    soil: UIColor(hex: 0x8F5638),
    pot: UIColor(hex: 0xB8643F),
    cover: UIColor(hex: 0xF4F7FA),
    info: UIColor(hex: 0x285D86),
    text: UIColor(hex: 0x18211C),
    textMuted: UIColor(hex: 0x56625A),
    surface: UIColor(hex: 0xFFFFFF),
    border: UIColor(hex: 0xD9DFD2),
    healthy: UIColor(hex: 0x26704A),
    watch: UIColor(hex: 0x8A5300),
    sick: UIColor(hex: 0xB3261E),
    dead: UIColor(hex: 0x5C6360),
    greens: [0x3F7D3A, 0x4E8A2E, 0x2F7A45, 0x7DAE4A, 0x3B6650, 0x5A8F3C].map { UIColor(hex: $0) }
  )

  static let dark = GardenPalette(
    background: UIColor(hex: 0x0F1412),
    plinth: UIColor(hex: 0x28332C),
    plaster: UIColor(hex: 0xB9B4AA),
    soil: UIColor(hex: 0x6E3F2A),
    pot: UIColor(hex: 0x8E4A2E),
    cover: UIColor(hex: 0xDDE6EE),
    info: UIColor(hex: 0x8EC1EA),
    text: UIColor(hex: 0xE7EDE9),
    textMuted: UIColor(hex: 0xA2AEA6),
    surface: UIColor(hex: 0x18201C),
    border: UIColor(hex: 0x2F3A34),
    healthy: UIColor(hex: 0x79D19D),
    watch: UIColor(hex: 0xF2BE5C),
    sick: UIColor(hex: 0xF2948B),
    dead: UIColor(hex: 0xA7AEAA),
    greens: [0x3A7335, 0x47802A, 0x2B7040, 0x6E9E42, 0x355E49, 0x528436].map { UIColor(hex: $0) }
  )
}

extension UIColor {
  convenience init(hex: UInt32) {
    self.init(
      red: CGFloat((hex >> 16) & 0xFF) / 255,
      green: CGFloat((hex >> 8) & 0xFF) / 255,
      blue: CGFloat(hex & 0xFF) / 255,
      alpha: 1
    )
  }

  var rgba: SIMD4<Float> {
    var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
    getRed(&r, green: &g, blue: &b, alpha: &a)
    return SIMD4(Float(r), Float(g), Float(b), Float(a))
  }

  convenience init(rgba v: SIMD4<Float>) {
    self.init(red: CGFloat(v.x), green: CGFloat(v.y), blue: CGFloat(v.z), alpha: CGFloat(v.w))
  }
}
