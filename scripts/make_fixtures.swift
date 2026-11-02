// Generates licence-clear H.264 test clips (macOS). Usage: swift scripts/make_fixtures.swift <dir>
import AVFoundation
import CoreMedia
import CoreVideo

struct Fixture {
  let name: String
  let width: Int
  let height: Int
  let frames: Int
  let transform: CGAffineTransform
  /// Presentation time of a frame, in seconds.
  let time: (Int) -> Double
}

let quarterTurn = CGAffineTransform(a: 0, b: 1, c: -1, d: 0, tx: 1080, ty: 0)
func constantRate(_ index: Int) -> Double {
  return Double(index) / 60
}

// Frame gaps alternate between about 60 and 24 fps.
func variableRate(_ index: Int) -> Double {
  let pairs = Double(index / 2)
  let offset: Double = index % 2 == 0 ? 0 : 1.0 / 60
  return pairs * (1.0 / 60 + 1.0 / 24) + offset
}

let fixtures: [Fixture] = [
  Fixture(
    name: "h264_1080p60", width: 1920, height: 1080, frames: 600, transform: .identity,
    time: constantRate),
  // Portrait phone clip: landscape samples with a 90° rotation in the track metadata.
  Fixture(
    name: "h264_1080p60_rotated", width: 1920, height: 1080, frames: 120, transform: quarterTurn,
    time: constantRate),
  Fixture(
    name: "h264_1080p_vfr", width: 1920, height: 1080, frames: 120, transform: .identity,
    time: variableRate),
]

func draw(_ buffer: CVPixelBuffer, _ index: Int) {
  CVPixelBufferLockBaseAddress(buffer, [])
  defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
  let base = CVPixelBufferGetBaseAddress(buffer)!.assumingMemoryBound(to: UInt8.self)
  let stride = CVPixelBufferGetBytesPerRow(buffer)
  let width = CVPixelBufferGetWidth(buffer)
  let height = CVPixelBufferGetHeight(buffer)
  let block = (index * 8) % width
  for y in 0..<height {
    let row = base + y * stride
    for x in 0..<width {
      let pixel = row + x * 4
      let inBlock = x >= block && x < block + 160 && y > 400 && y < 680
      // Noise keeps the bitrate close to real footage.
      let seed: Int = (x &* 31) &+ (y &* 17) &+ (index &* 13)
      let mixed: Int = (seed &* 2_654_435_761) >> 24
      let noise = UInt8(truncatingIfNeeded: mixed) >> 3
      let blue = UInt8(truncatingIfNeeded: x >> 3) &+ noise
      let green = UInt8(truncatingIfNeeded: y >> 3) &+ noise
      let red = UInt8(truncatingIfNeeded: index) &+ noise
      pixel[0] = inBlock ? 40 : blue
      pixel[1] = inBlock ? 220 : green
      pixel[2] = inBlock ? 240 : red
      pixel[3] = 255
    }
  }
}

func write(_ fixture: Fixture, to directory: URL) throws {
  let url = directory.appendingPathComponent("\(fixture.name).mov")
  try? FileManager.default.removeItem(at: url)
  let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
  let input = AVAssetWriterInput(
    mediaType: .video,
    outputSettings: [
      AVVideoCodecKey: AVVideoCodecType.h264,
      AVVideoWidthKey: fixture.width,
      AVVideoHeightKey: fixture.height,
      AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 12_000_000],
    ])
  input.transform = fixture.transform
  input.expectsMediaDataInRealTime = false
  let adaptor = AVAssetWriterInputPixelBufferAdaptor(
    assetWriterInput: input,
    sourcePixelBufferAttributes: [
      kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
      kCVPixelBufferWidthKey as String: fixture.width,
      kCVPixelBufferHeightKey as String: fixture.height,
    ])
  writer.add(input)
  writer.startWriting()
  writer.startSession(atSourceTime: .zero)
  for index in 0..<fixture.frames {
    while !input.isReadyForMoreMediaData { Thread.sleep(forTimeInterval: 0.001) }
    var buffer: CVPixelBuffer?
    CVPixelBufferPoolCreatePixelBuffer(nil, adaptor.pixelBufferPool!, &buffer)
    draw(buffer!, index)
    let time = CMTime(seconds: fixture.time(index), preferredTimescale: 600_000)
    guard adaptor.append(buffer!, withPresentationTime: time) else {
      throw writer.error ?? CocoaError(.fileWriteUnknown)
    }
  }
  input.markAsFinished()
  let done = DispatchSemaphore(value: 0)
  writer.finishWriting { done.signal() }
  done.wait()
  if let error = writer.error { throw error }
  print(url.path)
}

