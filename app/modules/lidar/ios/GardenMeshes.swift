import RealityKit
import simd

// Procedural low-poly meshes for the garden (docs/design/garden-3d.md 3: about 80 triangles a sphere, 6-sided
// cylinders). They are built with MeshDescriptor (iOS 15), so nothing here needs iOS 18's generateCylinder or
// generateCone. The unit meshes are built once and shared by every plant; each plant scales its entities.

struct MeshBuilder {
  private(set) var positions: [SIMD3<Float>] = []
  private(set) var normals: [SIMD3<Float>] = []
  private(set) var indices: [UInt32] = []

  var triangleCount: Int { indices.count / 3 }

  mutating func vertex(_ p: SIMD3<Float>, _ n: SIMD3<Float>) -> UInt32 {
    positions.append(p)
    normals.append(n)
    return UInt32(positions.count - 1)
  }

  /// A triangle of existing vertices, wound counter-clockwise as seen from `facing` (RealityKit's front face).
  mutating func triangle(_ i0: UInt32, _ i1: UInt32, _ i2: UInt32, facing: SIMD3<Float>) {
    let a = positions[Int(i0)], b = positions[Int(i1)], c = positions[Int(i2)]
    if simd_dot(simd_cross(b - a, c - a), facing) >= 0 {
      indices += [i0, i1, i2]
    } else {
      indices += [i0, i2, i1]
    }
  }

  /// A flat-shaded triangle with its own vertices.
  mutating func flat(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ c: SIMD3<Float>, facing: SIMD3<Float>) {
    var n = simd_cross(b - a, c - a)
    if simd_length(n) < 1e-9 { return }
    n = simd_normalize(n)
    if simd_dot(n, facing) < 0 { n = -n }
    let i0 = vertex(a, n), i1 = vertex(b, n), i2 = vertex(c, n)
    triangle(i0, i1, i2, facing: n)
  }

  mutating func flatQuad(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ c: SIMD3<Float>, _ d: SIMD3<Float>, facing: SIMD3<Float>) {
    flat(a, b, c, facing: facing)
    flat(a, c, d, facing: facing)
  }

  @MainActor
  func resource(_ name: String) -> MeshResource {
    var desc = MeshDescriptor(name: name)
    desc.positions = MeshBuffers.Positions(positions)
    desc.normals = MeshBuffers.Normals(normals)
    desc.primitives = .triangles(indices)
    if let mesh = try? MeshResource.generate(from: [desc]) { return mesh }
    return MeshResource.generateBox(size: 0.001)
  }
}

@MainActor
enum GardenMeshes {
  /// Stem: radius 1, y 0...1, 6 sides with a top cap.
  static let stem: MeshResource = {
    var b = MeshBuilder()
    addPrism(&b, sides: 6, bottomRadius: 1, topRadius: 1, height: 1, topCap: true)
    return b.resource("stem")
  }()

  /// Canopy blob: a unit sphere, 8 segments by 6 rings (80 triangles), smooth.
  static let blob: MeshResource = {
    var b = MeshBuilder()
    addSphere(&b, segments: 8, rings: 6, hemisphere: false)
    return b.resource("blob")
  }()

  /// Status ring: an annulus from 0.78 to 1 on the ground, facing up.
  static let ring: MeshResource = {
    var b = MeshBuilder()
    let segments = 28, inner: Float = 0.78
    let up = SIMD3<Float>(0, 1, 0)
    for i in 0 ..< segments {
      let a0 = Float(i) / Float(segments) * 2 * .pi
      let a1 = Float(i + 1) / Float(segments) * 2 * .pi
      let o0 = SIMD3(cos(a0), 0, sin(a0)), o1 = SIMD3(cos(a1), 0, sin(a1))
      b.flatQuad(o0 * inner, o0, o1, o1 * inner, facing: up)
    }
    return b.resource("ring")
  }()

  /// Pot: a frustum from radius 0.72 at y 0 to 1 at y 1, 10 sides, open at the top.
  static let pot: MeshResource = {
    var b = MeshBuilder()
    addPrism(&b, sides: 10, bottomRadius: 0.72, topRadius: 1, height: 1, topCap: false)
    return b.resource("pot")
  }()

