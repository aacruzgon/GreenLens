import RealityKit
import UIKit
import simd

// One plant in the 3D garden: its entities (shared unit meshes, scaled), its label, and the tween that moves it to
// the values JavaScript sent for the chosen date. A date change only moves the targets; nothing is rebuilt.
//
// Instanced (iOS 26, garden-3d 1 and 3): the stem, crown, ring, pot and soil are placeholders with no model, and
// GardenInstances draws every plant's part of one mesh and material in a single draw. A ring changing colour has its
// own model for the length of the tween (instances share their material), then goes back to its batch.

@MainActor
final class PlantNode {
  static let namePrefix = "plant:"
  /// Smallest tap target, metres (garden-3d 1: at least 0.25 m, about 44 pt at the default zoom).
  static let minHitRadius: Float = 0.25

  let id: String
  let root = Entity()
  let label = GardenLabelView()
  private(set) var plant: GardenPlant
  private let body = Entity()
  private let stem: ModelEntity
  /// The canopy and top blobs as one mesh (GardenMeshes.crown).
  private let crown: ModelEntity
  private let ring: ModelEntity
  private let cover: ModelEntity
  private let pot: Entity?
  private let soil: Entity?
  private let inPot: Bool
  private let instanced: Bool
  private let materials: GardenMaterials
  /// The crown's material and its batch: the plant's green, or the dead canopy's.
  private var crownMaterial: Material
  private var crownKey: String
  /// False while the ring tweens its colour on its own model.
  private var ringInBatch = false
  /// A part joined or left a batch since GardenView last rebuilt the instances.
  var instancesChanged = true
  private let potRadius: Float
  /// Where the plant stands: the bed's soil, the pot's soil or the plinth.
  let baseY: Float

  // Animated values and their targets.
  private(set) var height: Float
  private(set) var radius: Float
  private var coverAmount: Float = 0
  private var ringScale: Float = 1
  private var ringColour: SIMD4<Float>
  private var targetHeight: Float = 0
  private var targetRadius: Float = 0
  private var targetCover: Float = 0
  private var targetRingScale: Float = 1
  private var targetRingColour: SIMD4<Float>
  private var ringMaterial: UnlitMaterial
  private var ringMaterialKey = "unknown"
  private var appliedRingKey = "unknown"
  private var collisionKey: SIMD2<Float> = .zero
  private var labelKey = ""
  private var accessibilityKey = ""
  private var canopyDead = false

  init(
    plant: GardenPlant, position: SIMD3<Float>, bedTop: Float?, materials: GardenMaterials, from previous: PlantNode?, growIn: Bool,
    instanced: Bool
  ) {
    id = plant.id
    self.plant = plant
    self.instanced = instanced
    self.materials = materials
    // The ring starts on the unknown status's shared material, so an instanced one starts in that batch.
    ringInBatch = instanced
    inPot = bedTop == nil
    root.name = Self.namePrefix + plant.id
    root.position = position

    let shared = GardenMeshes.self
    func part(_ mesh: MeshResource, _ material: Material) -> ModelEntity {
      instanced ? ModelEntity() : ModelEntity(mesh: mesh, materials: [material])
    }
    crownMaterial = materials.green(for: plant.name)
    crownKey = materials.greenKey(for: plant.name)
    stem = part(shared.stem, materials.stem)
    crown = part(shared.crown, crownMaterial)
    ring = part(shared.ring, materials.ring(for: .unknown))
    cover = ModelEntity(mesh: shared.dome, materials: [materials.cover])
    ringMaterial = materials.ring(for: .unknown)
    ringColour = materials.palette.status(.unknown).rgba
    targetRingColour = ringColour

    if let bedTop {
      potRadius = 0
      baseY = bedTop + 0.004
      pot = nil
      soil = nil
    } else {
      // A plant with no bed stands in a terracotta pot (garden-3d; design.md, the showroom).
      potRadius = max(plant.shownRadius * 0.85, 0.12)
      let potHeight: Float = 0.24
      let pot = part(shared.pot, materials.pot)
      pot.scale = SIMD3(potRadius, potHeight, potRadius)
      let soil = part(shared.disc, materials.soil)
      soil.position = SIMD3(0, potHeight * 0.94, 0)
      soil.scale = SIMD3(potRadius * 0.95, 1, potRadius * 0.95)
      root.addChild(pot)
      root.addChild(soil)
      if !instanced { GardenMaterials.castShadow(pot) }
      self.pot = pot
      self.soil = soil
      baseY = potHeight * 0.94
    }

    // The body is in unit space: scaled to (radius, height, radius) by the tween.
    stem.scale = SIMD3(0.09, 0.5, 0.09)
    body.addChild(stem)
    body.addChild(crown)
    body.position = SIMD3(0, baseY, 0)
    root.addChild(body)
    if !instanced { for e in [stem, crown] { GardenMaterials.castShadow(e) } }

    ring.position = SIMD3(0, (inPot ? 0 : baseY) + 0.006, 0)
    root.addChild(ring)
    cover.position = SIMD3(0, baseY, 0)
    cover.isEnabled = false
    root.addChild(cover)

    // Start from the previous node's values (a rebuild keeps the motion), or from nothing for the grow-in.
    if let previous {
      height = previous.height
      radius = previous.radius
      ringColour = previous.ringColour
      coverAmount = previous.coverAmount
    } else if growIn {
      height = 0.001
      radius = plant.shownRadius * 0.2
    } else {
      height = plant.shownHeight
      radius = plant.shownRadius
    }
    applyTransforms()
  }

