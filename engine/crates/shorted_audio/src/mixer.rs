//! Offline-capable mixer: the same function feeds playback and export, so what you hear is what
//! you export. Pure: it reads a project and decoded samples, nothing else.

use std::collections::HashMap;
use std::sync::Arc;

use shorted_core::evaluate::clip_positions;
use shorted_core::model::{AudioItem, AudioTrackRef, ClipAudio, Id, Project};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Decoded stereo at 48 kHz, interleaved.
pub type Samples = Arc<Vec<f32>>;

pub fn us_to_frames(time_us: i64) -> i64 {
    (time_us as f64 * f64::from(SAMPLE_RATE) / 1e6).round() as i64
}

fn frames_to_us(frames: i64) -> i64 {
    (frames as f64 * 1e6 / f64::from(SAMPLE_RATE)).round() as i64
}

/// Gain at `offset_us` into a span of `length_us`, with its fades.
fn fade_gain(offset_us: i64, length_us: i64, fade_in_us: i64, fade_out_us: i64) -> f32 {
    let mut gain = 1.0;
    if fade_in_us > 0 && offset_us < fade_in_us {
        gain *= offset_us as f32 / fade_in_us as f32;
    }
    let remaining = length_us - offset_us;
    if fade_out_us > 0 && remaining < fade_out_us {
        gain *= remaining as f32 / fade_out_us as f32;
    }
    gain.clamp(0.0, 1.0)
}

/// A span of the timeline fed by one source.
struct Voice<'a> {
    samples: &'a [f32],
    start_frame: i64,
    end_frame: i64,
    source_in_frame: i64,
    source_frames: i64,
    speed: f64,
    looped: bool,
    volume: f32,
    fade_in_us: i64,
    fade_out_us: i64,
}

impl Voice<'_> {
    /// Adds this voice to `out`, which starts at timeline frame `first`.
    fn add_to(&self, first: i64, out: &mut [f32]) {
        let frames = (out.len() / CHANNELS) as i64;
        let from = self.start_frame.max(first);
        let to = self.end_frame.min(first + frames);
        let length_us = frames_to_us(self.end_frame - self.start_frame);
        for frame in from..to {
            let local = frame - self.start_frame;
            let mut position = (local as f64 * self.speed).round() as i64;
            if self.looped && self.source_frames > 0 {
                position %= self.source_frames;
            }
            let source = (self.source_in_frame + position) as usize * CHANNELS;
            let Some(pair) = self.samples.get(source..source + CHANNELS) else {
                continue;
            };
            let gain = self.volume
                * fade_gain(
                    frames_to_us(local),
                    length_us,
                    self.fade_in_us,
                    self.fade_out_us,
                );
            let at = (frame - first) as usize * CHANNELS;
            out[at] += pair[0] * gain;
            out[at + 1] += pair[1] * gain;
        }
    }
}

fn clip_voice<'a>(
    samples: &'a [f32],
    audio: &ClipAudio,
    start_us: i64,
    end_us: i64,
    source_in_us: i64,
    speed: f32,
) -> Voice<'a> {
    Voice {
        samples,
        start_frame: us_to_frames(start_us),
        end_frame: us_to_frames(end_us),
        source_in_frame: us_to_frames(source_in_us),
        source_frames: 0,
        speed: f64::from(speed),
        looped: false,
        volume: audio.volume,
        fade_in_us: audio.fade_in_us,
        fade_out_us: audio.fade_out_us,
    }
}

fn item_voice<'a>(samples: &'a [f32], item: &AudioItem, project: &Project) -> Option<Voice<'a>> {
    let flags = match item.track {
        AudioTrackRef::VoiceOver => project.timeline.track_flags.voice_over,
        AudioTrackRef::User(id) => {
            project
                .timeline
                .audio_tracks
                .iter()
                .find(|t| t.id == id)?
                .flags
        }
    };
    (!flags.muted).then(|| Voice {
        samples,
        start_frame: us_to_frames(item.start_us),
        // A looped item has no length of its own: it plays to the end of the project.
        end_frame: if item.loop_ {
            us_to_frames(item.end_us().max(project.duration_us()))
        } else {
            us_to_frames(item.end_us())
        },
        source_in_frame: us_to_frames(item.source_in_us),
        source_frames: us_to_frames(item.source_out_us - item.source_in_us),
        speed: 1.0,
        looped: item.loop_,
        volume: item.volume,
        fade_in_us: item.fade_in_us,
        fade_out_us: item.fade_out_us,
    })
}

