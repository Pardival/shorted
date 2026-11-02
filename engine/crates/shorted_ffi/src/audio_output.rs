//! Sound output. A worker mixes ahead into a lock-free ring; the real-time callback only reads
//! from it, never locks and never allocates (docs/05 §5). The frames it plays are the master
//! clock of playback.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Once};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use shorted_audio::mixer::{mix, us_to_frames, Samples, CHANNELS, SAMPLE_RATE};
use shorted_core::model::{Id, Project};
use shorted_media::apple::decode_audio;

use crate::api::media::resolve;
use crate::transport::transport;

/// About 200 ms: enough for a busy worker, short enough for a seek to feel immediate.
const RING_SAMPLES: usize = 9_600 * CHANNELS;
const CHUNK_FRAMES: usize = 1_024;
const IDLE: Duration = Duration::from_millis(5);

static STARTED: Once = Once::new();

/// Decodes what the project needs, once per media.
pub(crate) fn load_sources(project: &Project, folder: &Path, sources: &mut HashMap<Id, Samples>) {
    let used = project
        .timeline
        .clips
        .iter()
        .map(|clip| clip.media_id)
        .chain(project.timeline.audio.iter().map(|item| item.media_id));
    for id in used {
        if sources.contains_key(&id) {
            continue;
        }
        let media = project.media.iter().find(|media| media.id == id);
        if let Some(media) = media.filter(|media| media.probe.has_audio) {
            let decoded = resolve(media, folder).and_then(|path| decode_audio(&path).ok());
            sources.insert(id, Arc::new(decoded.unwrap_or_default()));
        }
    }
}

fn worker(mut ring: Producer<f32>, flush: Arc<AtomicBool>) {
    let mut sources = HashMap::new();
    let mut seen_revision = u64::MAX;
    let mut seen_generation = u64::MAX;
    let mut next_frame = 0;
    let mut chunk = vec![0.0; CHUNK_FRAMES * CHANNELS];
    loop {
        let (project, folder, playing, start_us, generation, revision) = {
            let state = transport();
            let (start, generation) = state.playback_start();
            (
                state.project.clone(),
                state.folder.clone(),
                // A slowed playback, for recording a path, is silent.
                state.playing() && state.rate == 1.0,
                start,
                generation,
                state.revision,
            )
        };
        let Some(project) = project.filter(|_| playing) else {
            thread::sleep(IDLE);
            continue;
        };
        if revision != seen_revision {
            load_sources(&project, &folder, &mut sources);
            seen_revision = revision;
        }
        if generation != seen_generation {
            seen_generation = generation;
            next_frame = us_to_frames(start_us);
            flush.store(true, Ordering::Release);
        }
        if ring.slots() < chunk.len() {
            thread::sleep(IDLE);
            continue;
        }
        mix(&project, &sources, next_frame, &mut chunk);
        next_frame += CHUNK_FRAMES as i64;
        for sample in &chunk {
            // Room was checked above, and this is the only producer.
            let _ = ring.push(*sample);
        }
    }
}

/// Real-time: reads the ring, counts frames, nothing else.
fn fill(out: &mut [f32], ring: &mut Consumer<f32>, flush: &AtomicBool, played: &AtomicU64) {
    if flush.swap(false, Ordering::Acquire) {
        while ring.pop().is_ok() {}
    }
    let mut delivered = 0;
    for slot in out.iter_mut() {
        match ring.pop() {
            Ok(sample) => {
                *slot = sample;
                delivered += 1;
            }
            Err(_) => *slot = 0.0,
        }
    }
    played.fetch_add((delivered / CHANNELS) as u64, Ordering::Relaxed);
}

fn start() -> Option<(cpal::Stream, Arc<AtomicU64>)> {
    let device = cpal::default_host().default_output_device()?;
    let config = cpal::StreamConfig {
        channels: CHANNELS as u16,
        sample_rate: SAMPLE_RATE,
        buffer_size: cpal::BufferSize::Default,
    };
    let (producer, mut consumer) = RingBuffer::new(RING_SAMPLES);
    let flush = Arc::new(AtomicBool::new(false));
    let played = Arc::new(AtomicU64::new(0));
    let (callback_flush, callback_played) = (flush.clone(), played.clone());
    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], _| fill(out, &mut consumer, &callback_flush, &callback_played),
            |error| tracing::error!(%error, "sound output failed"),
            None,
        )
        .inspect_err(|error| tracing::error!(%error, "sound output unavailable"))
        .ok()?;
    stream.play().ok()?;
    thread::Builder::new()
        .name("shorted-audio".into())
        .spawn(move || worker(producer, flush))
        .ok()?;
    Some((stream, played))
}

/// Starts sound once, in the background: opening a device can take a while, and the frames it
/// plays are the clock. Until it answers, or without a device, playback runs on the system clock.
pub fn ensure_started() {
    STARTED.call_once(|| {
        let opening = thread::Builder::new()
            .name("shorted-audio-open".into())
            .spawn(|| {
                if let Some((stream, played)) = start() {
                    let mut state = transport();
                    let position = state.position_us();
                    state.audio_frames = Some(played);
                    state.set_position(position);
                    // Lives as long as the app.
                    std::mem::forget(stream);
                }
            });
        if let Err(error) = opening {
            tracing::error!(%error, "sound output not started");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_counts_only_real_frames_and_pads_with_silence() {
        let (mut producer, mut consumer) = RingBuffer::new(8);
        for value in [0.1, 0.2, 0.3, 0.4] {
            producer.push(value).unwrap();
        }
        let played = AtomicU64::new(0);
        let flush = AtomicBool::new(false);
        let mut out = [9.0; 6];
        fill(&mut out, &mut consumer, &flush, &played);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4, 0.0, 0.0]);
        assert_eq!(played.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_flush_drops_stale_sound() {
        let (mut producer, mut consumer) = RingBuffer::new(8);
        producer.push(0.5).unwrap();
        let played = AtomicU64::new(0);
        let flush = AtomicBool::new(true);
        let mut out = [9.0; 2];
        fill(&mut out, &mut consumer, &flush, &played);
        assert_eq!(out, [0.0, 0.0]);
        assert_eq!(played.load(Ordering::Relaxed), 0);
    }
}
