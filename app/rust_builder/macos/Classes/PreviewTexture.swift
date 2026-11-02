import CoreVideo

#if os(iOS)
  import Flutter
#else
  import FlutterMacOS
#endif

@_silgen_name("shorted_preview_create")
private func previewCreate(_ width: UInt32, _ height: UInt32) -> OpaquePointer?
@_silgen_name("shorted_preview_destroy")
private func previewDestroy(_ session: OpaquePointer)
@_silgen_name("shorted_preview_attach_surface")
private func previewAttachSurface(_ session: OpaquePointer, _ surface: UnsafeRawPointer) -> Bool
@_silgen_name("shorted_preview_render")
private func previewRender(_ session: OpaquePointer, _ index: UInt32) -> Bool

private let surfaceCount = 3
private let tick = DispatchTimeInterval.microseconds(16_667)

/// Shows engine frames in a Flutter texture. Surfaces take turns, so Flutter never reads the one
/// being drawn. All engine calls stay on `queue`.
private final class PreviewTexture: NSObject, FlutterTexture {
  private let session: OpaquePointer
  private let buffers: [CVPixelBuffer]
  private let queue = DispatchQueue(label: "shorted.preview", qos: .userInteractive)
  private let lock = NSLock()
  private var timer: DispatchSourceTimer?
  private var latest: Int?
  private var next = 0
  var onFrame: (() -> Void)?

  init?(width: Int, height: Int) {
    guard let session = previewCreate(UInt32(width), UInt32(height)) else { return nil }
    self.session = session
    let attributes: [CFString: Any] = [
      kCVPixelBufferMetalCompatibilityKey: true,
      kCVPixelBufferIOSurfacePropertiesKey: [String: Any](),
    ]
    var buffers: [CVPixelBuffer] = []
    for _ in 0..<surfaceCount {
      var buffer: CVPixelBuffer?
      CVPixelBufferCreate(
        nil, width, height, kCVPixelFormatType_32BGRA, attributes as CFDictionary, &buffer)
      guard let buffer, let surface = CVPixelBufferGetIOSurface(buffer) else {
        previewDestroy(session)
        return nil
      }
      let raw = UnsafeRawPointer(Unmanaged.passUnretained(surface.takeUnretainedValue()).toOpaque())
      guard previewAttachSurface(session, raw) else {
        previewDestroy(session)
        return nil
      }
      buffers.append(buffer)
    }
    self.buffers = buffers
  }

  func start() {
    let timer = DispatchSource.makeTimerSource(queue: queue)
    timer.schedule(deadline: .now(), repeating: tick, leeway: .milliseconds(1))
    timer.setEventHandler { [weak self] in self?.renderFrame() }
    timer.resume()
    self.timer = timer
  }

  private func renderFrame() {
    let index = next
    guard previewRender(session, UInt32(index)) else { return }
    lock.lock()
    latest = index
    lock.unlock()
    next = (index + 1) % surfaceCount
    onFrame?()
  }

  func copyPixelBuffer() -> Unmanaged<CVPixelBuffer>? {
    lock.lock()
    defer { lock.unlock() }
    guard let latest else { return nil }
    return Unmanaged.passRetained(buffers[latest])
  }

  func stop() {
    timer?.cancel()
    timer = nil
    queue.sync {}
    previewDestroy(session)
  }
}

final class PreviewChannel: NSObject {
  private let textures: FlutterTextureRegistry
  private var active: (id: Int64, texture: PreviewTexture)?
  private let channel: FlutterMethodChannel

  init(messenger: FlutterBinaryMessenger, textures: FlutterTextureRegistry) {
    self.textures = textures
    channel = FlutterMethodChannel(name: "shorted/preview", binaryMessenger: messenger)
    super.init()
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call, result)
    }
  }

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    switch call.method {
    case "start":
      stop()
      let arguments = call.arguments as? [String: Any] ?? [:]
      guard
        let width = arguments["width"] as? Int, let height = arguments["height"] as? Int,
        let texture = PreviewTexture(width: width, height: height)
      else {
        result(FlutterError(code: "unavailable", message: nil, details: nil))
        return
      }
      let id = textures.register(texture)
      texture.onFrame = { [textures] in textures.textureFrameAvailable(id) }
      active = (id, texture)
      texture.start()
      result(id)
    case "stop":
      stop()
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private func stop() {
    guard let active else { return }
    textures.unregisterTexture(active.id)
    active.texture.stop()
    self.active = nil
  }
}
