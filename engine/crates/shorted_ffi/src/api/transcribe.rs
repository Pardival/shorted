//! The speech model and transcription, and the transcript as the Captions section shows it.

// Helpers only serve the Apple build until other platforms have a decoder.
#![cfg_attr(not(target_vendor = "apple"), allow(dead_code))]

use std::path::{Path, PathBuf};

use shorted_ai::providers::{ProviderKind, Secret};
use shorted_ai::speech::{
    read_transcription, transcription_upload, wav, UploadHttp, MAX_CHUNK_SAMPLES,
};
use shorted_core::captions::{apply_auto_emphasis, apply_preset, srt};
use shorted_core::command::{Command, TranscriptOwner};
use shorted_core::model::*;
use shorted_transcribe::{group_words, SpokenWord, BASE_MODEL};
use uuid::Uuid;

use crate::ai_access::{DeviceHttp, KeyStore, SystemKeychain};
use crate::api::clips::{edit, parse};
#[cfg(target_vendor = "apple")]
use crate::api::media::resolve;
use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView};
use crate::error::{EngineError, EngineFailure};
use crate::frb_generated::StreamSink;
use crate::runtime::{self, Priority, TaskContext, TaskEvent};
use crate::transport::transport;

fn model_path(root: &Path) -> PathBuf {
    root.join("models").join(BASE_MODEL.file)
}

pub struct ModelView {
    pub ready: bool,
    pub bytes: u64,
}

/// Whether the model is on disk, and its size whether it is or not.
#[flutter_rust_bridge::frb(sync)]
pub fn speech_model(root: String) -> ModelView {
    ModelView {
        ready: model_path(Path::new(&root)).exists(),
        bytes: BASE_MODEL.bytes,
    }
}

pub async fn delete_speech_model(root: String) -> Result<(), EngineFailure> {
    let path = model_path(Path::new(&root));
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(shorted_store::session::StoreError::from)
            .map_err(EngineError::from)?;
    }
    Ok(())
}

#[cfg(target_vendor = "apple")]
fn download(target: &Path, task: &TaskContext<'_>) -> Result<(), EngineError> {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};

    use shorted_transcribe::whisper::verify;
    use ureq::tls::{TlsConfig, TlsProvider};

    fn failed<E>(_: E) -> EngineError {
        EngineError::Internal("model not downloaded".into())
    }
    let folder = target.parent().ok_or_else(|| failed(()))?;
    std::fs::create_dir_all(folder).map_err(|_| failed(()))?;
    let part = target.with_extension("part");
    // A partial file resumes where it stopped.
    let done = std::fs::metadata(&part).map_or(0, |m| m.len());
    let tls = TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).build().into();
    let mut response = agent
        .get(BASE_MODEL.url)
        .header("Range", format!("bytes={done}-"))
        .call()
        .map_err(|_| failed(()))?;
    let resumed = response.status().as_u16() == 206;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)
        .map_err(|_| failed(()))?;
    let mut written = if resumed { done } else { 0 };
    let mut body = response.body_mut().as_reader();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        task.check_cancelled()?;
        let read = body.read(&mut buffer).map_err(|_| failed(()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|_| failed(()))?;
        written += read as u64;
        task.progress(written as f64 / BASE_MODEL.bytes as f64);
    }
    file.flush().map_err(|_| failed(()))?;
    drop(file);
    if !verify(&part, &BASE_MODEL).map_err(|_| failed(()))? {
        // A wrong file must never be loaded, and must not poison the next try.
        let _ = std::fs::remove_file(&part);
        return Err(failed(()));
    }
    std::fs::rename(&part, target).map_err(|_| failed(()))
}

#[cfg(not(target_vendor = "apple"))]
fn download(_target: &Path, _task: &TaskContext<'_>) -> Result<(), EngineError> {
    Err(EngineError::Internal("no transcription here yet".into()))
}

/// Downloads and checks the model. The first event carries the id for `cancel_task`.
pub fn download_speech_model(root: String, events: StreamSink<TaskEvent>) {
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            download(&model_path(Path::new(&root)), task)
        });
}

