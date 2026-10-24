import ExpoModulesCore

// The 3D garden's module (docs/design/garden-3d.md 1, "Hosting it in React Native"; roadmap F5 items 3 and 4).
// A module of its own, so the JavaScript side can find it as "LidarGarden" and fall back to the 2D map without it.
public final class LidarGardenModule: Module {
  public func definition() -> ModuleDefinition {
    Name("LidarGarden")

    View(GardenView.self) {
      Events("onSelectPlant")

      Prop("beds") { (view: GardenView, beds: [GardenBedRecord]) in
        view.beds = beds.map(GardenBed.init)
      }

      Prop("plants") { (view: GardenView, plants: [GardenPlantRecord]) in
        view.plants = plants.map(GardenPlant.init)
      }

      Prop("mode") { (view: GardenView, mode: String) in
        view.mode = GardenMode(rawValue: mode) ?? .health
      }

      Prop("selectedPlantId") { (view: GardenView, id: String?) in
        view.selectedPlantId = id
      }

      Prop("scheme") { (view: GardenView, scheme: String) in
        view.scheme = scheme == "dark" ? "dark" : "light"
      }

      Prop("reduceMotion") { (view: GardenView, reduce: Bool?) in
        view.reduceMotionProp = reduce ?? false
      }

      OnViewDidUpdateProps { (view: GardenView) in
        view.applyProps()
      }
    }
  }
}