  /// Soil in a pot: a disc of radius 1 at y 0, facing up.
  static let disc: MeshResource = {
    var b = MeshBuilder()
    addDisc(&b, sides: 12, radius: 1, y: 0)
    return b.resource("disc")
  }()

  /// Frost cover: a unit hemisphere (y >= 0), 10 segments by 4 rings.
  static let dome: MeshResource = {
    var b = MeshBuilder()
    addSphere(&b, segments: 10, rings: 4, hemisphere: true)
    return b.resource("dome")
  }()

  /// Triangles in one plant's shared meshes, for the budget (stem + two blobs + ring, plus pot, disc and cover).
  static let plantTriangles: Int = {
    var s = MeshBuilder(); addPrism(&s, sides: 6, bottomRadius: 1, topRadius: 1, height: 1, topCap: true)
    var bl = MeshBuilder(); addSphere(&bl, segments: 8, rings: 6, hemisphere: false)
    var d = MeshBuilder(); addSphere(&d, segments: 10, rings: 4, hemisphere: true)
    return s.triangleCount + 2 * bl.triangleCount + 56 + 20 + 12 + d.triangleCount
  }()

  // MARK: Builders

  static func addPrism(_ b: inout MeshBuilder, sides: Int, bottomRadius: Float, topRadius: Float, height: Float, topCap: Bool) {
    for i in 0 ..< sides {
      let a0 = Float(i) / Float(sides) * 2 * .pi
      let a1 = Float(i + 1) / Float(sides) * 2 * .pi
      let mid = (a0 + a1) / 2
      let out = SIMD3<Float>(cos(mid), 0, sin(mid))
      let p0 = SIMD3(cos(a0) * bottomRadius, 0, sin(a0) * bottomRadius)
      let p1 = SIMD3(cos(a1) * bottomRadius, 0, sin(a1) * bottomRadius)
      let p2 = SIMD3(cos(a1) * topRadius, height, sin(a1) * topRadius)
      let p3 = SIMD3(cos(a0) * topRadius, height, sin(a0) * topRadius)
      b.flatQuad(p0, p1, p2, p3, facing: out)
    }
    if topCap { addDisc(&b, sides: sides, radius: topRadius, y: height) }
  }

  static func addDisc(_ b: inout MeshBuilder, sides: Int, radius: Float, y: Float) {
    let up = SIMD3<Float>(0, 1, 0)
    let centre = SIMD3<Float>(0, y, 0)
    for i in 0 ..< sides {
      let a0 = Float(i) / Float(sides) * 2 * .pi
      let a1 = Float(i + 1) / Float(sides) * 2 * .pi
      b.flat(centre, SIMD3(cos(a0) * radius, y, sin(a0) * radius), SIMD3(cos(a1) * radius, y, sin(a1) * radius), facing: up)
    }
  }

  static func addSphere(_ b: inout MeshBuilder, segments: Int, rings: Int, hemisphere: Bool) {
    let maxPolar: Float = hemisphere ? .pi / 2 : .pi
    var grid: [[UInt32]] = []
    for r in 0 ... rings {
      let polar = Float(r) / Float(rings) * maxPolar
      var row: [UInt32] = []
      for s in 0 ... segments {
        let az = Float(s) / Float(segments) * 2 * .pi
        let n = SIMD3<Float>(sin(polar) * cos(az), cos(polar), sin(polar) * sin(az))
        row.append(b.vertex(n, n))
      }
      grid.append(row)
    }
    for r in 0 ..< rings {
      for s in 0 ..< segments {
        let a = grid[r][s], c = grid[r][s + 1], d = grid[r + 1][s], e = grid[r + 1][s + 1]
        let polar = (Float(r) + 0.5) / Float(rings) * maxPolar
        let az = (Float(s) + 0.5) / Float(segments) * 2 * .pi
        let facing = SIMD3<Float>(sin(polar) * cos(az), cos(polar), sin(polar) * sin(az))
        // The pole rows collapse one triangle of each quad.
        if r != 0 { b.triangle(a, c, e, facing: facing) }
        if r != rings - 1 || hemisphere { b.triangle(a, e, d, facing: facing) }
      }
    }
  }

