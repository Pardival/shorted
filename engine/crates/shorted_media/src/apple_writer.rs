//! H.264 export on Apple platforms: AVAssetWriter over VideoToolbox. Frames are drawn straight
//! into the encoder's own IOSurface-backed buffers, so nothing is copied through the CPU.

use std::cell::Cell;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked,
    kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescriptionCreate, CMAudioSampleBufferCreateReadyWithPacketDescriptions,
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime, kCMBlockBufferAssureMemoryNowFlag,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetIOSurface, CVPixelBufferPool,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_io_surface::IOSurfaceRef;

use crate::MediaError;

const BGRA_FOURCC: u32 = 0x4247_5241;
const TIMESCALE: i32 = 600_000;
const WRITER_TIMEOUT: Duration = Duration::from_secs(10);
const AUDIO_SAMPLE_RATE: u32 = 48_000;
const AUDIO_CHANNELS: u32 = 2;
const AUDIO_BITS_PER_SECOND: u32 = 192_000;

pub struct AppleEncoder {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    audio: Retained<AVAssetWriterInput>,
    audio_finished: Cell<bool>,
    pcm_format: CFRetained<CMFormatDescription>,
}

/// A frame the encoder will take: draw into `surface`, then pass it to `append`.
pub struct EncoderFrame {
    buffer: CFRetained<CVPixelBuffer>,
    pub surface: CFRetained<IOSurfaceRef>,
}

fn entry(key: Option<&NSString>) -> Result<&NSString, MediaError> {
    key.ok_or(MediaError::EncodeFailed)
}