  /// New values for the chosen date, mode and selection: sets the targets and the label.
  func update(_ plant: GardenPlant, mode: GardenMode, selected: Bool, materials: GardenMaterials) {
    self.plant = plant
    targetHeight = plant.shownHeight
    targetRadius = plant.shownRadius
    targetCover = (mode == .frost && plant.tender) ? 1 : 0
    targetRingScale = selected ? 1.18 : 1

    let ringTarget: UnlitMaterial
    let colour: UIColor
    if mode == .frost {
      // Frost rings the tender plants in the info colour (design.md, 3D or Map).
      colour = plant.tender ? materials.palette.info : materials.palette.border
      ringTarget = plant.tender ? materials.infoRing : materials.mutedRing
    } else {
      colour = materials.palette.status(plant.status)
      ringTarget = materials.ring(for: plant.status)
    }
    ringMaterial = ringTarget
    ringMaterialKey = mode == .frost ? (plant.tender ? "info" : "muted") : plant.status.rawValue
    targetRingColour = colour.rgba

    let dead = plant.status == .dead
    if dead != canopyDead {
      canopyDead = dead
      crownMaterial = dead ? materials.deadCanopy : materials.green(for: plant.name)
      crownKey = dead ? "dead" : materials.greenKey(for: plant.name)
      if instanced { instancesChanged = true } else { crown.model?.materials = [crownMaterial] }
    }

    let text: String
    if let given = plant.label {
      text = given
    } else if mode == .growth, let h = plant.height {
      text = "\(plant.name) · \(Int((h * 100).rounded())) cm"
    } else {
      text = plant.name
    }
    let key = "\(text)|\(plant.status.rawValue)|\(selected)"
    if key != labelKey {
      labelKey = key
      label.configure(text: text, status: plant.status, selected: selected, palette: materials.palette)
    }

    let spoken = plant.spokenText
    if spoken != accessibilityKey {
      accessibilityKey = spoken
      // The UIKit label is the VoiceOver element on every iOS: an AccessibilityComponent lives in the ARView, which
      // is out of the window while the garden is frozen. Labels shrunk to a dot keep their full words.
      label.accessibilityLabel = spoken
    }

    updateCollision()
  }

  /// One frame of the tween. Returns true while anything still moves.
  func step(dt: Float, snap: Bool) -> Bool {
    let k: Float = snap ? 1 : min(1, dt * 6)
    var moving = false
    func approach(_ v: inout Float, _ t: Float, _ eps: Float) {
      if abs(t - v) <= eps { v = t } else { v += (t - v) * k; moving = true }
    }
    approach(&height, targetHeight, 0.001)
    approach(&radius, targetRadius, 0.001)
    approach(&coverAmount, targetCover, 0.005)
    approach(&ringScale, targetRingScale, 0.002)

    if simd_length(targetRingColour - ringColour) <= 0.01 {
      ringColour = targetRingColour
      // Back on the shared material once the colour has arrived.
      if appliedRingKey != ringMaterialKey {
        appliedRingKey = ringMaterialKey
        if instanced {
          ring.model = nil
          ringInBatch = true
          instancesChanged = true
        } else {
          ring.model?.materials = [ringMaterial]
        }
      }
    } else {
      ringColour += (targetRingColour - ringColour) * k
      appliedRingKey = ""
      let tween = UnlitMaterial(color: UIColor(rgba: ringColour))
      if instanced, ring.model == nil {
        ring.model = ModelComponent(mesh: GardenMeshes.ring, materials: [tween])
        ringInBatch = false
        instancesChanged = true
      } else {
        ring.model?.materials = [tween]
      }
      moving = true
    }
    applyTransforms()
    return moving
  }

  /// The point above the plant where its label sits, in world space.
  var labelAnchor: SIMD3<Float> {
    root.position + SIMD3(0, baseY + height + 0.12, 0)
  }

  /// The middle of the crown, where a dot can sit when the space above the plant is taken.
  var crownAnchor: SIMD3<Float> {
    root.position + SIMD3(0, baseY + height * 0.7, 0)
  }

