import AVFoundation
import CoreMotion
import ExpoModulesCore
import ImageIO
import UIKit

// The camera on a LiDAR iPhone (architecture 6.7; docs/frontend/README.md 13). It replaces expo-camera there, because
// only one capture session can use the camera: the preview, the distance hint from streamed depth, and each photo
// with its depth map registered to it. The photo stays in the sensor frame with its EXIF orientation, so the photo
// and its depth line up, and the app uploads the file unchanged.

final class LidarCameraView: ExpoView, AVCaptureDepthDataOutputDelegate {
  let onDistance = EventDispatcher()
  let onReady = EventDispatcher()
  let onMountError = EventDispatcher()

  private let session = AVCaptureSession()
  private let sessionQueue = DispatchQueue(label: "org.buildnorth.pdm.lidar.session")
  private let depthQueue = DispatchQueue(label: "org.buildnorth.pdm.lidar.depth")
  private let photoOutput = AVCapturePhotoOutput()
  private let depthOutput = AVCaptureDepthDataOutput()
  private let previewLayer = AVCaptureVideoPreviewLayer()
  private let motion = CMMotionManager()
  private var device: AVCaptureDevice?
  private var configured = false
  private var active = false
  private var lastDistance: CFTimeInterval = 0
  private var latestCentre: Float?
  private var inFlight: [Int64: PhotoCapture] = [:]
  private var rotation: AnyObject?

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true
    backgroundColor = .black
    previewLayer.session = session
    previewLayer.videoGravity = .resizeAspectFill
    layer.addSublayer(previewLayer)
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    previewLayer.frame = bounds
  }

  /// False stops the session, so a sheet over capture doesn't keep the camera and the LiDAR running.
  func setActive(_ value: Bool) {
    guard value != active else { return }
    active = value
    sessionQueue.async { [weak self] in
      guard let self else { return }
      if value {
        if !self.configured { self.configure() }
        guard self.configured else { return }
        if !self.session.isRunning { self.session.startRunning() }
        DispatchQueue.main.async {
          if self.motion.isDeviceMotionAvailable, !self.motion.isDeviceMotionActive {
            self.motion.deviceMotionUpdateInterval = 1.0 / 30
            self.motion.startDeviceMotionUpdates()
          }
          self.onReady()
        }
      } else {
        if self.session.isRunning { self.session.stopRunning() }
        DispatchQueue.main.async { self.motion.stopDeviceMotionUpdates() }
      }
    }
  }

  override func removeFromSuperview() {
    sessionQueue.async { [session] in if session.isRunning { session.stopRunning() } }
    motion.stopDeviceMotionUpdates()
    super.removeFromSuperview()
  }

  // MARK: Configuration

  /// The LiDAR depth camera with the largest photos that still deliver depth, and its largest depth format.
  private func configure() {
    guard let device = AVCaptureDevice.default(.builtInLiDARDepthCamera, for: .video, position: .back) else {
      DispatchQueue.main.async { self.onMountError(["message": "This iPhone has no LiDAR camera."]) }
      return
    }
    let formats = device.formats.filter { !$0.supportedDepthDataFormats.isEmpty && !$0.isVideoBinned }
    guard let format = formats.max(by: { Self.photoArea($0) < Self.photoArea($1) }),
      let depthFormat = format.supportedDepthDataFormats
        .filter({ Self.isDepth($0.formatDescription.mediaSubType.rawValue) })
        .max(by: { $0.formatDescription.dimensions.width < $1.formatDescription.dimensions.width })
    else {
      DispatchQueue.main.async { self.onMountError(["message": "The LiDAR camera has no depth format."]) }
      return
    }

    session.beginConfiguration()
    defer { session.commitConfiguration() }
    session.sessionPreset = .inputPriority
    do {
      let input = try AVCaptureDeviceInput(device: device)
      guard session.canAddInput(input), session.canAddOutput(photoOutput), session.canAddOutput(depthOutput) else {
        throw NSError(domain: "lidar", code: 1, userInfo: [NSLocalizedDescriptionKey: "The camera couldn't start."])
      }
      session.addInput(input)
      session.addOutput(photoOutput)
      session.addOutput(depthOutput)
      try device.lockForConfiguration()
      device.activeFormat = format
      device.activeDepthDataFormat = depthFormat
      device.unlockForConfiguration()
    } catch {
      DispatchQueue.main.async { self.onMountError(["message": error.localizedDescription]) }
      return
    }

    photoOutput.isDepthDataDeliveryEnabled = photoOutput.isDepthDataDeliverySupported
    if let largest = format.supportedMaxPhotoDimensions.max(by: { $0.width * $0.height < $1.width * $1.height }) {
      photoOutput.maxPhotoDimensions = largest
    }
    photoOutput.maxPhotoQualityPrioritization = .quality
    // Smoothed depth for the hint only; the photo's depth is unfiltered.
    depthOutput.isFilteringEnabled = true
    depthOutput.alwaysDiscardsLateDepthData = true
    depthOutput.setDelegate(self, callbackQueue: depthQueue)

    if #available(iOS 17.0, *) {
      rotation = AVCaptureDevice.RotationCoordinator(device: device, previewLayer: previewLayer)
    }
    self.device = device
    configured = true
  }

  private static func photoArea(_ f: AVCaptureDevice.Format) -> Int32 {
    f.supportedMaxPhotoDimensions.map { $0.width * $0.height }.max() ?? 0
  }

  private static func isDepth(_ subtype: FourCharCode) -> Bool {
    subtype == kCVPixelFormatType_DepthFloat32 || subtype == kCVPixelFormatType_DepthFloat16
  }

  // MARK: Distance

  func depthDataOutput(
    _ output: AVCaptureDepthDataOutput,
    didOutput depthData: AVDepthData,
    timestamp: CMTime,
    connection: AVCaptureConnection
  ) {
    // Five a second is enough for a chip, and keeps the bridge quiet.
    let now = CACurrentMediaTime()
    guard now - lastDistance >= 0.2 else { return }
    lastDistance = now
    let centre = DepthGrid(depthData).centreMedian()
    latestCentre = centre
    DispatchQueue.main.async {
      self.onDistance(["distance_m": centre.map { Double($0) } as Any])
    }
  }

  // MARK: Photo

  /// One photo with its depth. Resolves with the files' URIs, the `DepthMeta` and a report for the day-1 check.
  func capture(promise: Promise) {
    sessionQueue.async { [weak self] in
      guard let self else { return }
      guard self.configured, self.session.isRunning else {
        promise.reject("ERR_LIDAR_NOT_READY", "The camera isn't ready.")
        return
      }
      let codec: [String: Any] = self.photoOutput.availablePhotoCodecTypes.contains(.jpeg) ? [AVVideoCodecKey: AVVideoCodecType.jpeg] : [:]
      let settings = AVCapturePhotoSettings(format: codec)
      settings.maxPhotoDimensions = self.photoOutput.maxPhotoDimensions
      settings.photoQualityPrioritization = .quality
      if self.photoOutput.isDepthDataDeliveryEnabled {
        settings.isDepthDataDeliveryEnabled = true
        // Missing depth stays missing (architecture 6.7), and the depth goes up as its own part.
        settings.isDepthDataFiltered = false
        settings.embedsDepthDataInPhoto = false
      }
      if #available(iOS 17.0, *), let coordinator = self.rotation as? AVCaptureDevice.RotationCoordinator,
        let connection = self.photoOutput.connection(with: .video)
      {
        // Sets only the EXIF orientation: the photo's pixels stay in the sensor frame, like its depth.
        let angle = coordinator.videoRotationAngleForHorizonLevelCapture
        if connection.isVideoRotationAngleSupported(angle) { connection.videoRotationAngle = angle }
      }
      let gravity = self.motion.deviceMotion.flatMap { DepthMeta.cameraGravity($0.gravity) }
      let delegate = PhotoCapture(gravity: gravity, centre: self.latestCentre, promise: promise) { [weak self] id in
        self?.sessionQueue.async { self?.inFlight[id] = nil }
      }
      self.inFlight[settings.uniqueID] = delegate
      self.photoOutput.capturePhoto(with: settings, delegate: delegate)
    }
  }
}