fn to_transcript(words: Vec<SpokenWord>, offset_us: i64, language: String) -> Transcript {
    let segments = group_words(words)
        .into_iter()
        .map(|phrase| Segment {
            id: Uuid::now_v7(),
            hidden: false,
            words: phrase
                .into_iter()
                .map(|word| Word {
                    original_text: word.text.clone(),
                    text: word.text,
                    start_us: word.start_us + offset_us,
                    end_us: word.end_us + offset_us,
                    confidence: word.confidence,
                    emphasis: EmphasisState::Auto(false),
                    deleted: false,
                })
                .collect(),
        })
        .collect();
    let mut transcript = Transcript { language, segments };
    apply_auto_emphasis(&mut transcript);
    transcript
}

/// A sound to transcribe: who owns the words, and the part of the file that is used.
struct Job {
    owner: TranscriptOwner,
    media: MediaRef,
    from_us: i64,
    to_us: i64,
}

/// What has no transcript yet; with `again`, everything that can be heard.
fn jobs(project: &Project, again: bool) -> Vec<Job> {
    let find = |id: Id| {
        project
            .media
            .iter()
            .find(|m| m.id == id && m.probe.has_audio)
    };
    let clips = project
        .timeline
        .clips
        .iter()
        .filter(|clip| (again || clip.transcript.is_none()) && clip.original_audio.transcribe)
        .filter_map(|clip| {
            Some(Job {
                owner: TranscriptOwner::Clip(clip.id),
                media: find(clip.media_id)?.clone(),
                from_us: clip.source_in_us,
                to_us: clip.source_out_us,
            })
        });
    // FR-AUD-05: voices are transcribed, music is not.
    let voices = project
        .timeline
        .audio
        .iter()
        .filter(|item| matches!(item.role, AudioRole::VoiceOver | AudioRole::ImportedVoice))
        .filter(|item| again || item.transcript.is_none())
        .filter_map(|item| {
            Some(Job {
                owner: TranscriptOwner::Audio(item.id),
                media: find(item.media_id)?.clone(),
                from_us: item.source_in_us,
                to_us: item.source_out_us,
            })
        });
    clips.chain(voices).collect()
}

/// Turns 16 kHz mono speech into words and their language.
type Hear<'a> = dyn FnMut(&[f32]) -> Result<(Vec<SpokenWord>, String), EngineError> + 'a;

#[cfg(target_vendor = "apple")]
fn run_jobs(
    project: &Project,
    folder: &Path,
    again: bool,
    hear: &mut Hear<'_>,
    task: &TaskContext<'_>,
) -> Result<Vec<Command>, EngineError> {
    use shorted_audio::mixer::{us_to_frames, CHANNELS};
    use shorted_media::apple::decode_audio;
    use shorted_transcribe::to_whisper_input;

    let jobs = jobs(project, again);
    let mut commands = Vec::new();
    for (index, job) in jobs.iter().enumerate() {
        task.check_cancelled()?;
        task.progress(index as f64 / jobs.len() as f64);
        let Some(path) = resolve(&job.media, folder) else {
            continue;
        };
        let samples = decode_audio(&path)
            .map_err(|_| EngineError::Internal("transcription failed".into()))?;
        let from = us_to_frames(job.from_us) as usize * CHANNELS;
        let to = (us_to_frames(job.to_us) as usize * CHANNELS).min(samples.len());
        let speech = to_whisper_input(samples.get(from..to).unwrap_or_default());
        let (words, language) = hear(&speech)?;
        commands.push(Command::SetTranscript {
            owner: job.owner,
            transcript: Some(to_transcript(words, job.from_us, language)),
        });
    }
    Ok(commands)
}

#[cfg(not(target_vendor = "apple"))]
fn run_jobs(
    _project: &Project,
    _folder: &Path,
    _again: bool,
    _hear: &mut Hear<'_>,
    _task: &TaskContext<'_>,
) -> Result<Vec<Command>, EngineError> {
    Err(EngineError::Internal("no transcription here yet".into()))
}

#[cfg(target_vendor = "apple")]
fn run_transcription(
    project: &Project,
    folder: &Path,
    model: &Path,
    task: &TaskContext<'_>,
) -> Result<Vec<Command>, EngineError> {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use shorted_transcribe::whisper::Transcriber;

    fn failed<E>(_: E) -> EngineError {
        EngineError::Internal("transcription failed".into())
    }
    let transcriber = Transcriber::load(model).map_err(failed)?;
    let mut hear = |speech: &[f32]| {
        transcriber
            .transcribe(speech, None, Arc::new(AtomicBool::new(false)))
            .map_err(failed)
    };
    run_jobs(project, folder, false, &mut hear, task)
}