/// Fills `out`, interleaved stereo, with the project's sound from timeline frame `first`. Media
/// missing from `sources` is silent: decoding may not have finished.
pub fn mix(project: &Project, sources: &HashMap<Id, Samples>, first: i64, out: &mut [f32]) {
    out.fill(0.0);
    let timeline = &project.timeline;
    if !timeline.track_flags.video.muted {
        for ((_, start, end), clip) in clip_positions(project).into_iter().zip(&timeline.clips) {
            let audio = &clip.original_audio;
            if audio.muted {
                continue;
            }
            if let Some(samples) = sources.get(&clip.media_id) {
                clip_voice(samples, audio, start, end, clip.source_in_us, clip.speed)
                    .add_to(first, out);
            }
        }
    }
    for item in &timeline.audio {
        if let Some(voice) = sources
            .get(&item.media_id)
            .and_then(|samples| item_voice(samples, item, project))
        {
            voice.add_to(first, out);
        }
    }
    for sample in out.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use shorted_core::model::*;
    use uuid::Uuid;

    use super::*;

    fn id(value: u128) -> Id {
        Uuid::from_u128(value)
    }

    fn fixture() -> Project {
        let json = include_str!("../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        project.timeline.transitions.clear();
        project.timeline.clips.truncate(1);
        project.timeline.audio.clear();
        project.timeline.overlays.clear();
        project.captions.sources.clear();
        let clip = &mut project.timeline.clips[0];
        clip.speed = 1.0;
        clip.original_audio.fade_out_us = 0;
        project
    }

    /// Each frame holds its own index, so that the position read can be checked.
    fn ramp(frames: usize) -> Samples {
        let scale = 1.0 / frames as f32;
        Arc::new((0..frames).flat_map(|f| [f as f32 * scale; 2]).collect())
    }

    fn sources(project: &Project, samples: Samples) -> HashMap<Id, Samples> {
        HashMap::from([(project.timeline.clips[0].media_id, samples)])
    }

    #[test]
    fn a_clip_plays_from_its_trim_point() {
        let mut project = fixture();
        project.timeline.clips[0].source_in_us = 1_000_000;
        let samples = ramp(480_000);
        let sources = sources(&project, samples.clone());
        let mut out = [0.0; 4];
        mix(&project, &sources, 0, &mut out);
        assert_eq!(out[0], samples[48_000 * 2]);
    }

    #[test]
    fn speed_reads_faster() {
        let mut project = fixture();
        project.timeline.clips[0].speed = 2.0;
        let samples = ramp(480_000);
        let sources = sources(&project, samples.clone());
        let mut out = [0.0; 2];
        mix(&project, &sources, 1_000, &mut out);
        assert_eq!(out[0], samples[2_000 * 2]);
    }

    #[test]
    fn volume_and_mute_apply() {
        let mut project = fixture();
        project.timeline.clips[0].original_audio.volume = 0.5;
        let samples = Arc::new(vec![0.8; 96_000]);
        let map = sources(&project, samples);
        let mut out = [0.0; 2];
        mix(&project, &map, 0, &mut out);
        assert_eq!(out[0], 0.4);
        project.timeline.clips[0].original_audio.muted = true;
        mix(&project, &map, 0, &mut out);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn a_muted_video_track_silences_clips() {
        let mut project = fixture();
        project.timeline.track_flags.video.muted = true;
        let map = sources(&project, Arc::new(vec![0.8; 96_000]));
        let mut out = [1.0; 2];
        mix(&project, &map, 0, &mut out);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn fades_ramp_and_end_at_silence() {
        assert_eq!(fade_gain(0, 1_000_000, 100_000, 0), 0.0);
        assert_eq!(fade_gain(50_000, 1_000_000, 100_000, 0), 0.5);
        assert_eq!(fade_gain(500_000, 1_000_000, 100_000, 100_000), 1.0);
        assert_eq!(fade_gain(950_000, 1_000_000, 0, 100_000), 0.5);
    }

    fn item(track: AudioTrackRef, media: Id) -> AudioItem {
        AudioItem {
            id: id(40),
            media_id: media,
            role: AudioRole::Music,
            track,
            start_us: 1_000_000,
            source_in_us: 0,
            source_out_us: 1_000_000,
            volume: 1.0,
            fade_in_us: 0,
            fade_out_us: 0,
            loop_: false,
            ducking: None,
            transcript: None,
        }
    }

    #[test]
    fn audio_items_start_on_time_and_add_up() {
        let mut project = fixture();
        project.timeline.clips[0].original_audio.muted = true;
        project
            .timeline
            .audio
            .push(item(AudioTrackRef::VoiceOver, id(12)));
        let map = HashMap::from([(id(12), Arc::new(vec![0.25; 96_000]))]);
        let mut out = [0.0; 4];
        mix(&project, &map, 47_999, &mut out);
        assert_eq!((out[0], out[2]), (0.0, 0.25));
    }

    #[test]
    fn a_muted_track_is_silent() {
        let mut project = fixture();
        project.timeline.clips[0].original_audio.muted = true;
        project.timeline.audio_tracks.push(AudioTrack {
            id: id(30),
            name: "Music".into(),
            flags: TrackFlags {
                muted: true,
                locked: false,
                hidden: false,
            },
        });
        project
            .timeline
            .audio
            .push(item(AudioTrackRef::User(id(30)), id(12)));
        let map = HashMap::from([(id(12), Arc::new(vec![0.25; 96_000]))]);
        let mut out = [1.0; 2];
        mix(&project, &map, 48_000, &mut out);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn looped_items_wrap_within_their_range() {
        let mut project = fixture();
        project.timeline.clips[0].original_audio.muted = true;
        let mut looped = item(AudioTrackRef::VoiceOver, id(12));
        looped.loop_ = true;
        looped.source_out_us = 500_000;
        project.timeline.audio.push(looped);
        let samples = ramp(48_000);
        let map = HashMap::from([(id(12), samples.clone())]);
        let mut out = [0.0; 2];
        mix(&project, &map, 48_000 + 24_001, &mut out);
        assert_eq!(out[0], samples[2]);
    }

    #[test]
    fn missing_media_and_past_the_end_are_silent() {
        let project = fixture();
        let mut out = [1.0; 4];
        mix(&project, &HashMap::new(), 0, &mut out);
        assert_eq!(out, [0.0; 4]);
        let map = sources(&project, Arc::new(vec![0.5; 8]));
        mix(&project, &map, 96_000_000, &mut out);
        assert_eq!(out, [0.0; 4]);
    }

    #[test]
    fn loud_mixes_are_limited() {
        let mut project = fixture();
        project.timeline.clips[0].original_audio.volume = 4.0;
        let map = sources(&project, Arc::new(vec![0.9; 96_000]));
        let mut out = [0.0; 2];
        mix(&project, &map, 0, &mut out);
        assert_eq!(out[0], 1.0);
    }
}
