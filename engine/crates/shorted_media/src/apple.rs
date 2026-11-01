//! Hardware decode on Apple platforms: AVAssetReader over VideoToolbox, into IOSurface-backed
//! pixel buffers that the renderer can sample without a CPU copy.

use std::path::Path;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::runtime::Bool;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack, AVMediaTypeVideo,
    AVURLAsset,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeRange, kCMTimePositiveInfinity};
use objc2_core_video::{
    CVImageBuffer, CVPixelBufferGetIOSurface, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
};
use objc2_foundation::{
    NSData, NSDictionary, NSNumber, NSString, NSURL, NSURLBookmarkCreationOptions,
    NSURLBookmarkResolutionOptions,
};
use objc2_io_surface::IOSurfaceRef;

use crate::{DecodedFrame, MediaBackend, MediaError, MediaInfo, VideoDecoder};

const BGRA_FOURCC: u32 = 0x4247_5241;
const TIMESCALE: i32 = 1_000_000;

pub struct AppleBackend;

/// Creates a hex-encoded security-scoped bookmark, which lets the sandboxed app reach a file again
/// after a restart. None where bookmarks are unavailable: the path is then all there is.
pub fn create_bookmark(path: &Path) -> Option<String> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope
                | NSURLBookmarkCreationOptions::SecurityScopeAllowOnlyReadAccess,
            None,
            None,
        )
        .ok()?;
    Some(
        data.to_vec()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// Finds a bookmarked file, wherever it moved, and opens access to it for this run.
pub fn resolve_bookmark(bookmark: &str) -> Option<std::path::PathBuf> {
    let bytes: Option<Vec<u8>> = (0..bookmark.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(bookmark.get(at..at + 2)?, 16).ok())
        .collect();
    let data = NSData::with_bytes(&bytes?);
    let mut stale = Bool::NO;
    // SAFETY: `data` and `stale` outlive the call; the URL is used on this thread only.
    let url = unsafe {
        let url =
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions::WithSecurityScope,
                None,
                &raw mut stale,
            )
            .ok()?;
        url.startAccessingSecurityScopedResource();
        url
    };
    Some(std::path::PathBuf::from(url.path()?.to_string()))
}

fn seconds_to_us(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

fn open_track(path: &Path) -> Result<(Retained<AVURLAsset>, Retained<AVAssetTrack>), MediaError> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    // SAFETY: valid URL, no options. The media type statics are provided by AVFoundation.
    unsafe {
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let media_type = AVMediaTypeVideo.ok_or(MediaError::OpenFailed)?;
        // The synchronous call is deprecated but loads at once, which suits this spike.
        #[allow(deprecated)]
        let tracks = asset.tracksWithMediaType(media_type);
        let track = tracks.firstObject().ok_or(MediaError::NoVideoTrack)?;
        Ok((asset, track))
    }
}

impl MediaBackend for AppleBackend {
    type Decoder = AppleDecoder;

    fn probe(&self, path: &Path) -> Result<MediaInfo, MediaError> {
        autoreleasepool(|_| {
            let (asset, track) = open_track(path)?;
            // SAFETY: plain property reads on live objects.
            unsafe {
                #[allow(deprecated)]
                let size = track.naturalSize();
                #[allow(deprecated)]
                let transform = track.preferredTransform();
                #[allow(deprecated)]
                let duration = asset.duration();
                Ok(MediaInfo {
                    width: size.width as u32,
                    height: size.height as u32,
                    rotation_degrees: transform.b.atan2(transform.a).to_degrees().round() as i32,
                    duration_us: seconds_to_us(duration.seconds()),
                    nominal_fps: track.nominalFrameRate(),
                })
            }
        })
    }

    fn open_decoder(&self, path: &Path) -> Result<AppleDecoder, MediaError> {
        autoreleasepool(|_| {
            let (asset, track) = open_track(path)?;
            let mut decoder = AppleDecoder {
                asset,
                track,
                reader: None,
            };
            decoder.start_at(0)?;
            Ok(decoder)
        })
    }
}