#[cfg(not(target_vendor = "apple"))]
fn run_transcription(
    _project: &Project,
    _folder: &Path,
    _model: &Path,
    _task: &TaskContext<'_>,
) -> Result<Vec<Command>, EngineError> {
    Err(EngineError::Internal("no transcription here yet".into()))
}

/// Sends each sound to OpenAI, ten minutes at a time, and reads the words back (T-1201).
fn hear_with_openai(
    http: &dyn UploadHttp,
    key: &Secret,
    speech: &[f32],
) -> Result<(Vec<SpokenWord>, String), EngineError> {
    let mut words = Vec::new();
    let mut language = String::new();
    for (index, chunk) in speech.chunks(MAX_CHUNK_SAMPLES).enumerate() {
        let offset_us = (index * MAX_CHUNK_SAMPLES) as i64 * 1_000_000
            / i64::from(shorted_transcribe::WHISPER_SAMPLE_RATE);
        let upload = transcription_upload(
            key,
            None,
            &wav(chunk, shorted_transcribe::WHISPER_SAMPLE_RATE),
        );
        let response = http.upload(&upload).map_err(EngineError::Ai)?;
        let (heard, spoken) = read_transcription(&response).map_err(EngineError::Ai)?;
        if language.is_empty() {
            language = spoken;
        }
        words.extend(heard.into_iter().map(|word| SpokenWord {
            text: word.text,
            start_us: word.start_us + offset_us,
            end_us: word.end_us + offset_us,
            confidence: 1.0,
        }));
    }
    Ok((words, language))
}

/// The edit for the transcripts heard, with voices added to the caption sources.
fn transcription_edit(project: &Project, commands: Vec<Command>) -> Command {
    let mut settings = project.captions.clone();
    for command in &commands {
        if let Command::SetTranscript {
            owner: TranscriptOwner::Audio(id),
            ..
        } = command
        {
            let source = CaptionSource::AudioItem(*id);
            if !settings.sources.contains(&source) {
                settings.sources.push(source);
            }
        }
    }
    let mut commands = commands;
    commands.push(Command::SetCaptionSettings {
        settings: Box::new(settings),
    });
    Command::Group {
        label: "transcribe".into(),
        commands,
    }
}

fn apply_transcription(project: &Project, commands: Vec<Command>) -> Result<(), EngineError> {
    if commands.is_empty() {
        return Ok(());
    }
    let edit = transcription_edit(project, commands);
    on_command_thread(move || {
        with_active(|session| {
            session.execute(&edit)?;
            Ok(())
        })
    })
    .map_err(|failure| EngineError::Internal(format!("{:?}", failure.code)))
}

/// Whether an OpenAI key is in the keychain, for transcription in the cloud.
#[flutter_rust_bridge::frb(sync)]
pub fn openai_transcription_ready() -> bool {
    SystemKeychain.get(ProviderKind::OpenAi.id()).is_some()
}

/// Keeps the user's OpenAI key in the keychain; the same entry the AI settings use.
pub async fn save_openai_key(key: String) -> Result<(), EngineFailure> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(EngineError::AiNotConfigured.into());
    }
    SystemKeychain.set(ProviderKind::OpenAi.id(), &Secret::new(key))?;
    Ok(())
}

/// Transcribes every sound again with OpenAI, after the user agreed to send it (T-1201).
pub fn transcribe_with_openai(events: StreamSink<TaskEvent>) {
    let (project, folder) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            let project = project.ok_or(EngineError::ExportFailed)?;
            let key = SystemKeychain
                .get(ProviderKind::OpenAi.id())
                .ok_or(EngineError::AiNotConfigured)?;
            let mut hear = |speech: &[f32]| hear_with_openai(&DeviceHttp, &key, speech);
            let commands = run_jobs(&project, &folder, true, &mut hear, task)?;
            apply_transcription(&project, commands)
        });
}

/// Transcribes what has no transcript yet, on the device, as one undo step.
pub fn transcribe_project(root: String, events: StreamSink<TaskEvent>) {
    let (project, folder) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            let project = project.ok_or(EngineError::ExportFailed)?;
            let model = model_path(Path::new(&root));
            let commands = run_transcription(&project, &folder, &model, task)?;
            apply_transcription(&project, commands)
        });
}

pub struct WordView {
    pub index: u32,
    pub text: String,
    pub start_us: i64,
    pub deleted: bool,
    pub emphasis: bool,
}

pub struct SegmentView {
    pub id: String,
    pub hidden: bool,
    pub words: Vec<WordView>,
}