/// One photo's delegate: writes the JPEG and the depth PNG to the cache and resolves the promise.
private final class PhotoCapture: NSObject, AVCapturePhotoCaptureDelegate {
  private let gravity: [Double]?
  private let centre: Float?
  private let promise: Promise
  private let done: (Int64) -> Void

  init(gravity: [Double]?, centre: Float?, promise: Promise, done: @escaping (Int64) -> Void) {
    self.gravity = gravity
    self.centre = centre
    self.promise = promise
    self.done = done
  }

  func photoOutput(_ output: AVCapturePhotoOutput, didFinishProcessingPhoto photo: AVCapturePhoto, error: Error?) {
    defer { done(photo.resolvedSettings.uniqueID) }
    if let error {
      promise.reject("ERR_LIDAR_CAPTURE", error.localizedDescription)
      return
    }
    guard let jpeg = photo.fileDataRepresentation() else {
      promise.reject("ERR_LIDAR_CAPTURE", "The photo couldn't be encoded.")
      return
    }
    do {
      let dir = try Self.directory()
      let id = UUID().uuidString
      let photoURL = dir.appendingPathComponent("\(id).jpg")
      try jpeg.write(to: photoURL)

      let props = CGImageSourceCreateWithData(jpeg as CFData, nil)
        .flatMap { CGImageSourceCopyPropertiesAtIndex($0, 0, nil) as? [CFString: Any] } ?? [:]
      let pixelWidth = props[kCGImagePropertyPixelWidth] as? Int ?? 0
      let pixelHeight = props[kCGImagePropertyPixelHeight] as? Int ?? 0
      let orientation = props[kCGImagePropertyOrientation] as? Int ?? 1
      // The size as shown: orientations 5 to 8 turn the photo a quarter.
      let turned = orientation >= 5
      var result: [String: Any] = [
        "file": [
          "uri": photoURL.absoluteString,
          "mimeType": "image/jpeg",
          "width": turned ? pixelHeight : pixelWidth,
          "height": turned ? pixelWidth : pixelHeight,
        ]
      ]
      var report: [String: Any] = [
        "photo": ["width": pixelWidth, "height": pixelHeight, "orientation": orientation],
        "centre_distance_m": centre.map { Double($0) } as Any,
        "gravity": gravity as Any,
      ]

      if let depthData = photo.depthData {
        let grid = DepthGrid(depthData)
        let absolute = depthData.depthDataAccuracy == .absolute
        report["depth"] = [
          "width": grid.width,
          "height": grid.height,
          "valid_fraction": grid.validFraction,
          "accuracy": absolute ? "absolute" : "relative",
          "filtered": depthData.isDepthDataFiltered,
          "has_distortion_table": depthData.cameraCalibrationData?.inverseLensDistortionLookupTable != nil,
          "centre_distance_m": grid.centreMedian().map { Double($0) } as Any,
        ]
        // Depth only with absolute accuracy (architecture 6.7).
        if absolute, let png = grid.png() {
          let depthURL = dir.appendingPathComponent("\(id)-depth.png")
          try png.write(to: depthURL)
          result["depth"] = ["depthUri": depthURL.absoluteString, "meta": DepthMeta.record(depthData, gravity: gravity)]
        } else {
          report["dropped"] = absolute ? "encoding" : "relative_accuracy"
        }
      } else {
        report["dropped"] = "no_depth"
      }
      result["report"] = report
      promise.resolve(result)
    } catch {
      promise.reject("ERR_LIDAR_CAPTURE", error.localizedDescription)
    }
  }

  private static func directory() throws -> URL {
    let dir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("lidar", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
  }
}
