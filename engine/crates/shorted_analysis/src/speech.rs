//! Where people speak, from the sound alone, then refined with transcript words (docs/07 §2).

/// Times are microseconds of the source, half open.
pub type Range = (i64, i64);

#[derive(Clone, Copy, Debug)]
pub struct SpeechConfig {
    pub frame_us: i64,
    /// Speech starts this far above the noise floor, and ends `hysteresis_db` lower.
    pub above_floor_db: f32,
    pub hysteresis_db: f32,
    /// Quieter than this is never speech, however quiet the room.
    pub min_level_db: f32,
    /// Shorter bursts are clicks, not words.
    pub min_speech_us: i64,
    /// Pauses shorter than this stay inside the speech around them.
    pub bridge_us: i64,
    /// Kept around speech, so no word is clipped (docs/07 §2).
    pub padding_us: i64,
    /// Silence shorter than this is rhythm, not dead time.
    pub min_dead_us: i64,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            frame_us: 10_000,
            above_floor_db: 12.0,
            hysteresis_db: 6.0,
            min_level_db: -50.0,
            min_speech_us: 120_000,
            bridge_us: 300_000,
            padding_us: 150_000,
            min_dead_us: 700_000,
        }
    }
}

fn level_db(frame: &[f32]) -> f32 {
    let power = frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32;
    10.0 * power.max(1e-12).log10()
}

/// Ranges of speech in mono samples at `rate`, by energy against the clip's own noise floor.
pub fn speech_ranges(mono: &[f32], rate: u32, config: &SpeechConfig) -> Vec<Range> {
    let frame_len = (i64::from(rate) * config.frame_us / 1_000_000).max(1) as usize;
    let levels: Vec<f32> = mono.chunks(frame_len).map(level_db).collect();
    if levels.is_empty() {
        return Vec::new();
    }
    let mut sorted = levels.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    // A clip that is nearly all speech has no quiet tenth: the threshold also stays below
    // what the clip mostly sounds like, so its speech is still found.
    let usual = sorted[sorted.len() * 9 / 10];
    let open = (floor + config.above_floor_db)
        .min(usual - config.above_floor_db)
        .max(config.min_level_db);
    let close = open - config.hysteresis_db;

    let mut ranges: Vec<Range> = Vec::new();
    let mut start: Option<usize> = None;
    for (index, level) in levels.iter().enumerate() {
        match start {
            None if *level >= open => start = Some(index),
            Some(from) if *level < close => {
                ranges.push((
                    from as i64 * config.frame_us,
                    index as i64 * config.frame_us,
                ));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        ranges.push((
            from as i64 * config.frame_us,
            levels.len() as i64 * config.frame_us,
        ));
    }
    let bridged = merge(ranges, config.bridge_us);
    bridged
        .into_iter()
        .filter(|(from, to)| to - from >= config.min_speech_us)
        .collect()
}

/// Sorted, with ranges closer than `gap` joined.
pub fn merge(mut ranges: Vec<Range>, gap: i64) -> Vec<Range> {
    ranges.sort_unstable();
    let mut merged: Vec<Range> = Vec::with_capacity(ranges.len());
    for (from, to) in ranges {
        match merged.last_mut() {
            Some(last) if from - last.1 <= gap => last.1 = last.1.max(to),
            _ => merged.push((from, to)),
        }
    }
    merged
}

/// Speech heard, plus every transcribed word: whisper hears quiet words the meter misses.
pub fn with_words(speech: &[Range], words: &[Range], config: &SpeechConfig) -> Vec<Range> {
    let all = speech.iter().chain(words).copied().collect();
    merge(all, config.bridge_us)
}

/// The dead time of `[start, end)`: everything but padded speech, in stretches long enough to
/// matter.
pub fn dead_ranges(speech: &[Range], start: i64, end: i64, config: &SpeechConfig) -> Vec<Range> {
    let padded = merge(
        speech
            .iter()
            .map(|(from, to)| (from - config.padding_us, to + config.padding_us))
            .collect(),
        0,
    );
    let mut dead = Vec::new();
    let mut cursor = start;
    for (from, to) in padded {
        if from > cursor {
            dead.push((cursor, from.min(end)));
        }
        cursor = cursor.max(to);
        if cursor >= end {
            break;
        }
    }
    if cursor < end {
        dead.push((cursor, end));
    }
    dead.retain(|(from, to)| to - from >= config.min_dead_us);
    dead
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| amplitude * (i as f32 * 0.3).sin())
            .collect()
    }

    #[test]
    fn loud_stretches_are_speech_and_short_pauses_stay_inside() {
        let config = SpeechConfig::default();
        let mut mono = tone(1.0, 0.002);
        mono.extend(tone(0.8, 0.3));
        mono.extend(tone(0.2, 0.002));
        mono.extend(tone(0.5, 0.3));
        mono.extend(tone(1.5, 0.002));
        let ranges = speech_ranges(&mono, RATE, &config);
        assert_eq!(ranges.len(), 1, "{ranges:?}");
        let (from, to) = ranges[0];
        assert!((from - 1_000_000).abs() <= 20_000, "{from}");
        assert!((to - 2_500_000).abs() <= 20_000, "{to}");
    }

    #[test]
    fn a_clip_that_is_nearly_all_speech_keeps_it() {
        let mut mono = tone(0.1, 0.002);
        mono.extend(tone(3.0, 0.3));
        mono.extend(tone(0.1, 0.002));
        let ranges = speech_ranges(&mono, RATE, &SpeechConfig::default());
        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].1 - ranges[0].0 >= 2_900_000);
    }

    #[test]
    fn a_click_is_not_speech() {
        let mut mono = tone(1.0, 0.002);
        mono.extend(tone(0.05, 0.5));
        mono.extend(tone(1.0, 0.002));
        assert!(speech_ranges(&mono, RATE, &SpeechConfig::default()).is_empty());
    }

    #[test]
    fn dead_time_leaves_padding_around_speech() {
        let config = SpeechConfig::default();
        let dead = dead_ranges(&[(2_000_000, 3_000_000)], 0, 6_000_000, &config);
        assert_eq!(dead, vec![(0, 1_850_000), (3_150_000, 6_000_000)]);
        let short = dead_ranges(&[(500_000, 5_800_000)], 0, 6_000_000, &config);
        assert!(short.is_empty(), "{short:?}");
    }

    #[test]
    fn transcribed_words_count_as_speech() {
        let config = SpeechConfig::default();
        let joined = with_words(&[(0, 1_000_000)], &[(1_200_000, 1_500_000)], &config);
        assert_eq!(joined, vec![(0, 1_500_000)]);
    }
}
