//! Voice-over capture. The real-time callback only hands samples to a ring and measures their
//! level; a thread of its own writes them to a WAV file, so the disk never reaches the audio
//! thread (docs/05 §5).

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::error::EngineError;

/// One second at the highest usual rate: the writer only falls this far behind on a stalled disk.
const RING_SAMPLES: usize = 192_000;
const DRAIN_PAUSE: Duration = Duration::from_millis(10);
const WAV_HEADER_BYTES: u64 = 44;
const FLOAT_FORMAT: u16 = 3;
const BITS_PER_SAMPLE: u16 = 32;

struct Recording {
    stop: Sender<()>,
    thread: JoinHandle<io::Result<u64>>,
    path: PathBuf,
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);
static LEVEL_BITS: AtomicU32 = AtomicU32::new(0);
static PAUSED: AtomicBool = AtomicBool::new(false);

fn header(rate: u32, channels: u16, data_bytes: u32) -> [u8; WAV_HEADER_BYTES as usize] {
    let mut bytes = [0; WAV_HEADER_BYTES as usize];
    let block = channels * BITS_PER_SAMPLE / 8;
    bytes[..4].copy_from_slice(b"RIFF");
    bytes[4..8].copy_from_slice(&(data_bytes.saturating_add(36)).to_le_bytes());
    bytes[8..16].copy_from_slice(b"WAVEfmt ");
    bytes[16..20].copy_from_slice(&16_u32.to_le_bytes());
    bytes[20..22].copy_from_slice(&FLOAT_FORMAT.to_le_bytes());
    bytes[22..24].copy_from_slice(&channels.to_le_bytes());
    bytes[24..28].copy_from_slice(&rate.to_le_bytes());
    bytes[28..32].copy_from_slice(&(rate * u32::from(block)).to_le_bytes());
    bytes[32..34].copy_from_slice(&block.to_le_bytes());
    bytes[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes[36..40].copy_from_slice(b"data");
    bytes[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    bytes
}

fn drain(ring: &mut Consumer<f32>, out: &mut impl Write) -> io::Result<u64> {
    let mut written = 0;
    while let Ok(sample) = ring.pop() {
        out.write_all(&sample.to_le_bytes())?;
        written += 4;
    }
    Ok(written)
}

fn write_samples(
    path: &Path,
    mut ring: Consumer<f32>,
    rate: u32,
    channels: u16,
    stop: &Receiver<()>,
) -> io::Result<u64> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    out.write_all(&header(rate, channels, 0))?;
    let mut written = 0;
    while stop.recv_timeout(DRAIN_PAUSE).is_err() {
        written += drain(&mut ring, &mut out)?;
    }
    written += drain(&mut ring, &mut out)?;
    out.seek(SeekFrom::Start(0))?;
    let data_bytes = u32::try_from(written).unwrap_or(u32::MAX);
    out.write_all(&header(rate, channels, data_bytes))?;
    out.flush()?;
    Ok(written)
}

fn capture(input: &[f32], ring: &mut Producer<f32>) {
    if PAUSED.load(Ordering::Relaxed) {
        LEVEL_BITS.store(0, Ordering::Relaxed);
        return;
    }
    let mut loudest = 0.0_f32;
    for sample in input {
        loudest = loudest.max(sample.abs());
        // A full ring drops sound rather than blocking the audio thread.
        let _ = ring.push(*sample);
    }
    LEVEL_BITS.store(loudest.to_bits(), Ordering::Relaxed);
}

/// Starts writing the microphone to `path`.
pub fn start(path: PathBuf) -> Result<(), EngineError> {
    let unavailable = || EngineError::Internal("microphone unavailable".into());
    let mut active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
    if active.is_some() {
        return Err(EngineError::Internal("already recording".into()));
    }
    let (stop, stopped) = mpsc::channel();
    let (ready, opened) = mpsc::channel();
    let target = path.clone();
    PAUSED.store(false, Ordering::Relaxed);
    let thread = thread::Builder::new()
        .name("shorted-voice".into())
        .spawn(move || {
            let opening = || -> Result<(cpal::Stream, Consumer<f32>, u32, u16), String> {
                let device = cpal::default_host()
                    .default_input_device()
                    .ok_or("no input device")?;
                let config = device
                    .default_input_config()
                    .map_err(|error| error.to_string())?;
                let (rate, channels) = (config.sample_rate(), config.channels());
                let (mut producer, consumer) = RingBuffer::new(RING_SAMPLES);
                let stream = device
                    .build_input_stream(
                        config.config(),
                        move |input: &[f32], _| capture(input, &mut producer),
                        |error| tracing::error!(%error, "voice capture failed"),
                        None,
                    )
                    .map_err(|error| error.to_string())?;
                stream.play().map_err(|error| error.to_string())?;
                Ok((stream, consumer, rate, channels))
            };
            match opening() {
                Ok((stream, consumer, rate, channels)) => {
                    let _ = ready.send(true);
                    let result = write_samples(&target, consumer, rate, channels, &stopped);
                    drop(stream);
                    result
                }
                Err(error) => {
                    tracing::error!(%error, "microphone not opened");
                    let _ = ready.send(false);
                    Err(io::Error::other(error))
                }
            }
        })
        .map_err(|_| unavailable())?;
    if opened.recv().unwrap_or(false) {
        *active = Some(Recording { stop, thread, path });
        Ok(())
    } else {
        let _ = thread.join();
        Err(unavailable())
    }
}

pub fn set_paused(paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
}

/// The loudest sample of the last callback, from 0 to 1.
pub fn level() -> f32 {
    f32::from_bits(LEVEL_BITS.load(Ordering::Relaxed))
}

/// Stops and returns the finished file, if sound was written.
pub fn stop() -> Option<PathBuf> {
    let recording = ACTIVE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()?;
    let _ = recording.stop.send(());
    LEVEL_BITS.store(0, Ordering::Relaxed);
    match recording.thread.join() {
        Ok(Ok(bytes)) if bytes > 0 => Some(recording.path),
        Ok(Err(error)) => {
            tracing::error!(%error, "voice-over not saved");
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_describes_float_samples() {
        let bytes = header(48_000, 1, 400);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 436);
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            192_000
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 400);
    }

    #[test]
    fn a_written_file_reads_back_as_sound() {
        let (mut producer, consumer) = RingBuffer::new(9_600);
        for frame in 0..4_800 {
            producer.push((frame as f32 / 20.0).sin() * 0.5).unwrap();
        }
        let path = std::env::temp_dir().join(format!("shorted-voice-{}.wav", uuid::Uuid::now_v7()));
        let (stop, stopped) = mpsc::channel();
        stop.send(()).unwrap();
        write_samples(&path, consumer, 48_000, 1, &stopped).unwrap();
        let decoded = shorted_media::apple::decode_audio(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(decoded.len().abs_diff(9_600) < 1_024, "{}", decoded.len());
        assert!(decoded.iter().any(|sample| sample.abs() > 0.3));
    }
}
