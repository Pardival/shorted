//! Backend conformance (ADR-0002). Fixtures come from `scripts/make_fixtures.swift`; tests are
//! skipped when they are absent.
#![cfg(target_vendor = "apple")]

use std::path::{Path, PathBuf};

use shorted_media::apple::AppleBackend;
use shorted_media::{DecodedFrame, MediaBackend, VideoDecoder};

fn fixture(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/media")
        .join(name);
    path.exists().then_some(path)
}

#[test]
fn probes_and_decodes_a_constant_rate_clip() {
    let Some(path) = fixture("h264_1080p60.mov") else {
        return;
    };
    let info = AppleBackend.probe(&path).unwrap();
    assert_eq!(
        (info.width, info.height, info.rotation_degrees),
        (1920, 1080, 0)
    );
    assert!(
        (info.nominal_fps - 60.0).abs() < 0.5,
        "{}",
        info.nominal_fps
    );
    assert!(
        (info.duration_us - 10_000_000).abs() < 50_000,
        "{}",
        info.duration_us
    );

    let mut decoder = AppleBackend.open_decoder(&path).unwrap();
    let mut count = 0;
    let mut previous = -1;
    while let Some(frame) = decoder.next_frame().unwrap() {
        assert!(frame.pts_us() > previous);
        assert!(frame.surface().is_some(), "frame must be IOSurface-backed");
        previous = frame.pts_us();
        count += 1;
    }
    assert_eq!(count, 600);
}

#[test]
fn rotation_metadata_gives_a_portrait_display_size() {
    let Some(path) = fixture("h264_1080p60_rotated.mov") else {
        return;
    };
    let info = AppleBackend.probe(&path).unwrap();
    assert_eq!(info.rotation_degrees, 90);
    assert_eq!(info.display_size(), (1080, 1920));
}

#[test]
fn variable_frame_rate_keeps_file_timestamps() {
    let Some(path) = fixture("h264_1080p_vfr.mov") else {
        return;
    };
    let mut decoder = AppleBackend.open_decoder(&path).unwrap();
    let mut times = Vec::new();
    while let Some(frame) = decoder.next_frame().unwrap() {
        times.push(frame.pts_us());
    }
    assert_eq!(times.len(), 120);
    let short = 1e6 / 60.0;
    let long = 1e6 / 24.0;
    for (index, gap) in times
        .windows(2)
        .map(|pair| (pair[1] - pair[0]) as f64)
        .enumerate()
    {
        let expected = if index % 2 == 0 { short } else { long };
        assert!((gap - expected).abs() < 1000.0, "gap {index} is {gap}");
    }
}

#[test]
fn seek_lands_on_the_first_frame_at_or_after_the_target() {
    let Some(path) = fixture("h264_1080p60.mov") else {
        return;
    };
    let mut decoder = AppleBackend.open_decoder(&path).unwrap();
    decoder.seek(5_000_000).unwrap();
    let frame = decoder.next_frame().unwrap().unwrap();
    assert!(
        (frame.pts_us() - 5_000_000).abs() <= 17_000,
        "{}",
        frame.pts_us()
    );
}