  /// A bed: the polygon extruded to `height` with plaster walls and top, plus a soil cap inset by `inset`.
  /// Returns nil for a polygon it can't triangulate.
  static func bed(polygon: [SIMD2<Float>], height: Float, inset: Float) -> (plaster: MeshResource, soil: MeshResource)? {
    var pts = polygon
    if pts.count >= 2, simd_distance(pts[0], pts[pts.count - 1]) < 1e-5 { pts.removeLast() }
    guard pts.count >= 3 else { return nil }
    // Counter-clockwise in (x, y), so the outward normal of an edge (dx, dy) is (dy, -dx).
    if signedArea(pts) < 0 { pts.reverse() }
    guard let tris = earClip(pts) else { return nil }

    var plaster = MeshBuilder()
    let up = SIMD3<Float>(0, 1, 0)
    for i in 0 ..< pts.count {
      let p = pts[i], q = pts[(i + 1) % pts.count]
      let d = q - p
      let out = SIMD3<Float>(d.y, 0, -d.x)
      plaster.flatQuad(SIMD3(p.x, 0, p.y), SIMD3(q.x, 0, q.y), SIMD3(q.x, height, q.y), SIMD3(p.x, height, p.y), facing: out)
    }
    for t in tris {
      plaster.flat(SIMD3(pts[t.0].x, height, pts[t.0].y), SIMD3(pts[t.1].x, height, pts[t.1].y), SIMD3(pts[t.2].x, height, pts[t.2].y), facing: up)
    }

    // Soil: each corner moved inward along its bisector, keeping the same triangulation.
    let inner = insetPolygon(pts, by: inset)
    var soil = MeshBuilder()
    let y = height + 0.004
    for t in tris {
      soil.flat(SIMD3(inner[t.0].x, y, inner[t.0].y), SIMD3(inner[t.1].x, y, inner[t.1].y), SIMD3(inner[t.2].x, y, inner[t.2].y), facing: up)
    }
    return (plaster.resource("bed"), soil.resource("bed-soil"))
  }

  static func signedArea(_ p: [SIMD2<Float>]) -> Float {
    var a: Float = 0
    for i in 0 ..< p.count {
      let q = p[(i + 1) % p.count]
      a += p[i].x * q.y - q.x * p[i].y
    }
    return a / 2
  }

  /// Ear clipping for a simple counter-clockwise polygon.
  static func earClip(_ p: [SIMD2<Float>]) -> [(Int, Int, Int)]? {
    var idx = Array(0 ..< p.count)
    var out: [(Int, Int, Int)] = []
    var guardCount = 0
    while idx.count > 3 {
      guardCount += 1
      if guardCount > 10_000 { return nil }
      var clipped = false
      for k in 0 ..< idx.count {
        let i0 = idx[(k + idx.count - 1) % idx.count], i1 = idx[k], i2 = idx[(k + 1) % idx.count]
        let a = p[i0], b = p[i1], c = p[i2]
        let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
        if cross <= 1e-9 { continue }
        let inside = idx.contains { j in
          j != i0 && j != i1 && j != i2 && pointInTriangle(p[j], a, b, c)
        }
        if inside { continue }
        out.append((i0, i1, i2))
        idx.remove(at: k)
        clipped = true
        break
      }
      if !clipped { return nil }
    }
    out.append((idx[0], idx[1], idx[2]))
    return out
  }

  private static func pointInTriangle(_ p: SIMD2<Float>, _ a: SIMD2<Float>, _ b: SIMD2<Float>, _ c: SIMD2<Float>) -> Bool {
    func side(_ u: SIMD2<Float>, _ v: SIMD2<Float>, _ w: SIMD2<Float>) -> Float {
      (v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x)
    }
    return side(a, b, p) >= 0 && side(b, c, p) >= 0 && side(c, a, p) >= 0
  }

  static func insetPolygon(_ p: [SIMD2<Float>], by inset: Float) -> [SIMD2<Float>] {
    let n = p.count
    func outward(_ i: Int) -> SIMD2<Float> {
      let d = p[(i + 1) % n] - p[i]
      let len = max(simd_length(d), 1e-6)
      return SIMD2(d.y, -d.x) / len
    }
    return (0 ..< n).map { i in
      let n1 = outward((i + n - 1) % n), n2 = outward(i)
      var m = n1 + n2
      if simd_length(m) < 1e-6 { m = n2 } else { m = simd_normalize(m) }
      let k = max(simd_dot(m, n2), 0.3)
      return p[i] - m * (inset / k)
    }
  }
}