pub struct TranscriptView {
    pub clip_id: Option<String>,
    pub audio_id: Option<String>,
    pub language: String,
    pub segments: Vec<SegmentView>,
}

fn transcript_view(
    clip_id: Option<String>,
    audio_id: Option<String>,
    transcript: &Transcript,
) -> TranscriptView {
    TranscriptView {
        clip_id,
        audio_id,
        language: transcript.language.clone(),
        segments: transcript
            .segments
            .iter()
            .map(|segment| SegmentView {
                id: segment.id.to_string(),
                hidden: segment.hidden,
                words: segment
                    .words
                    .iter()
                    .enumerate()
                    .map(|(index, word)| WordView {
                        index: index as u32,
                        text: word.text.clone(),
                        start_us: word.start_us,
                        deleted: word.deleted,
                        emphasis: matches!(
                            word.emphasis,
                            EmphasisState::Auto(true) | EmphasisState::User(true)
                        ),
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub(crate) fn transcript_views(project: &Project) -> Vec<TranscriptView> {
    let timeline = &project.timeline;
    let clips = timeline.clips.iter().filter_map(|clip| {
        let transcript = clip.transcript.as_ref()?;
        Some(transcript_view(Some(clip.id.to_string()), None, transcript))
    });
    let voices = timeline.audio.iter().filter_map(|item| {
        let transcript = item.transcript.as_ref()?;
        Some(transcript_view(None, Some(item.id.to_string()), transcript))
    });
    clips.chain(voices).collect()
}

fn owner(
    clip_id: Option<String>,
    audio_id: Option<String>,
) -> Result<TranscriptOwner, EngineError> {
    match (clip_id, audio_id) {
        (Some(id), _) => Ok(TranscriptOwner::Clip(parse(&id)?)),
        (_, Some(id)) => Ok(TranscriptOwner::Audio(parse(&id)?)),
        _ => Err(EngineError::Internal("no transcript owner".into())),
    }
}

/// Fixes a word. The media is untouched.
pub async fn edit_word(
    clip_id: Option<String>,
    audio_id: Option<String>,
    segment: String,
    word: u32,
    text: String,
) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::EditWord {
            owner: owner(clip_id, audio_id)?,
            segment: parse(&segment)?,
            word: word as usize,
            text,
        })
    })
}

/// Shows or hides a phrase in captions (FR-TRN-04).
pub async fn hide_segment(
    clip_id: Option<String>,
    audio_id: Option<String>,
    segment: String,
    hidden: bool,
) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::HideSegment {
            owner: owner(clip_id, audio_id)?,
            segment: parse(&segment)?,
            hidden,
        })
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CaptionPresetView {
    WordByWord,
    WordGroups,
    Sentence,
    Karaoke,
    Outline,
    Box,
    Bold,
    Minimal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CaptionAnimationView {
    None,
    Pop,
    Fade,
    Typewriter,
}

impl From<CaptionPresetView> for CaptionPreset {
    fn from(preset: CaptionPresetView) -> Self {
        match preset {
            CaptionPresetView::WordByWord => Self::WordByWord,
            CaptionPresetView::WordGroups => Self::WordGroups,
            CaptionPresetView::Sentence => Self::Sentence,
            CaptionPresetView::Karaoke => Self::Karaoke,
            CaptionPresetView::Outline => Self::Outline,
            CaptionPresetView::Box => Self::Box,
            CaptionPresetView::Bold => Self::Bold,
            CaptionPresetView::Minimal => Self::Minimal,
        }
    }
}

impl From<CaptionPreset> for CaptionPresetView {
    fn from(preset: CaptionPreset) -> Self {
        match preset {
            CaptionPreset::WordByWord => Self::WordByWord,
            CaptionPreset::WordGroups => Self::WordGroups,
            CaptionPreset::Sentence => Self::Sentence,
            CaptionPreset::Karaoke => Self::Karaoke,
            CaptionPreset::Outline => Self::Outline,
            CaptionPreset::Box => Self::Box,
            CaptionPreset::Bold => Self::Bold,
            CaptionPreset::Minimal => Self::Minimal,
        }
    }
}

impl From<CaptionAnimationView> for CaptionAnimation {
    fn from(animation: CaptionAnimationView) -> Self {
        match animation {
            CaptionAnimationView::None => Self::None,
            CaptionAnimationView::Pop => Self::Pop,
            CaptionAnimationView::Fade => Self::Fade,
            CaptionAnimationView::Typewriter => Self::Typewriter,
        }
    }
}

impl From<CaptionAnimation> for CaptionAnimationView {
    fn from(animation: CaptionAnimation) -> Self {
        match animation {
            CaptionAnimation::None => Self::None,
            CaptionAnimation::Pop => Self::Pop,
            CaptionAnimation::Fade => Self::Fade,
            CaptionAnimation::Typewriter => Self::Typewriter,
        }
    }
}

pub struct CaptionsView {
    pub enabled: bool,
    pub preset: CaptionPresetView,
    pub animation: CaptionAnimationView,
    pub font: String,
    pub size: f32,
    pub uppercase: bool,
    pub color: String,
    pub emphasis_color: String,
    pub anchor: f32,
}

pub(crate) fn captions_view(project: &Project) -> CaptionsView {
    let settings = &project.captions;
    CaptionsView {
        enabled: settings.enabled,
        preset: settings.preset.into(),
        animation: settings.animation.into(),
        font: settings.style.font.clone(),
        size: settings.style.size,
        uppercase: settings.style.uppercase,
        color: settings.style.color.clone(),
        emphasis_color: settings.emphasis_color.clone(),
        anchor: settings.vertical_anchor,
    }
}

fn settings_command(
    project: &Project,
    change: impl FnOnce(&mut CaptionSettings),
) -> Result<Command, EngineError> {
    let mut settings = project.captions.clone();
    change(&mut settings);
    Ok(Command::SetCaptionSettings {
        settings: Box::new(settings),
    })
}

/// Switches to a preset, which sets grouping, entrance and case (FR-CAP-02).
pub async fn set_caption_preset(preset: CaptionPresetView) -> Result<ProjectView, EngineFailure> {
    edit(move |project| settings_command(project, |settings| apply_preset(settings, preset.into())))
}

pub async fn set_captions_enabled(enabled: bool) -> Result<ProjectView, EngineFailure> {
    edit(move |project| settings_command(project, |settings| settings.enabled = enabled))
}

pub async fn set_caption_animation(
    animation: CaptionAnimationView,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| settings_command(project, |settings| settings.animation = animation.into()))
}

/// The look a user adjusts on top of a preset (FR-CAP-03).
pub async fn set_caption_style(
    font: String,
    size: f32,
    uppercase: bool,
    color: String,
    emphasis_color: String,
    anchor: f32,
) -> Result<ProjectView, EngineFailure> {
    edit(move |project| {
        settings_command(project, |settings| {
            settings.style.font = font;
            settings.style.size = size;
            settings.style.uppercase = uppercase;
            settings.style.color = color;
            settings.emphasis_color = emphasis_color;
            settings.vertical_anchor = anchor;
        })
    })
}

/// Turns the emphasis of one word on or off; the choice then stays the user's (FR-CAP-04).
pub async fn toggle_emphasis(
    clip_id: Option<String>,
    audio_id: Option<String>,
    segment: String,
    word: u32,
) -> Result<ProjectView, EngineFailure> {
    edit(move |_| {
        Ok(Command::ToggleEmphasis {
            owner: owner(clip_id, audio_id)?,
            segment: parse(&segment)?,
            word: word as usize,
        })
    })
}

/// Writes the project's captions next to an export (FR-CAP-07).
pub async fn write_srt(path: String) -> Result<(), EngineFailure> {
    let project = transport().project.clone();
    let project = project.ok_or_else(|| EngineError::Internal("no project is open".into()))?;
    std::fs::write(path, srt(&project))
        .map_err(shorted_store::session::StoreError::from)
        .map_err(EngineError::from)?;
    Ok(())
}

/// The view after a background task changed the project.
pub async fn current_view() -> Result<ProjectView, EngineFailure> {
    on_command_thread(|| with_active(|session| Ok(view_of(session))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start_us: i64) -> SpokenWord {
        SpokenWord {
            text: text.into(),
            start_us,
            end_us: start_us + 100,
            confidence: 0.9,
        }
    }

    /// Fetches the real file: about 60 MB.
    #[cfg(target_vendor = "apple")]
    #[test]
    #[ignore = "uses the network"]
    fn the_pinned_model_downloads_and_passes_its_check() {
        let root = std::env::temp_dir().join(format!("shorted-model-{}", Uuid::now_v7()));
        let target = model_path(&root);
        crate::runtime::with_test_context(|task| download(&target, task)).unwrap();
        assert_eq!(std::fs::metadata(&target).unwrap().len(), BASE_MODEL.bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Real speech through the whole path. Needs SHORTED_WHISPER_MODEL.
    #[cfg(target_vendor = "apple")]
    #[test]
    #[ignore = "needs the speech model"]
    fn a_project_clip_gets_its_transcript() {
        let wav = std::env::temp_dir().join(format!("shorted-voice-{}.wav", Uuid::now_v7()));
        let spoken = std::process::Command::new("say")
            .arg("-o")
            .arg(&wav)
            .args(["--data-format=LEF32@48000", "--channels=2"])
            .arg("Welcome to this short video.")
            .status()
            .unwrap();
        assert!(spoken.success());
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        project.timeline.audio.clear();
        project.timeline.clips.truncate(1);
        let clip = &mut project.timeline.clips[0];
        clip.transcript = None;
        clip.original_audio.transcribe = true;
        clip.source_in_us = 0;
        clip.source_out_us = 60_000_000;
        let id = clip.media_id;
        let media = project.media.iter_mut().find(|m| m.id == id).unwrap();
        media.probe.has_audio = true;
        media.location = MediaLocation::External {
            abs_path: wav.to_string_lossy().into_owned(),
            bookmark: None,
        };
        let model = PathBuf::from(std::env::var("SHORTED_WHISPER_MODEL").unwrap());
        let commands = crate::runtime::with_test_context(|task| {
            run_transcription(&project, Path::new("/"), &model, task)
        })
        .unwrap();
        std::fs::remove_file(&wav).unwrap();
        let [Command::SetTranscript {
            transcript: Some(transcript),
            ..
        }] = commands.as_slice()
        else {
            panic!("no transcript");
        };
        let words: Vec<_> = transcript
            .segments
            .iter()
            .flat_map(|segment| &segment.words)
            .map(|word| word.text.to_lowercase())
            .collect();
        println!("{words:?}");
        assert!(words.iter().any(|word| word.contains("video")));
    }

    /// Answers every upload with one word at 0.5 s, and counts the uploads.
    struct OneWord(std::cell::Cell<usize>);

    impl UploadHttp for OneWord {
        fn upload(
            &self,
            upload: &shorted_ai::speech::Upload,
        ) -> Result<shorted_ai::providers::HttpResponse, shorted_ai::providers::AiError> {
            assert!(upload.body.len() < 25 * 1024 * 1024, "over OpenAI's limit");
            self.0.set(self.0.get() + 1);
            Ok(shorted_ai::providers::HttpResponse {
                status: 200,
                body: r#"{"language":"french","words":[{"word":"oui","start":0.5,"end":0.75}]}"#
                    .into(),
            })
        }
    }

    #[test]
    fn long_sound_goes_in_parts_and_words_keep_their_time() {
        let http = OneWord(std::cell::Cell::new(0));
        let speech = vec![0.0; MAX_CHUNK_SAMPLES + 16_000];
        let (words, language) = hear_with_openai(&http, &Secret::new("k".into()), &speech).unwrap();
        assert_eq!(http.0.get(), 2);
        assert_eq!(language, "fr");
        assert_eq!(words[0].start_us, 500_000);
        assert_eq!(words[1].start_us, 600_000_000 + 500_000);
    }

    #[test]
    fn words_keep_their_place_in_the_source() {
        let transcript = to_transcript(vec![word("Hi", 10)], 5_000, "en".into());
        let first = &transcript.segments[0].words[0];
        assert_eq!((first.start_us, first.end_us), (5_010, 5_110));
        assert_eq!(first.original_text, "Hi");
        assert_eq!(transcript.language, "en");
    }

    #[test]
    fn voices_are_transcribed_and_music_is_not() {
        let json = include_str!("../../../../fixtures/projects/v1_full.json");
        let mut project = Project::from_json(json).unwrap();
        for clip in &mut project.timeline.clips {
            clip.transcript = None;
            clip.original_audio.transcribe = false;
        }
        for media in &mut project.media {
            media.probe.has_audio = true;
        }
        let mut item = project.timeline.audio[0].clone();
        item.transcript = None;
        item.role = AudioRole::Music;
        project.timeline.audio = vec![item.clone()];
        assert!(jobs(&project, false).is_empty());
        item.role = AudioRole::ImportedVoice;
        project.timeline.audio = vec![item];
        assert_eq!(jobs(&project, false).len(), 1);
    }
}
