//! H.264 export on Apple platforms: AVAssetWriter over VideoToolbox. Frames are drawn straight
//! into the encoder's own IOSurface-backed buffers, so nothing is copied through the CPU.

use std::path::Path;
use std::ptr;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
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

pub struct AppleEncoder {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
}

/// A frame the encoder will take: draw into `surface`, then pass it to `append`.
pub struct EncoderFrame {
    buffer: CFRetained<CVPixelBuffer>,
    pub surface: CFRetained<IOSurfaceRef>,
}

fn entry(key: Option<&NSString>) -> Result<&NSString, MediaError> {
    key.ok_or(MediaError::EncodeFailed)
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
            if !writer.startWriting() {
                return Err(MediaError::EncodeFailed);
            }
            writer.startSessionAtSourceTime(CMTime::with_seconds(0.0, TIMESCALE));
            Ok(Self {
                writer,
                input,
                adaptor,
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
            let status = CVPixelBufferPool::create_pixel_buffer(
                None,
                &pool,
                std::ptr::NonNull::from(&mut buffer),
            );
            let buffer = std::ptr::NonNull::new(buffer).filter(|_| status == 0);
            let buffer = buffer.ok_or(MediaError::EncodeFailed)?;
            let buffer = CFRetained::from_raw(buffer);
            let surface =
                CVPixelBufferGetIOSurface(Some(&buffer)).ok_or(MediaError::EncodeFailed)?;
            Ok(EncoderFrame { buffer, surface })
        }
    }

    /// Hands a drawn frame to the encoder, waiting while it is busy.
    pub fn append(&self, frame: &EncoderFrame, time_us: i64) -> Result<(), MediaError> {
        let start = Instant::now();
        // SAFETY: plain property reads on live objects.
        while !unsafe { self.input.isReadyForMoreMediaData() } {
            if start.elapsed() > WRITER_TIMEOUT
                || unsafe { self.writer.status() } != AVAssetWriterStatus::Writing
            {
                return Err(MediaError::EncodeFailed);
            }
            thread::sleep(Duration::from_millis(1));
        }
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

    /// Finishes the file and waits for it.
    pub fn finish(self) -> Result<(), MediaError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let done = RcBlock::new(move || {
            let _ = sender.send(());
        });
        // SAFETY: the writer is started; AVFoundation copies the block.
        unsafe {
            self.input.markAsFinished();
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
