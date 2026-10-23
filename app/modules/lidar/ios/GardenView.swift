import Combine
import ExpoModulesCore
import RealityKit
import UIKit
import simd

// The 3D garden on RealityKit (docs/design/garden-3d.md 1 and 7; design.md, the showroom): an ARView with the
// camera off, so it runs no ARSession and the mapping view stays the only camera owner. JavaScript sends the map's
// beds and each plant's values for the chosen date; the view tweens to them. Orbit with pan and pinch, tap to
// select, UIKit labels placed with project(_:) on every SceneEvents.Update.
//
// Idle frames: RealityKit has no public render-on-demand switch for ARView. When nothing has moved for a moment
// (no gesture, no tween, the turntable done or off), the view snapshots itself, shows the snapshot and takes the
// ARView out of the window, which stops its drawing; the next gesture, tap, prop change or new size puts it back.
// The first build woke on every layout and drew 60 fps idle; check again with Metal System Trace.

final class GardenView: ExpoView {
  let onSelectPlant = EventDispatcher()

  // Props, applied together in applyProps() once React has set them all.
  var beds: [GardenBed] = []
  var plants: [GardenPlant] = []
  var mode: GardenMode = .health
  var selectedPlantId: String?
  var scheme = "light"
  var reduceMotionProp = false

  private static let fieldOfView: Float = 24
  private static let minDistance: Float = 16.8
  private static let bedHeight: Float = 0.2
  private static let plinthHeight: Float = 0.12
  private static let minElevation: Float = 10 * .pi / 180
  private static let maxElevation: Float = 85 * .pi / 180
  /// The turntable waits this long after the last touch, then turns for `turntableRun` seconds.
  private static let turntableDelay: CFTimeInterval = 3
  private static let turntableRun: CFTimeInterval = 40
  private static let turntableSpeed: Float = 4 * .pi / 180
  /// Nothing moving for this long freezes the view.
  private static let freezeAfter: CFTimeInterval = 0.6

  private let arView = ARView(frame: .zero, cameraMode: .nonAR, automaticallyConfigureSession: false)
  private let snapshotView = UIImageView()
  private let labelLayer = UIView()
  private let worldAnchor = AnchorEntity(world: .zero)
  private let staticRoot = Entity()
  private let plantsRoot = Entity()
  private let camera = PerspectiveCamera()
  private let keyLight = DirectionalLight()
  private var updateSubscription: (any Cancellable)?

  private var nodes: [String: PlantNode] = [:]
  private var builtBedsKey: String?
  private var builtPlantsKey: String?
  private var builtScheme: String?
  private var materials = GardenMaterials(palette: .light)

  // The orbit camera.
  private var target = SIMD3<Float>(0, 0.2, 0)
  private var azimuth: Float = 0.5
  private var elevation: Float = 38 * .pi / 180
  private var zoom: Float = 1
  private var sceneExtent = SIMD2<Float>(4, 4)
  private var cameraDirty = true
  private var labelsDirty = true
  private var laidOutSize = CGSize.zero

  // Motion and idle.
  private var gestureActive = false
  private var lastInteraction = CACurrentMediaTime()
  private var idleSince: CFTimeInterval?
  private var frozen = false
  private var freezing = false
  private var systemReduceMotion = UIAccessibility.isReduceMotionEnabled
  private var reduceMotion: Bool { reduceMotionProp || systemReduceMotion }

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true

    arView.renderOptions = [.disableMotionBlur, .disableDepthOfField, .disableCameraGrain, .disablePersonOcclusion, .disableFaceMesh]
    arView.environment.background = .color(GardenPalette.light.background)
    arView.isUserInteractionEnabled = false
    addSubview(arView)

    snapshotView.isHidden = true
    snapshotView.contentMode = .scaleToFill
    snapshotView.isUserInteractionEnabled = false
    addSubview(snapshotView)

    labelLayer.isUserInteractionEnabled = false
    addSubview(labelLayer)

    camera.camera.fieldOfViewInDegrees = Self.fieldOfView
    camera.camera.near = 0.1
    camera.camera.far = 200
    keyLight.light.color = UIColor(hex: 0xFFEFD8)
    keyLight.light.intensity = 2600
    keyLight.look(at: .zero, from: SIMD3(-4, 9, 6), relativeTo: nil)