/// The stereo float the mixer produces.
fn pcm_format() -> Result<CFRetained<CMFormatDescription>, MediaError> {
    let bytes_per_frame = AUDIO_CHANNELS * size_of::<f32>() as u32;
    let description = AudioStreamBasicDescription {
        mSampleRate: f64::from(AUDIO_SAMPLE_RATE),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: AUDIO_CHANNELS,
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    let mut format: *const CMFormatDescription = ptr::null();
    // SAFETY: both pointers are valid for the call; the optional arguments are null.
    let status = unsafe {
        CMAudioFormatDescriptionCreate(
            None,
            NonNull::from(&description),
            0,
            ptr::null(),
            0,
            ptr::null(),
            None,
            NonNull::from(&mut format),
        )
    };
    let format = NonNull::new(format.cast_mut()).filter(|_| status == 0);
    let format = format.ok_or(MediaError::EncodeFailed)?;
    // SAFETY: a Create function returns an owned reference.
    Ok(unsafe { CFRetained::from_raw(format) })
}

fn aac_settings() -> Retained<NSDictionary<NSString, AnyObject>> {
    // Values of AVFAudio's keys, a framework the bindings here do not cover.
    let keys: Vec<Retained<NSString>> = [
        "AVFormatIDKey",
        "AVSampleRateKey",
        "AVNumberOfChannelsKey",
        "AVEncoderBitRateKey",
    ]
    .map(NSString::from_str)
    .into();
    let format = NSNumber::new_u32(kAudioFormatMPEG4AAC);
    let rate = NSNumber::new_u32(AUDIO_SAMPLE_RATE);
    let channels = NSNumber::new_u32(AUDIO_CHANNELS);
    let bitrate = NSNumber::new_u32(AUDIO_BITS_PER_SECOND);
    let values: [&AnyObject; 4] = [&format, &rate, &channels, &bitrate];
    let references: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    NSDictionary::from_slices(&references, &values)
}

impl AppleEncoder {
    /// Writes `path`, replacing any file there: the user chose it in the save panel.
    pub fn new(
        path: &Path,
        width: u32,
        height: u32,
        bits_per_second: u32,
    ) -> Result<Self, MediaError> {
        let _ = std::fs::remove_file(path);
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        // SAFETY: statics from AVFoundation; dictionaries hold the value types the API documents;
        // objects are configured before writing starts.
        unsafe {
            let file_type = AVFileTypeMPEG4.ok_or(MediaError::EncodeFailed)?;
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|_| MediaError::EncodeFailed)?;

            let bitrate = NSNumber::new_u32(bits_per_second);
            let compression_values: [&AnyObject; 1] = [&bitrate];
            let compression =
                NSDictionary::from_slices(&[entry(AVVideoAverageBitRateKey)?], &compression_values);
            let width_value = NSNumber::new_u32(width);
            let height_value = NSNumber::new_u32(height);
            let codec = AVVideoCodecTypeH264.ok_or(MediaError::EncodeFailed)?;
            let values: [&AnyObject; 4] = [codec, &width_value, &height_value, &compression];
            let settings = NSDictionary::from_slices(
                &[
                    entry(AVVideoCodecKey)?,
                    entry(AVVideoWidthKey)?,
                    entry(AVVideoHeightKey)?,
                    entry(AVVideoCompressionPropertiesKey)?,
                ],
                &values,
            );
            let media_type = AVMediaTypeVideo.ok_or(MediaError::EncodeFailed)?;
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                media_type,
                Some(&settings),
            );
            input.setExpectsMediaDataInRealTime(false);

            let key = |value: &'static objc2_core_foundation::CFString| {
                &*ptr::from_ref(value).cast::<NSString>()
            };
            let empty = NSDictionary::<NSString, AnyObject>::new();
            let fourcc = NSNumber::new_u32(BGRA_FOURCC);
            let yes = NSNumber::new_bool(true);
            let width_pixels = NSNumber::new_u32(width);
            let height_pixels = NSNumber::new_u32(height);
            let buffer_values: [&AnyObject; 5] =
                [&fourcc, &empty, &yes, &width_pixels, &height_pixels];
            let buffer_keys = [
                key(kCVPixelBufferPixelFormatTypeKey),
                key(kCVPixelBufferIOSurfacePropertiesKey),
                key(kCVPixelBufferMetalCompatibilityKey),
                key(objc2_core_video::kCVPixelBufferWidthKey),
                key(objc2_core_video::kCVPixelBufferHeightKey),
            ];
            let attributes = NSDictionary::from_slices(&buffer_keys, &buffer_values);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &input,
                Some(&attributes),
            );
            writer.addInput(&input);

            let settings = aac_settings();
            let media_type = AVMediaTypeAudio.ok_or(MediaError::EncodeFailed)?;
            let audio = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                media_type,
                Some(&settings),
            );
            audio.setExpectsMediaDataInRealTime(false);
            writer.addInput(&audio);

            if !writer.startWriting() {
                return Err(MediaError::EncodeFailed);
            }
            writer.startSessionAtSourceTime(CMTime::with_seconds(0.0, TIMESCALE));
            Ok(Self {
                writer,
                input,
                adaptor,
                audio,
                audio_finished: Cell::new(false),
                pcm_format: pcm_format()?,
            })
        }
    }

    pub fn next_frame(&self) -> Result<EncoderFrame, MediaError> {
        // SAFETY: the pool belongs to a started adaptor; the out pointer is valid for the call.
        unsafe {
            let pool = self
                .adaptor
                .pixelBufferPool()
                .ok_or(MediaError::EncodeFailed)?;
            let mut buffer: *mut CVPixelBuffer = ptr::null_mut();
            let status =
                CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut buffer));
            let buffer = NonNull::new(buffer).filter(|_| status == 0);
            let buffer = buffer.ok_or(MediaError::EncodeFailed)?;
            let buffer = CFRetained::from_raw(buffer);
            let surface =
                CVPixelBufferGetIOSurface(Some(&buffer)).ok_or(MediaError::EncodeFailed)?;
            Ok(EncoderFrame { buffer, surface })
        }
    }

    fn wait_until_ready(
        &self,
        input: &AVAssetWriterInput,
        mut meanwhile: impl FnMut() -> Result<(), MediaError>,
    ) -> Result<(), MediaError> {
        let start = Instant::now();
        // SAFETY: plain property reads on live objects.
        while !unsafe { input.isReadyForMoreMediaData() } {
            if start.elapsed() > WRITER_TIMEOUT
                || unsafe { self.writer.status() } != AVAssetWriterStatus::Writing
            {
                return Err(MediaError::EncodeFailed);
            }
            meanwhile()?;
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    pub fn audio_ready(&self) -> bool {
        // SAFETY: plain property read on a live object.
        unsafe { self.audio.isReadyForMoreMediaData() }
    }

    /// The writer interleaves its tracks and holds one back until the other catches up, so a
    /// caller waiting on one track has to keep feeding the other: that is `meanwhile`.
    pub fn wait_for_video(
        &self,
        meanwhile: impl FnMut() -> Result<(), MediaError>,
    ) -> Result<(), MediaError> {
        self.wait_until_ready(&self.input, meanwhile)
    }

    pub fn wait_for_audio(&self) -> Result<(), MediaError> {
        self.wait_until_ready(&self.audio, || Ok(()))
    }

    /// Hands sound to the encoder, which must be ready for it: interleaved stereo at 48 kHz from
    /// sound frame `first_frame`.
    pub fn append_audio(&self, samples: &[f32], first_frame: i64) -> Result<(), MediaError> {
        let frames = samples.len() / AUDIO_CHANNELS as usize;
        let length = size_of_val(samples);
        let source = NonNull::new(samples.as_ptr().cast_mut().cast());
        let source = source.ok_or(MediaError::EncodeFailed)?;
        let mut block: *mut CMBlockBuffer = ptr::null_mut();
        let mut sample: *mut CMSampleBuffer = ptr::null_mut();
        // SAFETY: null memory asks Core Media to allocate `length` bytes, which are then copied
        // from `samples`; out pointers are valid; a Create function returns an owned reference.
        unsafe {
            let status = CMBlockBuffer::create_with_memory_block(
                None,
                ptr::null_mut(),
                length,
                None,
                ptr::null(),
                0,
                length,
                kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut block),
            );
            let block = NonNull::new(block).filter(|_| status == 0);
            let block = CFRetained::from_raw(block.ok_or(MediaError::EncodeFailed)?);
            if CMBlockBuffer::replace_data_bytes(source, &block, 0, length) != 0 {
                return Err(MediaError::EncodeFailed);
            }
            let time = CMTime::with_seconds(
                first_frame as f64 / f64::from(AUDIO_SAMPLE_RATE),
                AUDIO_SAMPLE_RATE as i32,
            );
            let status = CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &block,
                &self.pcm_format,
                frames as isize,
                time,
                ptr::null(),
                NonNull::from(&mut sample),
            );
            let sample = NonNull::new(sample).filter(|_| status == 0);
            let sample = CFRetained::from_raw(sample.ok_or(MediaError::EncodeFailed)?);
            if self.audio.appendSampleBuffer(&sample) {
                Ok(())
            } else {
                Err(MediaError::EncodeFailed)
            }
        }
    }

    /// Hands a drawn frame to the encoder, which must be ready for it.
    pub fn append(&self, frame: &EncoderFrame, time_us: i64) -> Result<(), MediaError> {
        let time = {
            // SAFETY: pure conversion.
            unsafe { CMTime::with_seconds(time_us as f64 / 1e6, TIMESCALE) }
        };
        // SAFETY: the buffer comes from this adaptor's pool, and the GPU finished drawing it.
        let accepted = unsafe {
            self.adaptor
                .appendPixelBuffer_withPresentationTime(&frame.buffer, time)
        };
        if accepted {
            Ok(())
        } else {
            Err(MediaError::EncodeFailed)
        }
    }

    /// No more sound. Without this the last frames can wait forever for sound that never comes.
    pub fn finish_audio(&self) {
        if !self.audio_finished.replace(true) {
            // SAFETY: the writer is started.
            unsafe { self.audio.markAsFinished() };
        }
    }

    /// No more frames. The writer can then release the sound it was holding back.
    pub fn finish_video(&self) {
        // SAFETY: the writer is started.
        unsafe { self.input.markAsFinished() };
    }

    /// Finishes the file and waits for it. Call `finish_video` first.
    pub fn finish(self) -> Result<(), MediaError> {
        self.finish_audio();
        let (sender, receiver) = mpsc::sync_channel(1);
        let done = RcBlock::new(move || {
            let _ = sender.send(());
        });
        // SAFETY: the writer is started; AVFoundation copies the block.
        unsafe {
            self.writer.finishWritingWithCompletionHandler(&done);
        }
        receiver
            .recv_timeout(WRITER_TIMEOUT)
            .map_err(|_| MediaError::EncodeFailed)?;
        // SAFETY: plain property read.
        match unsafe { self.writer.status() } {
            AVAssetWriterStatus::Completed => Ok(()),
            _ => Err(MediaError::EncodeFailed),
        }
    }

    /// Stops and removes the partial file.
    pub fn cancel(self) {
        // SAFETY: the writer is started.
        unsafe { self.writer.cancelWriting() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple::{decode_audio, has_audio};

    #[test]
    fn exported_sound_comes_back_from_the_file() {
        let path = std::env::temp_dir().join(format!("shorted-aac-{}.mp4", std::process::id()));
        let encoder = AppleEncoder::new(&path, 64, 114, 1_000_000).unwrap();
        // One second of 440 Hz, in lockstep with 30 frames as the export loop does.
        let frames = 48_000;
        let tone: Vec<f32> = (0..frames)
            .flat_map(|frame| {
                let phase = frame as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU;
                [phase.sin() * 0.5; 2]
            })
            .collect();
        for index in 0..30 {
            let first = index * 1_600;
            let span = first as usize * 2..(first + 1_600) as usize * 2;
            encoder.wait_for_audio().unwrap();
            encoder.append_audio(&tone[span], first).unwrap();
            encoder.wait_for_video(|| Ok(())).unwrap();
            let frame = encoder.next_frame().unwrap();
            encoder.append(&frame, index * 33_333).unwrap();
        }
        encoder.finish_video();
        encoder.finish().unwrap();

        assert!(has_audio(&path));
        let decoded = decode_audio(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            decoded.len().abs_diff(tone.len()) < 4_096,
            "{}",
            decoded.len()
        );
        // Skips the encoder's start-up, then compares energy: a sine of 0.5 has an RMS of 0.354.
        let middle = &decoded[decoded.len() / 4..decoded.len() * 3 / 4];
        let rms = (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt();
        assert!((rms - 0.354).abs() < 0.02, "{rms}");
    }
}