struct ActiveReader {
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
}

pub struct AppleDecoder {
    asset: Retained<AVURLAsset>,
    track: Retained<AVAssetTrack>,
    reader: Option<ActiveReader>,
}

fn output_settings() -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: these CFString keys are toll-free bridged to NSString and are valid statics.
    let key = |value: &'static objc2_core_foundation::CFString| unsafe {
        &*std::ptr::from_ref(value).cast::<NSString>()
    };
    // SAFETY: as above, for the three CoreVideo statics.
    let (format, surface, metal) = unsafe {
        (
            key(kCVPixelBufferPixelFormatTypeKey),
            key(kCVPixelBufferIOSurfacePropertiesKey),
            key(kCVPixelBufferMetalCompatibilityKey),
        )
    };
    let empty = NSDictionary::<NSString, AnyObject>::new();
    let fourcc = NSNumber::new_u32(BGRA_FOURCC);
    let yes = NSNumber::new_bool(true);
    let values: [&AnyObject; 3] = [&fourcc, &empty, &yes];
    NSDictionary::from_slices(&[format, surface, metal], &values)
}

impl AppleDecoder {
    fn start_at(&mut self, time_us: i64) -> Result<(), MediaError> {
        // SAFETY: objects are live; settings are valid for a track output; the time range is valid
        // and set before reading starts.
        unsafe {
            let reader = AVAssetReader::assetReaderWithAsset_error(&self.asset)
                .map_err(|_| MediaError::OpenFailed)?;
            let settings = output_settings();
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                &self.track,
                Some(&settings),
            );
            output.setAlwaysCopiesSampleData(false);
            reader.setTimeRange(CMTimeRange {
                start: CMTime::with_seconds(time_us as f64 / 1e6, TIMESCALE),
                duration: kCMTimePositiveInfinity,
            });
            reader.addOutput(&output);
            if !reader.startReading() {
                return Err(MediaError::OpenFailed);
            }
            self.reader = Some(ActiveReader { reader, output });
        }
        Ok(())
    }
}

pub struct AppleFrame {
    pts_us: i64,
    pixel_buffer: CFRetained<CVImageBuffer>,
    // Keeps the buffer valid: it may belong to the sample.
    _sample: Retained<CMSampleBuffer>,
}

impl AppleFrame {
    /// The frame's surface. Holding it keeps the pixels alive.
    pub fn surface(&self) -> Option<CFRetained<IOSurfaceRef>> {
        CVPixelBufferGetIOSurface(Some(&self.pixel_buffer))
    }
}

impl DecodedFrame for AppleFrame {
    fn pts_us(&self) -> i64 {
        self.pts_us
    }
}

impl VideoDecoder for AppleDecoder {
    type Frame = AppleFrame;

    fn next_frame(&mut self) -> Result<Option<AppleFrame>, MediaError> {
        let active = self.reader.as_ref().ok_or(MediaError::DecodeFailed)?;
        autoreleasepool(|_| {
            // SAFETY: the output belongs to a started reader, used from one thread at a time.
            let sample = unsafe { active.output.copyNextSampleBuffer() };
            let Some(sample) = sample else {
                // SAFETY: plain property read.
                return if unsafe { active.reader.status() } == AVAssetReaderStatus::Failed {
                    Err(MediaError::DecodeFailed)
                } else {
                    Ok(None)
                };
            };
            // SAFETY: `sample` is a valid video sample from the reader.
            let (pixel_buffer, seconds) = unsafe {
                (
                    sample.image_buffer(),
                    sample.presentation_time_stamp().seconds(),
                )
            };
            let pixel_buffer = pixel_buffer.ok_or(MediaError::DecodeFailed)?;
            Ok(Some(AppleFrame {
                pts_us: seconds_to_us(seconds),
                pixel_buffer,
                _sample: sample,
            }))
        })
    }

    fn seek(&mut self, time_us: i64) -> Result<(), MediaError> {
        autoreleasepool(|_| self.start_at(time_us))
    }
}
