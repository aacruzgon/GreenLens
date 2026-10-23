import AVFoundation
import CoreMotion
import ImageIO
import UniformTypeIdentifiers

// The depth parts of an upload (contracts 3, POST /scans/{id}/images): the 16-bit PNG in millimetres, 0 for no depth,
// in the sensor frame, and the DepthMeta record. Also the median distance at the centre of a depth map, for the
// distance hint (architecture 6.7).

/// A depth map as metres per pixel, row-major, NaN where there is no depth.
struct DepthGrid {
  let width: Int
  let height: Int
  let metres: [Float]

  /// Converts disparity to depth when the camera sent disparity (architecture 6.7).
  init(_ data: AVDepthData) {
    let depth = data.depthDataType == kCVPixelFormatType_DepthFloat32
      ? data
      : data.converting(toDepthDataType: kCVPixelFormatType_DepthFloat32)
    let map = depth.depthDataMap
    CVPixelBufferLockBaseAddress(map, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(map, .readOnly) }
    width = CVPixelBufferGetWidth(map)
    height = CVPixelBufferGetHeight(map)
    let rowBytes = CVPixelBufferGetBytesPerRow(map)
    var values = [Float](repeating: .nan, count: width * height)
    if let base = CVPixelBufferGetBaseAddress(map) {
      for y in 0..<height {
        let row = base.advanced(by: y * rowBytes).assumingMemoryBound(to: Float32.self)
        for x in 0..<width {
          let v = row[x]
          values[y * width + x] = v.isFinite && v > 0 ? v : .nan
        }
      }
    }
    metres = values
  }

  /// The share of pixels with depth: the API drops a map under 20% (contracts 3, `too_sparse`).
  var validFraction: Double {
    guard !metres.isEmpty else { return 0 }
    return Double(metres.lazy.filter { !$0.isNaN }.count) / Double(metres.count)
  }

  /// The median depth of the centre of the frame, a square a tenth of the short side across. Nil when none of it
  /// has depth.
  func centreMedian() -> Float? {
    median(atX: 0.5, y: 0.5, halfSide: max(2, min(width, height) / 20))
  }

  /// The median depth in a square around a point in normalised sensor-frame coordinates.
  func median(atX nx: Double, y ny: Double, halfSide: Int) -> Float? {
    let cx = Int((nx * Double(width)).rounded(.down))
    let cy = Int((ny * Double(height)).rounded(.down))
    var values: [Float] = []
    for y in max(0, cy - halfSide)...min(height - 1, cy + halfSide) {
      for x in max(0, cx - halfSide)...min(width - 1, cx + halfSide) {
        let v = metres[y * width + x]
        if !v.isNaN { values.append(v) }
      }
    }
    guard !values.isEmpty else { return nil }
    values.sort()
    // Nearest rank, as the measurement stage does (architecture 6.7).
    return values[(values.count - 1) / 2]
  }

  /// 16-bit grayscale PNG of millimetres, 0 for no depth.
  func png() -> Data? {
    var millimetres = [UInt16](repeating: 0, count: width * height)
    for i in 0..<metres.count {
      let v = metres[i]
      if !v.isNaN { millimetres[i] = UInt16(min(65535, max(1, (v * 1000).rounded()))) }
    }
    let bytes = millimetres.withUnsafeBufferPointer { Data(buffer: $0) }
    guard
      let provider = CGDataProvider(data: bytes as CFData),
      let image = CGImage(
        width: width,
        height: height,
        bitsPerComponent: 16,
        bitsPerPixel: 16,
        bytesPerRow: width * 2,
        space: CGColorSpaceCreateDeviceGray(),
        bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue | CGBitmapInfo.byteOrder16Little.rawValue),
        provider: provider,
        decode: nil,
        shouldInterpolate: false,
        intent: .defaultIntent
      )
    else { return nil }
    let out = NSMutableData()
    guard let dest = CGImageDestinationCreateWithData(out, UTType.png.identifier as CFString, 1, nil) else { return nil }
    CGImageDestinationAddImage(dest, image, nil)
    guard CGImageDestinationFinalize(dest) else { return nil }
    return out as Data
  }

  /// Reads a depth PNG this module wrote back into metres, for the day-1 check's ruler.
  init?(png url: URL) {
    guard
      let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
      image.bitsPerComponent == 16
    else { return nil }
    width = image.width
    height = image.height
    var millimetres = [UInt16](repeating: 0, count: width * height)
    let drawn: Bool = millimetres.withUnsafeMutableBytes { raw in
      guard
        let ctx = CGContext(
          data: raw.baseAddress,
          width: image.width,
          height: image.height,
          bitsPerComponent: 16,
          bytesPerRow: image.width * 2,
          space: CGColorSpaceCreateDeviceGray(),
          bitmapInfo: CGImageAlphaInfo.none.rawValue | CGBitmapInfo.byteOrder16Little.rawValue
        )
      else { return false }
      ctx.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
      return true
    }
    guard drawn else { return nil }
    metres = millimetres.map { $0 == 0 ? .nan : Float($0) / 1000 }
  }
}

enum DepthMeta {
  /// `DepthMeta` (contracts 3). The distortion table is always read here; the app decides whether to send it, which
  /// the day-1 check settles (contracts open question 11).
  static func record(_ data: AVDepthData, gravity: [Double]?) -> [String: Any] {
    var meta: [String: Any] = [
      "source": "lidar",
      "device_model": deviceModel(),
      "filtered": data.isDepthDataFiltered,
    ]
    if let calibration = data.cameraCalibrationData {
      let k = calibration.intrinsicMatrix
      let ref = calibration.intrinsicMatrixReferenceDimensions
      meta["intrinsics"] = [
        "fx": Double(k.columns.0.x),
        "fy": Double(k.columns.1.y),
        "cx": Double(k.columns.2.x),
        "cy": Double(k.columns.2.y),
        "ref_width": Double(ref.width),
        "ref_height": Double(ref.height),
      ]
      if let table = calibration.inverseLensDistortionLookupTable {
        let floats = table.withUnsafeBytes { Array($0.bindMemory(to: Float32.self)) }
        meta["distortion"] = [
          "center": [Double(calibration.lensDistortionCenter.x), Double(calibration.lensDistortionCenter.y)],
          "inverse_lookup": floats.map(Double.init),
        ]
      }
    }
    if let gravity { meta["gravity"] = gravity }
    return meta
  }

  /// "iPhone18,2".
  static func deviceModel() -> String {
    var info = utsname()
    uname(&info)
    return withUnsafeBytes(of: &info.machine) { raw in
      String(decoding: raw.prefix { $0 != 0 }, as: UTF8.self)
    }
  }

  /// Gravity in the back camera's sensor frame (x right, y down, z forward), a unit vector towards the ground.
  /// Core Motion's device frame has x to the right of the screen, y to its top and z out of the screen. The back
  /// camera's sensor is landscape: its x runs to the phone's bottom edge, its y to the phone's left edge, and its z
  /// out of the back (EXIF orientation 6 when held upright). Checked by the day-1 check's spirit level.
  static func cameraGravity(_ g: CMAcceleration) -> [Double]? {
    let v = [-g.y, -g.x, -g.z]
    let norm = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).squareRoot()
    guard norm > 0.5 else { return nil }
    return v.map { $0 / norm }
  }
}
