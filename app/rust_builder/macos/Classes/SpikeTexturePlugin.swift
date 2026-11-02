import CoreVideo
import QuartzCore

#if os(iOS)
  import Flutter
  import UIKit
#else
  import FlutterMacOS
#endif

// T-0005 spike: feeds a Flutter texture from the Rust renderer. See docs in the PR for the paths.
@_silgen_name("shorted_spike_create")
private func spikeCreate(_ width: UInt32, _ height: UInt32) -> OpaquePointer?
@_silgen_name("shorted_spike_destroy")
private func spikeDestroy(_ session: OpaquePointer)
@_silgen_name("shorted_spike_attach_surface")
private func spikeAttachSurface(_ session: OpaquePointer, _ surface: UnsafeRawPointer) -> Int32
@_silgen_name("shorted_spike_open_video")
private func spikeOpenVideo(
  _ session: OpaquePointer, _ path: UnsafePointer<UInt8>, _ length: Int
) -> Bool
@_silgen_name("shorted_spike_render_surface")
private func spikeRenderSurface(_ session: OpaquePointer, _ index: UInt32, _ time: Float) -> Bool
@_silgen_name("shorted_spike_render_bytes")
private func spikeRenderBytes(
  _ session: OpaquePointer, _ time: Float, _ destination: UnsafeMutableRawPointer, _ length: Int,
  _ stride: Int
) -> Bool

private let bufferCount = 3
private let frameInterval = DispatchTimeInterval.microseconds(16_667)

private final class SpikeSession: NSObject, FlutterTexture {
  private let session: OpaquePointer
  private let zeroCopy: Bool
  private let buffers: [CVPixelBuffer]
  private let queue = DispatchQueue(label: "shorted.spike.render", qos: .userInteractive)
  private let lock = NSLock()
  private var timer: DispatchSourceTimer?
  private var latest: Int?
  private var next = 0
  private var busy = false
  private var rendered = 0
  private var skipped = 0
  private var failed = 0
  private var presented = 0
  private var renderMs: [Double] = []
  private let startedAt = CACurrentMediaTime()
  var onFrame: (() -> Void)?

  init?(width: Int, height: Int, zeroCopy: Bool, clip: String?) {
    guard let session = spikeCreate(UInt32(width), UInt32(height)) else { return nil }
    self.session = session
    self.zeroCopy = zeroCopy
    let attributes: [CFString: Any] = [
      kCVPixelBufferMetalCompatibilityKey: true,
      kCVPixelBufferIOSurfacePropertiesKey: [String: Any](),
    ]
    var buffers: [CVPixelBuffer] = []
    for _ in 0..<bufferCount {
      var buffer: CVPixelBuffer?
      CVPixelBufferCreate(
        nil, width, height, kCVPixelFormatType_32BGRA, attributes as CFDictionary, &buffer)
      guard let buffer, let surface = CVPixelBufferGetIOSurface(buffer) else {
        spikeDestroy(session)
        return nil
      }
      if zeroCopy {
        let raw = UnsafeRawPointer(Unmanaged.passUnretained(surface.takeUnretainedValue()).toOpaque())
        guard spikeAttachSurface(session, raw) >= 0 else {
          spikeDestroy(session)
          return nil
        }
      }
      buffers.append(buffer)
    }
    self.buffers = buffers
    if let clip {
      let bytes = Array(clip.utf8)
      guard spikeOpenVideo(session, bytes, bytes.count) else { return nil }
    }
  }

  func start() {
    let timer = DispatchSource.makeTimerSource(queue: queue)
    timer.schedule(deadline: .now(), repeating: frameInterval, leeway: .milliseconds(1))
    timer.setEventHandler { [weak self] in self?.renderFrame() }
    timer.resume()
    self.timer = timer
  }

  private func renderFrame() {
    lock.lock()
    if busy {
      skipped += 1
      lock.unlock()
      return
    }
    busy = true
    let index = next
    lock.unlock()

    let time = Float(CACurrentMediaTime() - startedAt)
    let begin = CACurrentMediaTime()
    let ok: Bool
    if zeroCopy {
      ok = spikeRenderSurface(session, UInt32(index), time)
    } else {
      let buffer = buffers[index]
      CVPixelBufferLockBaseAddress(buffer, [])
      let stride = CVPixelBufferGetBytesPerRow(buffer)
      ok = spikeRenderBytes(
        session, time, CVPixelBufferGetBaseAddress(buffer)!, stride * CVPixelBufferGetHeight(buffer),
        stride)
      CVPixelBufferUnlockBaseAddress(buffer, [])
    }
    let elapsed = (CACurrentMediaTime() - begin) * 1000

    lock.lock()
    busy = false
    if ok {
      latest = index
      next = (index + 1) % bufferCount
      rendered += 1
      renderMs.append(elapsed)
    } else {
      failed += 1
    }
    lock.unlock()
    if ok { onFrame?() }
  }

  func copyPixelBuffer() -> Unmanaged<CVPixelBuffer>? {
    lock.lock()
    defer { lock.unlock() }
    guard let latest else { return nil }
    presented += 1
    return Unmanaged.passRetained(buffers[latest])
  }

  /// Counters since the previous call, for one measurement window.
  func takeStats() -> [String: Any] {
    lock.lock()
    defer { lock.unlock() }
    let stats: [String: Any] = [
      "rendered": rendered, "skipped": skipped, "failed": failed, "presented": presented,
      "renderMs": renderMs,
    ]
    rendered = 0
    skipped = 0
    failed = 0
    presented = 0
    renderMs = []
    return stats
  }

  func stop() {
    timer?.cancel()
    timer = nil
    queue.sync {}
    spikeDestroy(session)
  }
}

public final class SpikeTexturePlugin: NSObject, FlutterPlugin {
  private static var preview: PreviewChannel?
  private let textures: FlutterTextureRegistry
  private var active: (id: Int64, session: SpikeSession)?

  init(textures: FlutterTextureRegistry) {
    self.textures = textures
  }

  public static func register(with registrar: FlutterPluginRegistrar) {
    #if os(iOS)
      let messenger = registrar.messenger()
      let textures = registrar.textures()
    #else
      let messenger = registrar.messenger
      let textures = registrar.textures
    #endif
    preview = PreviewChannel(messenger: messenger, textures: textures)
    let channel = FlutterMethodChannel(name: "shorted/spike", binaryMessenger: messenger)
    registrar.addMethodCallDelegate(SpikeTexturePlugin(textures: textures), channel: channel)
  }

  public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "start":
      stopActive()
      let arguments = call.arguments as? [String: Any] ?? [:]
      guard
        let width = arguments["width"] as? Int, let height = arguments["height"] as? Int,
        let zeroCopy = arguments["zeroCopy"] as? Bool,
        let session = SpikeSession(
          width: width, height: height, zeroCopy: zeroCopy, clip: arguments["clip"] as? String)
      else {
        result(FlutterError(code: "unavailable", message: nil, details: nil))
        return
      }
      let id = textures.register(session)
      session.onFrame = { [textures] in textures.textureFrameAvailable(id) }
      active = (id, session)
      session.start()
      #if os(iOS)
        // The measurement runs untouched: the phone must not lock during it.
        UIApplication.shared.isIdleTimerDisabled = true
      #endif
      result(id)
    case "stats":
      result(active?.session.takeStats())
    case "stop":
      stopActive()
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private func stopActive() {
    guard let active else { return }
    textures.unregisterTexture(active.id)
    active.session.stop()
    self.active = nil
    #if os(iOS)
      UIApplication.shared.isIdleTimerDisabled = false
    #endif
  }
}
