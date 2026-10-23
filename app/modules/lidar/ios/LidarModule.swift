import ARKit
import AVFoundation
import ExpoModulesCore

// The LiDAR module's definition (docs/frontend/README.md 13, the module's JavaScript surface). The views and their
// work live in their own files.
public final class LidarModule: Module {
  public func definition() -> ModuleDefinition {
    Name("Lidar")

    /// `depth`: a back LiDAR depth camera. `mapping`: ARKit's scene reconstruction (architecture 6.7).
    Function("getCapabilities") { () -> [String: Bool] in
      [
        "depth": AVCaptureDevice.default(.builtInLiDARDepthCamera, for: .video, position: .back) != nil,
        "mapping": ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh),
      ]
    }

    /// The median depth in metres around each point of a depth PNG this module wrote, for the day-1 check's ruler.
    /// Points are normalised, in the sensor frame; null where there is no depth.
    AsyncFunction("sampleDepth") { (uri: URL, points: [[Double]], halfSide: Int) -> [Double?] in
      guard let grid = DepthGrid(png: uri) else {
        throw Exception(name: "ERR_LIDAR_DEPTH", description: "The depth map couldn't be read.")
      }
      return points.map { p in
        guard p.count == 2 else { return nil }
        return grid.median(atX: p[0], y: p[1], halfSide: max(0, halfSide)).map(Double.init)
      }
    }

    /// The world map stays on the phone, one per garden, tagged with the map version it produced (architecture 6.8).
    AsyncFunction("saveWorldMap") { (gardenId: String, version: Int, promise: Promise) in
      guard let view = GardenMappingView.current else {
        promise.reject("ERR_LIDAR_NO_SESSION", "No mapping session is open.")
        return
      }
      view.saveWorldMap(gardenId: gardenId, version: version, promise: promise)
    }.runOnQueue(.main)

    Function("savedWorldMapVersion") { (gardenId: String) -> Int? in
      WorldMaps.savedVersion(gardenId)
    }

    AsyncFunction("relocalise") { (gardenId: String, promise: Promise) in
      guard let view = GardenMappingView.current else {
        promise.resolve("failed")
        return
      }
      view.relocalise(gardenId: gardenId, promise: promise)
    }.runOnQueue(.main)

    /// A stale map, or a garden the user left or deleted.
    Function("removeWorldMap") { (gardenId: String) in
      WorldMaps.remove(gardenId)
    }

    /// On sign-out: nothing of a garden's space stays on the phone.
    Function("removeWorldMaps") {
      WorldMaps.removeAll()
    }

    View(GardenMappingView.self) {
      Events("onTrackingState", "onGroundTap", "onPinPlaced", "onHeading", "onMountError", "onInterruption")

      Prop("tapMode") { (view: GardenMappingView, mode: String) in
        view.tapMode = mode
      }

      AsyncFunction("undoLast") { (view: GardenMappingView) in
        view.undoLast()
      }.runOnQueue(.main)

      AsyncFunction("clearMarkers") { (view: GardenMappingView) in
        view.clearMarkers()
      }.runOnQueue(.main)

      AsyncFunction("restart") { (view: GardenMappingView) in
        view.restart()
      }.runOnQueue(.main)
    }

    View(LidarCameraView.self) {
      Events("onDistance", "onReady", "onMountError", "onInterruption")

      Prop("active") { (view: LidarCameraView, active: Bool) in
        view.setActive(active)
      }

      AsyncFunction("capture") { (view: LidarCameraView, promise: Promise) in
        view.capture(promise: promise)
      }
    }
  }
}
