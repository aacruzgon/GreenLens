import RealityKit
import simd

// The plants' shared meshes drawn as instances (garden-3d 1 and 3; RealityKit's MeshInstancesComponent, iOS 26):
// one entity per mesh and material, holding every plant's transform for it. Forty plants come to a dozen draws
// (the stems, the pots, the soil, a crown per green and a ring per status) instead of four to six each.
//
// Instances share their entity's material (Apple has no per-instance colour without a custom shader), so a batch's
// key is its part and its material. Only a change in a batch's size makes a new instance buffer; a move rewrites
// the transforms in place.

@available(iOS 26.0, *)
@MainActor
final class GardenInstances {
  /// Sits beside the plants' root, both at the identity, so the parts' transforms in that root are local here.
  let root = Entity()

  private final class Batch {
    let entity: ModelEntity
    let data: LowLevelInstanceData
    let count: Int

    init(entity: ModelEntity, data: LowLevelInstanceData, count: Int) {
      self.entity = entity
      self.data = data
      self.count = count
    }
  }

  private var batches: [String: Batch] = [:]

  /// Every plant's instanced parts, grouped by batch.
  func update(_ parts: [PlantNode.InstancePart]) {
    var groups: [String: (first: PlantNode.InstancePart, transforms: [float4x4])] = [:]
    for part in parts { groups[part.key, default: (part, [])].transforms.append(part.transform) }

    for (key, batch) in batches where groups[key] == nil {
      batch.entity.removeFromParent()
      batches[key] = nil
    }
    for (key, group) in groups {
      let transforms = group.transforms
      if let batch = batches[key], batch.count == transforms.count {
        batch.data.withMutableTransforms { t in
          for (i, m) in transforms.enumerated() { t[i] = m }
        }
        continue
      }
      do {
        let data = try LowLevelInstanceData(instanceCount: transforms.count)
        data.withMutableTransforms { t in
          for (i, m) in transforms.enumerated() { t[i] = m }
        }
        let mesh = group.first.mesh
        let entity = batches[key]?.entity ?? makeEntity(group.first)
        entity.components.set(try MeshInstancesComponent(mesh: mesh, modelID: mesh.contents.models.first?.id, instances: data, bounds: nil))
        batches[key] = Batch(entity: entity, data: data, count: transforms.count)
      } catch {
        // Without its buffer the batch isn't drawn; the next update tries again.
        batches[key]?.entity.removeFromParent()
        batches[key] = nil
      }
    }
  }

  func removeAll() {
    for batch in batches.values { batch.entity.removeFromParent() }
    batches = [:]
  }

  private func makeEntity(_ part: PlantNode.InstancePart) -> ModelEntity {
    let entity = ModelEntity(mesh: part.mesh, materials: [part.material])
    if part.castsShadow { GardenMaterials.castShadow(entity) }
    root.addChild(entity)
    return entity
  }
}