/// 2 s of black video with a 440 Hz tone, for the audio path.
func writeWithSound(to directory: URL) throws {
  let url = directory.appendingPathComponent("h264_audio_440hz.mov")
  try? FileManager.default.removeItem(at: url)
  let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
  let video = AVAssetWriterInput(
    mediaType: .video,
    outputSettings: [
      AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: 640, AVVideoHeightKey: 360,
    ])
  let adaptor = AVAssetWriterInputPixelBufferAdaptor(
    assetWriterInput: video,
    sourcePixelBufferAttributes: [
      kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
      kCVPixelBufferWidthKey as String: 640,
      kCVPixelBufferHeightKey as String: 360,
    ])
  let audio = AVAssetWriterInput(
    mediaType: .audio,
    outputSettings: [
      AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 48_000, AVNumberOfChannelsKey: 2,
      AVEncoderBitRateKey: 128_000,
    ])
  writer.add(video)
  writer.add(audio)
  writer.startWriting()
  writer.startSession(atSourceTime: .zero)

  var description = AudioStreamBasicDescription(
    mSampleRate: 48_000, mFormatID: kAudioFormatLinearPCM,
    mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked, mBytesPerPacket: 8,
    mFramesPerPacket: 1, mBytesPerFrame: 8, mChannelsPerFrame: 2, mBitsPerChannel: 32,
    mReserved: 0)
  var format: CMAudioFormatDescription?
  CMAudioFormatDescriptionCreate(
    allocator: nil, asbd: &description, layoutSize: 0, layout: nil, magicCookieSize: 0,
    magicCookie: nil, extensions: nil, formatDescriptionOut: &format)

  let chunk = 4800
  for index in 0..<20 {
    while !video.isReadyForMoreMediaData || !audio.isReadyForMoreMediaData {
      Thread.sleep(forTimeInterval: 0.001)
    }
    var pixels: CVPixelBuffer?
    CVPixelBufferPoolCreatePixelBuffer(nil, adaptor.pixelBufferPool!, &pixels)
    CVPixelBufferLockBaseAddress(pixels!, [])
    memset(
      CVPixelBufferGetBaseAddress(pixels!), 0,
      CVPixelBufferGetBytesPerRow(pixels!) * CVPixelBufferGetHeight(pixels!))
    CVPixelBufferUnlockBaseAddress(pixels!, [])
    _ = adaptor.append(pixels!, withPresentationTime: CMTime(value: Int64(index), timescale: 10))

    var samples = [Float](repeating: 0, count: chunk * 2)
    for frame in 0..<chunk {
      let t = Double(index * chunk + frame) / 48_000
      let value = Float(sin(2 * Double.pi * 440 * t) * 0.5)
      samples[frame * 2] = value
      samples[frame * 2 + 1] = value
    }
    let bytes = samples.count * MemoryLayout<Float>.size
    var block: CMBlockBuffer?
    CMBlockBufferCreateWithMemoryBlock(
      allocator: nil, memoryBlock: nil, blockLength: bytes, blockAllocator: nil,
      customBlockSource: nil, offsetToData: 0, dataLength: bytes,
      flags: kCMBlockBufferAssureMemoryNowFlag, blockBufferOut: &block)
    samples.withUnsafeBytes {
      _ = CMBlockBufferReplaceDataBytes(
        with: $0.baseAddress!, blockBuffer: block!, offsetIntoDestination: 0, dataLength: bytes)
    }
    var sample: CMSampleBuffer?
    CMAudioSampleBufferCreateReadyWithPacketDescriptions(
      allocator: nil, dataBuffer: block!, formatDescription: format!, sampleCount: chunk,
      presentationTimeStamp: CMTime(value: Int64(index * chunk), timescale: 48_000),
      packetDescriptions: nil, sampleBufferOut: &sample)
    guard audio.append(sample!) else { throw writer.error ?? CocoaError(.fileWriteUnknown) }
  }
  video.markAsFinished()
  audio.markAsFinished()
  let done = DispatchSemaphore(value: 0)
  writer.finishWriting { done.signal() }
  done.wait()
  if let error = writer.error { throw error }
  print(url.path)
}

let directory = URL(fileURLWithPath: CommandLine.arguments[1])
for fixture in fixtures { try write(fixture, to: directory) }
try writeWithSound(to: directory)