    worldAnchor.addChild(staticRoot)
    worldAnchor.addChild(plantsRoot)
    worldAnchor.addChild(camera)
    worldAnchor.addChild(keyLight)
    arView.scene.addAnchor(worldAnchor)

    updateSubscription = arView.scene.subscribe(to: SceneEvents.Update.self) { [weak self] event in
      MainActor.assumeIsolated {
        self?.tick(dt: Float(event.deltaTime))
      }
    }

    let pan = UIPanGestureRecognizer(target: self, action: #selector(onPan(_:)))
    pan.maximumNumberOfTouches = 1
    let pinch = UIPinchGestureRecognizer(target: self, action: #selector(onPinch(_:)))
    let tap = UITapGestureRecognizer(target: self, action: #selector(onTap(_:)))
    addGestureRecognizer(pan)
    addGestureRecognizer(pinch)
    addGestureRecognizer(tap)

    isAccessibilityElement = false
    NotificationCenter.default.addObserver(
      self, selector: #selector(reduceMotionChanged), name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
    NotificationCenter.default.addObserver(
      self, selector: #selector(contentSizeChanged), name: UIContentSizeCategory.didChangeNotification, object: nil)
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    // Freezing takes the ARView out, which lays this view out again: only a new size wakes it, or the view would
    // loop freeze, snapshot, wake (measured at 60 fps idle on an iPhone 13 Pro Max).
    guard bounds.size != laidOutSize else { return }
    laidOutSize = bounds.size
    arView.frame = bounds
    snapshotView.frame = bounds
    labelLayer.frame = bounds
    cameraDirty = true
    labelsDirty = true
    wake()
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window != nil { wake() }
  }

  // MARK: Props

  func applyProps() {
    wake()
    let schemeChanged = scheme != builtScheme
    if schemeChanged {
      builtScheme = scheme
      let palette = GardenPalette.of(scheme)
      materials = GardenMaterials(palette: palette)
      arView.environment.background = .color(palette.background)
      keyLight.light.intensity = scheme == "dark" ? 1800 : 2600
    }
    let bedsKey = beds.map { "\($0.id):\($0.polygon)" }.joined(separator: ";")
    let plantsKey = plants.map(\.placementKey).joined(separator: ";")
    if schemeChanged || bedsKey != builtBedsKey || plantsKey != builtPlantsKey {
      builtBedsKey = bedsKey
      builtPlantsKey = plantsKey
      rebuild()
    }
    for plant in plants {
      nodes[plant.id]?.update(plant, mode: mode, selected: plant.id == selectedPlantId, materials: materials)
    }
    labelsDirty = true
  }

  /// Builds the plinth, the beds and the plants. Only a change to the beds, the plants' places or the scheme
  /// comes here; a new date only moves the targets.
  private func rebuild() {
    let previous = nodes
    let firstBuild = previous.isEmpty
    staticRoot.children.removeAll()
    plantsRoot.children.removeAll()
    for node in previous.values { node.label.removeFromSuperview() }
    nodes = [:]

    // Bounds of everything on the map, centred on the origin.
    var lo = SIMD2<Float>(repeating: .greatestFiniteMagnitude)
    var hi = SIMD2<Float>(repeating: -.greatestFiniteMagnitude)
    for bed in beds { for p in bed.polygon { lo = simd_min(lo, p); hi = simd_max(hi, p) } }
    for plant in plants {
      let p = SIMD2(plant.x, plant.y), r = SIMD2<Float>(repeating: plant.shownRadius)
      lo = simd_min(lo, p - r)
      hi = simd_max(hi, p + r)
    }
    if lo.x > hi.x { lo = SIMD2(-1, -1); hi = SIMD2(1, 1) }
    let centre = (lo + hi) / 2
    let size = hi - lo + SIMD2(repeating: 1.6)
    sceneExtent = size

    // The pale sage plinth (design.md, the showroom), its top at y = 0.
    let plinth = ModelEntity(
      mesh: MeshResource.generateBox(size: SIMD3(size.x, Self.plinthHeight, size.y), cornerRadius: 0.06),
      materials: [materials.plinth])
    plinth.position = SIMD3(0, -Self.plinthHeight / 2, 0)
    staticRoot.addChild(plinth)

    // Plaster beds extruded from the map's polygons, with terracotta soil. The map's y is the session's -z
    // (GardenMappingView), so it goes back to -z here; +z would draw the garden's mirror image.
    var bedIds = Set<String>()
    for bed in beds {
      let polygon = bed.polygon.map { SIMD2($0.x - centre.x, -($0.y - centre.y)) }
      guard let meshes = GardenMeshes.bed(polygon: polygon, height: Self.bedHeight, inset: 0.06) else { continue }
      bedIds.insert(bed.id)
      let plaster = ModelEntity(mesh: meshes.plaster, materials: [materials.plaster])
      let soil = ModelEntity(mesh: meshes.soil, materials: [materials.soil])
      GardenMaterials.castShadow(plaster)
      staticRoot.addChild(plaster)
      staticRoot.addChild(soil)
    }

    let growIn = firstBuild && !reduceMotion
    for plant in plants where nodes[plant.id] == nil {
      let inBed = plant.bedId.map { bedIds.contains($0) } ?? false
      let node = PlantNode(
        plant: plant,
        position: SIMD3(plant.x - centre.x, 0, -(plant.y - centre.y)),
        bedTop: inBed ? Self.bedHeight : nil,
        materials: materials,
        from: previous[plant.id],
        growIn: growIn || previous[plant.id] == nil && !firstBuild && !reduceMotion)
      node.label.onActivate = { [weak self] in self?.select(plant.id) }
      nodes[plant.id] = node
      plantsRoot.addChild(node.root)
      labelLayer.addSubview(node.label)
    }

    target = SIMD3(0, Self.bedHeight, 0)
    cameraDirty = true
    labelsDirty = true
  }

  // MARK: Frame

  private func tick(dt: Float) {
    let now = CACurrentMediaTime()
    var moving = false

    let sinceTouch = now - lastInteraction
    if !reduceMotion, !gestureActive, sinceTouch > Self.turntableDelay, sinceTouch < Self.turntableDelay + Self.turntableRun {
      azimuth += Self.turntableSpeed * dt
      cameraDirty = true
      moving = true
    }

    if cameraDirty { placeCamera() }
    for node in nodes.values where node.step(dt: dt, snap: reduceMotion) {
      moving = true
    }
    if cameraDirty || moving || labelsDirty {
      layoutLabels()
      cameraDirty = false
      labelsDirty = false
    }

    // Keep drawing while the turntable is still to come; otherwise freeze once still.
    let turntablePending = !reduceMotion && sinceTouch < Self.turntableDelay + Self.turntableRun
    if moving || gestureActive || turntablePending {
      idleSince = nil
    } else if let since = idleSince {
      if now - since > Self.freezeAfter { freeze() }
    } else {
      idleSince = now
    }
  }

  private func distance() -> Float {
    // Fit the plinth in the narrower of the two fields of view, never nearer than 16.8 m (the 24° long lens). The
    // orbit turns the plinth, so fit its diagonal, the widest it can look; the longer side alone let a 6-plant
    // garden run off the right edge at the first framing.
    let half = Self.fieldOfView / 2 * .pi / 180
    let aspect = bounds.height > 0 ? Float(bounds.width / bounds.height) : 1
    let horizontalHalf = atan(tan(half) * aspect)
    let extent = simd_length(sceneExtent)
    let fit = extent / 2 / tan(min(half, horizontalHalf)) * 1.1
    return max(Self.minDistance, fit) * zoom
  }

  private func placeCamera() {
    let d = distance()
    let c = cos(elevation)
    let position = target + d * SIMD3(c * sin(azimuth), sin(elevation), c * cos(azimuth))
    camera.look(at: target, from: position, relativeTo: nil)
  }

  /// Places the labels: the selected plant's first, then the nearest. A label that would overlap one already placed
  /// shrinks to its status dot (garden-3d 5), so a zoomed-out or crowded garden shows dots, not a pile of names.
  private func layoutLabels() {
    let cameraPosition = camera.position(relativeTo: nil)
    let forward = -camera.transform.matrix.columns.2
    let forward3 = SIMD3(forward.x, forward.y, forward.z)
    let visible = bounds.insetBy(dx: -40, dy: -40)
    var placed: [(node: PlantNode, point: CGPoint, depth: Float)] = []
    for node in nodes.values {
      let anchor = node.labelAnchor
      let depth = simd_dot(anchor - cameraPosition, forward3)
      guard depth > 0, let point = arView.project(anchor), visible.contains(point) else {
        node.label.isHidden = true
        continue
      }
      placed.append((node, point, depth))
    }
    placed.sort { a, b in
      let sa = a.node.id == selectedPlantId, sb = b.node.id == selectedPlantId
      return sa != sb ? sa : a.depth < b.depth
    }
    var taken: [CGRect] = []
    for (node, point, _) in placed {
      let full = node.label.fullBounds
      let frame = CGRect(x: point.x - full.width / 2, y: point.y - full.height, width: full.width, height: full.height)
      let fits = node.id == selectedPlantId || !taken.contains { $0.intersects(frame.insetBy(dx: -2, dy: -2)) }
      node.label.setCompact(!fits)
      if fits { taken.append(frame) }
      node.label.isHidden = false
      node.label.center = CGPoint(x: point.x, y: point.y - node.label.bounds.height / 2)
    }
  }

  // MARK: Idle

  private func freeze() {
    guard !frozen, !freezing, arView.superview != nil, window != nil, bounds.width > 0 else { return }
    freezing = true
    arView.snapshot(saveToHDR: false) { [weak self] image in
      guard let self else { return }
      self.freezing = false
      guard let image, self.idleSince != nil, !self.frozen else { return }
      self.snapshotView.image = image
      self.snapshotView.isHidden = false
      self.arView.removeFromSuperview()
      self.frozen = true
    }
  }

  private func wake() {
    idleSince = nil
    guard frozen else { return }
    frozen = false
    insertSubview(arView, belowSubview: snapshotView)
    arView.frame = bounds
    // Keep the snapshot up until the ARView has drawn again.
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { [weak self] in
      guard let self, !self.frozen else { return }
      self.snapshotView.isHidden = true
      self.snapshotView.image = nil
    }
  }

  // MARK: Gestures

  @objc private func onPan(_ g: UIPanGestureRecognizer) {
    interaction(g.state)
    let t = g.translation(in: self)
    g.setTranslation(.zero, in: self)
    azimuth -= Float(t.x) * 0.008
    elevation = min(Self.maxElevation, max(Self.minElevation, elevation + Float(t.y) * 0.006))
    cameraDirty = true
  }

  @objc private func onPinch(_ g: UIPinchGestureRecognizer) {
    interaction(g.state)
    if g.scale > 0 { zoom = min(1.6, max(0.4, zoom / Float(g.scale))) }
    g.scale = 1
    cameraDirty = true
  }

  @objc private func onTap(_ g: UITapGestureRecognizer) {
    interaction(.ended)
    let point = g.location(in: self)
    var hit: String?
    // A label is a target too.
    for node in nodes.values where !node.label.isHidden && node.label.frame.insetBy(dx: -6, dy: -6).contains(point) {
      hit = node.id
      break
    }
    if hit == nil {
      var entity = arView.entity(at: point)
      while let e = entity, hit == nil {
        if e.name.hasPrefix(PlantNode.namePrefix) { hit = String(e.name.dropFirst(PlantNode.namePrefix.count)) }
        entity = e.parent
      }
    }
    select(hit)
  }

  /// Shown at once; JavaScript confirms through selectedPlantId.
  private func select(_ plantId: String?) {
    lastInteraction = CACurrentMediaTime()
    selectedPlantId = plantId
    applyProps()
    onSelectPlant(["plant_id": plantId.map { $0 as Any } ?? NSNull()])
  }

  private func interaction(_ state: UIGestureRecognizer.State) {
    wake()
    lastInteraction = CACurrentMediaTime()
    switch state {
    case .began, .changed: gestureActive = true
    default: gestureActive = false
    }
  }

  @objc private func reduceMotionChanged() {
    systemReduceMotion = UIAccessibility.isReduceMotionEnabled
    wake()
  }

  @objc private func contentSizeChanged() {
    for node in nodes.values { node.label.sizeToFitContent() }
    labelsDirty = true
    wake()
  }
}
