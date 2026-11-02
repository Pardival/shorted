//! Picture signals on small sampled frames (docs/07 §2): blur, exposure, shake and scene cuts.

use crate::speech::{Range, merge};

const BINS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct FrameConfig {
    /// A frame is blurry below this share of the clip's median sharpness.
    pub blur_share: f32,
    /// Share of pixels that must be near black, or near white, for a bad exposure.
    pub dark_share: f32,
    pub bright_share: f32,
    /// Mean change of brightness between samples, out of 255, above which the camera shakes.
    pub shake_level: f32,
    /// Histogram distance (0 to 2) above which a new shot starts.
    pub cut_distance: f32,
    /// Bad stretches shorter than this are kept.
    pub min_bad_us: i64,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            blur_share: 0.3,
            dark_share: 0.92,
            bright_share: 0.85,
            shake_level: 40.0,
            cut_distance: 0.9,
            min_bad_us: 500_000,
        }
    }
}

/// What one sampled frame looks like.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameMetrics {
    pub time_us: i64,
    /// Variance of the Laplacian of brightness: low when blurry.
    pub sharpness: f32,
    pub dark: f32,
    pub bright: f32,
    /// Mean brightness change from the previous sample.
    pub motion: f32,
    pub histogram: [f32; BINS],
}

fn luma(rgba: &[u8]) -> Vec<f32> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
        .collect()
}

/// Metrics of a `width` × `height` RGBA frame, compared with the previous sample's brightness.
pub fn measure(
    time_us: i64,
    rgba: &[u8],
    width: usize,
    height: usize,
    previous: Option<&[f32]>,
) -> (FrameMetrics, Vec<f32>) {
    let light = luma(rgba);
    let at = |x: usize, y: usize| light[y * width + x];
    let mut laplacians = Vec::new();
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            laplacians
                .push(at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1) - 4.0 * at(x, y));
        }
    }
    let count = laplacians.len().max(1) as f32;
    let mean = laplacians.iter().sum::<f32>() / count;
    let sharpness = laplacians.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / count;
    let total = light.len().max(1) as f32;
    let dark = light.iter().filter(|v| **v < 16.0).count() as f32 / total;
    let bright = light.iter().filter(|v| **v > 245.0).count() as f32 / total;
    let mut histogram = [0.0; BINS];
    for value in &light {
        histogram[((*value as usize) * BINS / 256).min(BINS - 1)] += 1.0 / total;
    }
    let motion = previous
        .filter(|before| before.len() == light.len())
        .map_or(0.0, |before| {
            before
                .iter()
                .zip(&light)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / total
        });
    (
        FrameMetrics {
            time_us,
            sharpness,
            dark,
            bright,
            motion,
            histogram,
        },
        light,
    )
}

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// Why a stretch of picture is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defect {
    Blur,
    Shake,
    Dark,
    Bright,
}

/// Stretches of bad frames, each reaching to the next sample, with the defect seen first.
pub fn bad_ranges(
    frames: &[FrameMetrics],
    step_us: i64,
    config: &FrameConfig,
) -> Vec<(Range, Defect)> {
    let sharp = median(frames.iter().map(|f| f.sharpness).collect());
    let defect = |frame: &FrameMetrics| {
        if frame.dark >= config.dark_share {
            Some(Defect::Dark)
        } else if frame.bright >= config.bright_share {
            Some(Defect::Bright)
        } else if frame.motion >= config.shake_level {
            Some(Defect::Shake)
        } else if sharp > 0.0 && frame.sharpness < sharp * config.blur_share {
            Some(Defect::Blur)
        } else {
            None
        }
    };
    let mut ranges: Vec<(Range, Defect)> = Vec::new();
    for frame in frames {
        let Some(found) = defect(frame) else {
            continue;
        };
        let range = (frame.time_us, frame.time_us + step_us);
        match ranges.last_mut() {
            Some((last, kind)) if *kind == found && range.0 <= last.1 => last.1 = range.1,
            _ => ranges.push((range, found)),
        }
    }
    ranges.retain(|((from, to), _)| to - from >= config.min_bad_us);
    ranges
}

/// Times where a new shot starts.
pub fn scene_cuts(frames: &[FrameMetrics], config: &FrameConfig) -> Vec<i64> {
    frames
        .windows(2)
        .filter(|pair| {
            let distance: f32 = pair[0]
                .histogram
                .iter()
                .zip(&pair[1].histogram)
                .map(|(a, b)| (a - b).abs())
                .sum();
            distance >= config.cut_distance
        })
        .map(|pair| pair[1].time_us)
        .collect()
}

/// Every defect range, joined when they touch.
pub fn bad_union(ranges: &[(Range, Defect)]) -> Vec<Range> {
    merge(ranges.iter().map(|(range, _)| *range).collect(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: usize = 32;

    fn checker(shade: u8) -> Vec<u8> {
        (0..SIDE * SIDE)
            .flat_map(|i| {
                let on = (i % SIDE + i / SIDE).is_multiple_of(2);
                let v = if on { shade } else { shade / 3 };
                [v, v, v, 255]
            })
            .collect()
    }

    fn flat(v: u8) -> Vec<u8> {
        [v, v, v, 255].repeat(SIDE * SIDE)
    }

    fn sample(time_us: i64, rgba: &[u8], previous: Option<&[f32]>) -> (FrameMetrics, Vec<f32>) {
        measure(time_us, rgba, SIDE, SIDE, previous)
    }

    #[test]
    fn detail_is_sharp_and_a_flat_picture_is_blurry() {
        let (sharp, _) = sample(0, &checker(200), None);
        let (soft, _) = sample(0, &flat(120), None);
        assert!(sharp.sharpness > 1000.0 && soft.sharpness < 1.0);
    }

    #[test]
    fn black_frames_and_blurry_ones_make_bad_ranges() {
        let step = 200_000;
        let mut frames = Vec::new();
        for index in 0..20 {
            let picture = match index {
                0..=4 => flat(2),
                10..=13 => flat(120),
                _ => checker(200),
            };
            frames.push(sample(index * step, &picture, None).0);
        }
        let bad = bad_ranges(&frames, step, &FrameConfig::default());
        assert_eq!(
            bad,
            vec![
                ((0, 1_000_000), Defect::Dark),
                ((2_000_000, 2_800_000), Defect::Blur)
            ]
        );
    }

    #[test]
    fn a_jump_in_the_picture_is_a_cut_and_a_jolt() {
        let (first, light) = sample(0, &checker(200), None);
        let (second, _) = sample(200_000, &flat(250), Some(&light));
        assert!(second.motion > 40.0);
        let cuts = scene_cuts(&[first, second], &FrameConfig::default());
        assert_eq!(cuts, vec![200_000]);
    }
}
