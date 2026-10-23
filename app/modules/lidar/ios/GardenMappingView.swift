import ARKit
import CoreLocation
import ExpoModulesCore
import RealityKit
import UIKit

// The garden mapping session (architecture 6.8; flows 11): ARKit world tracking with scene reconstruction. The user
// taps bed corners on the ground and drops a pin on each plant; the view reports points in metres on a local ground
// plane (x to the right of where the phone first looked, y ahead of it), a canopy radius per pin from the mesh, the
// tracking state and the compass. The mesh, the camera frames and the world map never leave the phone.

final class GardenMappingView: ExpoView, ARSessionDelegate, CLLocationManagerDelegate {
  /// The session on screen, for the module's world-map functions. One mapping view at a time.
  static weak var current: GardenMappingView?

  let onTrackingState = EventDispatcher()
  let onGroundTap = EventDispatcher()
  let onPinPlaced = EventDispatcher()
  let onHeading = EventDispatcher()
  let onMountError = EventDispatcher()

  /// "corner" puts a bed corner on the ground, "pin" a plant's pin.
  var tapMode = "corner"

  private let arView = ARView(frame: .zero, cameraMode: .ar, automaticallyConfigureSession: false)
  private let location = CLLocationManager()
  /// Apple's coaching view: how to move the phone at the start and while finding a saved map (HIG, augmented reality).
  private let coaching = ARCoachingOverlayView()
  private var markers: [AnchorEntity] = []
  private var running = false
  private var lastState: String?
  private var lastHeading: CFTimeInterval = 0
  private var relocalised: ((String) -> Void)?

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    arView.session.delegate = self
    arView.environment.sceneUnderstanding.options = []
    // The reconstructed mesh drawn over the camera, so the user sees what has been mapped (garden-3d 7, item 4).
    arView.debugOptions = [.showSceneUnderstanding]
    addSubview(arView)
    coaching.session = arView.session
    coaching.goal = .horizontalPlane
    coaching.activatesAutomatically = true
    coaching.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    arView.addSubview(coaching)
    arView.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped(_:))))
    location.delegate = self
    location.headingFilter = 1
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    arView.frame = bounds
    coaching.frame = arView.bounds
  }

  // The session runs only while the view is on screen: closing mapping ends the only AR session.
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window != nil {
      Self.current = self
      if !running { start(worldMap: nil) }
    } else {
      stop()
    }
  }

  private func start(worldMap: ARWorldMap?) {
    guard ARWorldTrackingConfiguration.isSupported else {
      onMountError(["message": "This iPhone can't map a garden."])
      return
    }
    let config = ARWorldTrackingConfiguration()
    config.worldAlignment = .gravity
    config.planeDetection = [.horizontal]
    if ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh) { config.sceneReconstruction = .mesh }
    config.initialWorldMap = worldMap
    arView.session.run(config, options: [.resetTracking, .removeExistingAnchors])
    running = true
    if CLLocationManager.headingAvailable() { location.startUpdatingHeading() }
  }

  private func stop() {
    guard running else { return }
    arView.session.pause()
    location.stopUpdatingHeading()
    running = false
    relocalised?("failed")
    relocalised = nil
    if Self.current === self { Self.current = nil }
  }

  // MARK: Taps

  @objc private func tapped(_ gesture: UITapGestureRecognizer) {
    let point = gesture.location(in: arView)
    if tapMode == "pin" {
      // A plant is rarely a plane: any surface, the mesh included, then the ground under it.
      guard let hit = arView.raycast(from: point, allowing: .estimatedPlane, alignment: .any).first else { return }
      let p = hit.worldTransform.columns.3
      let radius = canopyRadius(x: p.x, z: p.z)
      mark(p, colour: .systemGreen, radius: 0.03)
      var event: [String: Any] = ["x_m": Double(p.x), "y_m": Double(-p.z)]
      if let radius { event["canopy_radius_m"] = Double(radius) }
      onPinPlaced(event)
    } else {
      guard let hit = arView.raycast(from: point, allowing: .estimatedPlane, alignment: .horizontal).first else { return }
      let p = hit.worldTransform.columns.3
      mark(p, colour: .white, radius: 0.02)
      onGroundTap(["x_m": Double(p.x), "y_m": Double(-p.z)])
    }
  }

  private func mark(_ p: SIMD4<Float>, colour: UIColor, radius: Float) {
    let anchor = AnchorEntity(world: SIMD3(p.x, p.y, p.z))
    anchor.addChild(ModelEntity(mesh: .generateSphere(radius: radius), materials: [UnlitMaterial(color: colour)]))
    arView.scene.addAnchor(anchor)
    markers.append(anchor)
  }

  /// Starts over with no map: after a failed or cancelled relocalisation, or the screen's Start again.
  func restart() {
    relocalised?("failed")
    relocalised = nil
    clearMarkers()
    lastState = nil
    start(worldMap: nil)
  }

  /// Removes the last marker: the screen's Undo.
  func undoLast() {
    guard let last = markers.popLast() else { return }
    arView.scene.removeAnchor(last)
  }

  func clearMarkers() {
    markers.forEach { arView.scene.removeAnchor($0) }
    markers.removeAll()
  }

  /// The canopy's radius around a pin from the reconstructed mesh: the ground is the lowest mesh point within a
  /// metre, the plant is what stands more than 5 cm above it within 75 cm, and the radius is the 90th percentile of
  /// its horizontal distances. Nil with too little mesh. Shown on the map, not used by any rule (architecture 6.8).
  private func canopyRadius(x: Float, z: Float) -> Float? {
    guard let frame = arView.session.currentFrame else { return nil }
    var near: [(d: Float, y: Float)] = []
    for case let mesh as ARMeshAnchor in frame.anchors {
      let vertices = mesh.geometry.vertices
      let buffer = vertices.buffer.contents()
      for i in 0..<vertices.count {
        let v = buffer.advanced(by: vertices.offset + vertices.stride * i).assumingMemoryBound(to: SIMD3<Float>.self).pointee
        let world = mesh.transform * SIMD4(v.x, v.y, v.z, 1)
        let d = ((world.x - x) * (world.x - x) + (world.z - z) * (world.z - z)).squareRoot()
        if d <= 1 { near.append((d, world.y)) }
      }
    }
    guard let ground = near.map(\.y).min() else { return nil }
    let plant = near.filter { $0.d <= 0.75 && $0.y > ground + 0.05 }.map(\.d).sorted()
    guard plant.count >= 30 else { return nil }
    return plant[min(plant.count - 1, Int(Float(plant.count) * 0.9))]
  }

  // MARK: Tracking

  func session(_ session: ARSession, cameraDidChangeTrackingState camera: ARCamera) {
    let state: String
    switch camera.trackingState {
    case .normal: state = "normal"
    case .limited: state = "limited"
    case .notAvailable: state = "lost"
    }
    if state == "normal", let done = relocalised {
      relocalised = nil
      done("ok")
    }
    guard state != lastState else { return }
    lastState = state
    onTrackingState(["state": state])
  }

  func session(_ session: ARSession, didFailWithError error: Error) {
    onTrackingState(["state": "lost"])
    relocalised?("failed")
    relocalised = nil
  }

  // MARK: Compass

  /// North as the map needs it: clockwise from the map's +y axis. The camera's heading on the map comes from ARKit,
  /// its heading on the Earth from the compass, read at the same moment with the phone held up.
  func locationManager(_ manager: CLLocationManager, didUpdateHeading heading: CLHeading) {
    let now = CACurrentMediaTime()
    guard now - lastHeading >= 1, heading.headingAccuracy >= 0, let frame = arView.session.currentFrame,
      case .normal = frame.camera.trackingState
    else { return }
    lastHeading = now
    let forward = -frame.camera.transform.columns.2
    guard (forward.x * forward.x + forward.z * forward.z) > 0.1 else { return }
    let onMap = Double(atan2(forward.x, -forward.z)) * 180 / .pi
    let onEarth = heading.trueHeading >= 0 ? heading.trueHeading : heading.magneticHeading
    let north = (onMap - onEarth).truncatingRemainder(dividingBy: 360)
    onHeading(["north_deg": north < 0 ? north + 360 : north, "accuracy_deg": heading.headingAccuracy])
  }

  // MARK: World map

  /// Saves the session's world map on the phone, tagged with the garden and the map version it produced.
  func saveWorldMap(gardenId: String, version: Int, promise: Promise) {
    arView.session.getCurrentWorldMap { map, error in
      guard let map else {
        promise.reject("ERR_LIDAR_WORLDMAP", error?.localizedDescription ?? "The garden isn't mapped enough yet.")
        return
      }
      do {
        let data = try NSKeyedArchiver.archivedData(withRootObject: map, requiringSecureCoding: true)
        try data.write(to: try WorldMaps.url(gardenId), options: [.atomic, .completeFileProtection])
        UserDefaults.standard.set(version, forKey: WorldMaps.versionKey(gardenId))
        promise.resolve(nil)
      } catch {
        promise.reject("ERR_LIDAR_WORLDMAP", error.localizedDescription)
      }
    }
  }

  /// Restarts the session from the saved world map, and resolves "ok" once tracking is normal again, or "failed"
  /// after 20 seconds, when the map can't be read, or when mapping closes first.
  func relocalise(gardenId: String, promise: Promise) {
    guard
      let data = try? Data(contentsOf: WorldMaps.url(gardenId)),
      let map = try? NSKeyedUnarchiver.unarchivedObject(ofClass: ARWorldMap.self, from: data)
    else {
      promise.resolve("failed")
      return
    }
    relocalised?("failed")
    clearMarkers()
    lastState = nil
    var settled = false
    relocalised = { result in
      guard !settled else { return }
      settled = true
      promise.resolve(result)
    }
    start(worldMap: map)
    DispatchQueue.main.asyncAfter(deadline: .now() + 20) { [weak self] in
      guard let self, let done = self.relocalised else { return }
      self.relocalised = nil
      done("failed")
    }
  }
}

/// Where world maps are kept: Application Support, which the Files app doesn't show, one per garden.
enum WorldMaps {
  static func url(_ gardenId: String) throws -> URL {
    let dir = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
      .appendingPathComponent("worldmaps", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let safe = gardenId.filter { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" }
    return dir.appendingPathComponent("\(safe).arworldmap")
  }

  static func versionKey(_ gardenId: String) -> String { "pdm.worldmap.version.\(gardenId)" }

  static func savedVersion(_ gardenId: String) -> Int? {
    guard let url = try? url(gardenId), FileManager.default.fileExists(atPath: url.path) else { return nil }
    return UserDefaults.standard.object(forKey: versionKey(gardenId)) as? Int
  }

  /// Deletes every saved world map: on sign-out.
  static func removeAll() {
    guard let dir = try? FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: false)
      .appendingPathComponent("worldmaps", isDirectory: true)
    else { return }
    try? FileManager.default.removeItem(at: dir)
    for key in UserDefaults.standard.dictionaryRepresentation().keys where key.hasPrefix("pdm.worldmap.version.") {
      UserDefaults.standard.removeObject(forKey: key)
    }
  }
}