  private func applyTransforms() {
    body.scale = SIMD3(radius, max(height, 0.001), radius)
    let ringRadius = max(radius * 1.15 + 0.04, inPot ? potRadius + 0.05 : 0)
    ring.scale = SIMD3(repeating: ringRadius * ringScale)
    cover.isEnabled = coverAmount > 0.01
    let coverRadius = radius * 1.1 + 0.04
    cover.scale = SIMD3(coverRadius, height * 1.08 + 0.04, coverRadius) * max(coverAmount, 0.001)
  }

  /// One instance of a shared mesh: `key` names its batch (the part and its material).
  struct InstancePart {
    let key: String
    let mesh: MeshResource
    let material: Material
    let transform: float4x4
    let castsShadow: Bool
  }

  /// The instanced parts with their transforms in `space`, for GardenInstances. Empty when not instanced.
  func instanceParts(in space: Entity) -> [InstancePart] {
    guard instanced else { return [] }
    let shared = GardenMeshes.self
    var parts = [
      InstancePart(key: "stem", mesh: shared.stem, material: materials.stem, transform: stem.transformMatrix(relativeTo: space), castsShadow: true),
      InstancePart(key: "crown:" + crownKey, mesh: shared.crown, material: crownMaterial, transform: crown.transformMatrix(relativeTo: space), castsShadow: true),
    ]
    if ringInBatch {
      parts.append(InstancePart(key: "ring:" + ringMaterialKey, mesh: shared.ring, material: ringMaterial, transform: ring.transformMatrix(relativeTo: space), castsShadow: false))
    }
    if let pot, let soil {
      parts.append(InstancePart(key: "pot", mesh: shared.pot, material: materials.pot, transform: pot.transformMatrix(relativeTo: space), castsShadow: true))
      parts.append(InstancePart(key: "soil", mesh: shared.disc, material: materials.soil, transform: soil.transformMatrix(relativeTo: space), castsShadow: false))
    }
    return parts
  }

  private func updateCollision() {
    let r = max(Self.minHitRadius, targetRadius)
    let h = max(targetHeight, 0.3) + 0.1
    let key = SIMD2(r, h)
    guard key != collisionKey else { return }
    collisionKey = key
    let shape = ShapeResource.generateBox(size: SIMD3(2 * r, baseY + h, 2 * r)).offsetBy(translation: SIMD3(0, (baseY + h) / 2, 0))
    root.components.set(CollisionComponent(shapes: [shape]))
  }
}

/// Materials shared by every plant and bed, per scheme. Flat colours, no textures (garden-3d 3).
@MainActor
final class GardenMaterials {
  let palette: GardenPalette
  let plinth: SimpleMaterial
  let plaster: SimpleMaterial
  let soil: SimpleMaterial
  let pot: SimpleMaterial
  let stem: SimpleMaterial
  let deadCanopy: SimpleMaterial
  let cover: PhysicallyBasedMaterial
  let infoRing: UnlitMaterial
  let mutedRing: UnlitMaterial
  private var greens: [String: SimpleMaterial] = [:]
  private var rings: [GardenStatus: UnlitMaterial] = [:]

  init(palette: GardenPalette) {
    self.palette = palette
    plinth = Self.lit(palette.plinth)
    plaster = Self.lit(palette.plaster)
    soil = Self.lit(palette.soil)
    pot = Self.lit(palette.pot, roughness: 0.8)
    stem = Self.lit(UIColor(hex: 0x5E6B3A))
    deadCanopy = Self.lit(UIColor(hex: 0x8A7F62))
    var c = PhysicallyBasedMaterial()
    c.baseColor = .init(tint: palette.cover)
    c.roughness = .init(floatLiteral: 0.95)
    c.blending = .transparent(opacity: .init(floatLiteral: 0.6))
    cover = c
    infoRing = UnlitMaterial(color: palette.info)
    mutedRing = UnlitMaterial(color: palette.border)
  }

  /// The batch key of a plant's green: plants of one green share a material.
  func greenKey(for name: String) -> String { "green:\(palette.green(for: name).rgba)" }

  func green(for name: String) -> SimpleMaterial {
    let colour = palette.green(for: name)
    let key = "\(colour.rgba)"
    if let m = greens[key] { return m }
    let m = Self.lit(colour)
    greens[key] = m
    return m
  }

  func ring(for status: GardenStatus) -> UnlitMaterial {
    if let m = rings[status] { return m }
    let m = UnlitMaterial(color: palette.status(status))
    rings[status] = m
    return m
  }

  static func lit(_ colour: UIColor, roughness: Float = 0.9) -> SimpleMaterial {
    SimpleMaterial(color: colour, roughness: .float(roughness), isMetallic: false)
  }

  /// A contact shadow (iOS 18). Earlier systems draw none: the garden adds no shadow map (garden-3d 3).
  static func castShadow(_ entity: Entity) {
    if #available(iOS 18.0, *) {
      entity.components.set(GroundingShadowComponent(castsShadow: true))
    }
  }
}
