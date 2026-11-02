//! Project document, schema version 1 (docs/06 §3). Times are microseconds, space is canvas units.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ModelError;

pub const SCHEMA_VERSION: u32 = 1;
pub const CANVAS_WIDTH: u32 = 1080;
pub const CANVAS_HEIGHT: u32 = 1920;
/// Export is blocked above this (FR-PRJ-07); a longer project still opens, so it can be fixed.
pub const MAX_PROJECT_DURATION_MS: i64 = 120_000;

pub type Id = Uuid;
/// RFC 3339 UTC time. Set by the engine: `shorted_core` reads no clock.
pub type DateTime = String;
/// `#RRGGBBAA`.
pub type Color = String;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: Id,
    pub title: String,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    pub canvas: Canvas,
    pub media: Vec<MediaRef>,
    pub timeline: Timeline,
    pub captions: CaptionSettings,
    pub language: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps_hint: u32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: CANVAS_WIDTH,
            height: CANVAS_HEIGHT,
            fps_hint: 30,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaRef {
    pub id: Id,
    pub kind: MediaKind,
    pub location: MediaLocation,
    pub content_hash: String,
    pub probe: MediaProbe,
    pub captured_at: Option<DateTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Image,
    AnimatedImage,
    Audio,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaLocation {
    ProjectCopy {
        rel_path: String,
    },
    External {
        abs_path: String,
        bookmark: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaProbe {
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub fps: f32,
    pub has_audio: bool,
    pub codec: String,
    pub color: ColorRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorRange {
    Sdr,
    Hdr,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub clips: Vec<ClipItem>,
    pub transitions: Vec<Transition>,
    pub audio: Vec<AudioItem>,
    pub audio_tracks: Vec<AudioTrack>,
    pub overlays: Vec<OverlayItem>,
    pub track_flags: TrackFlagsSet,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackFlags {
    pub muted: bool,
    pub locked: bool,
    pub hidden: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackFlagsSet {
    pub video: TrackFlags,
    pub widgets: TrackFlags,
    pub captions: TrackFlags,
    pub voice_over: TrackFlags,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub id: Id,
    pub name: String,
    pub flags: TrackFlags,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipItem {
    pub id: Id,
    pub media_id: Id,
    pub source_in_us: i64,
    pub source_out_us: i64,
    pub speed: f32,
    pub crop: Crop,
    pub color: ColorAdjust,
    pub image_motion: Option<KenBurns>,
    pub original_audio: ClipAudio,
    pub transcript: Option<Transcript>,
}

impl ClipItem {
    /// Length on the timeline, once speed is applied.
    pub fn duration_us(&self) -> i64 {
        ((self.source_out_us - self.source_in_us) as f64 / f64::from(self.speed)).round() as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub center: Vec2,
    pub scale: f32,
    pub rotation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorAdjust {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct KenBurns {
    pub start_scale: f32,
    pub end_scale: f32,
    pub start_center: Vec2,
    pub end_center: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipAudio {
    pub volume: f32,
    pub muted: bool,
    pub fade_in_us: i64,
    pub fade_out_us: i64,
    pub transcribe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub left: Id,
    pub right: Id,
    pub kind: TransitionKind,
    pub duration_us: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    Cut,
    Crossfade,
    DipToBlack,
    DipToWhite,
    Slide,
    Push,
    ZoomIn,
    ZoomOut,
    WhipPan,
    Spin,
    Glitch,
    Flash,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioItem {
    pub id: Id,
    pub media_id: Id,
    pub role: AudioRole,
    pub track: AudioTrackRef,
    pub start_us: i64,
    pub source_in_us: i64,
    pub source_out_us: i64,
    pub volume: f32,
    pub fade_in_us: i64,
    pub fade_out_us: i64,
    #[serde(rename = "loop")]
    pub loop_: bool,
    pub ducking: Option<Ducking>,
    pub transcript: Option<Transcript>,
}

impl AudioItem {
    pub fn end_us(&self) -> i64 {
        self.start_us + self.source_out_us - self.source_in_us
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRole {
    VoiceOver,
    ImportedVoice,
    Music,
    SoundEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "track_id", rename_all = "snake_case")]
pub enum AudioTrackRef {
    VoiceOver,
    User(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ducking {
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub language: String,
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub id: Id,
    pub words: Vec<Word>,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub original_text: String,
    pub start_us: i64,
    pub end_us: i64,
    pub confidence: f32,
    pub emphasis: EmphasisState,
    pub deleted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "on", rename_all = "snake_case")]
pub enum EmphasisState {
    Auto(bool),
    User(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptionSettings {
    pub enabled: bool,
    pub sources: Vec<CaptionSource>,
    pub preset: CaptionPreset,
    pub style: TextStyle,
    pub emphasis_color: Color,
    pub grouping: Grouping,
    pub animation: CaptionAnimation,
    pub vertical_anchor: f32,
}

impl Default for CaptionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sources: vec![CaptionSource::OriginalSound],
            preset: CaptionPreset::WordByWord,
            style: TextStyle::default(),
            emphasis_color: "#C6F135FF".into(),
            grouping: Grouping::OneWord,
            animation: CaptionAnimation::Pop,
            vertical_anchor: 0.7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum CaptionSource {
    OriginalSound,
    AudioItem(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionPreset {
    WordByWord,
    WordGroups,
    Sentence,
    Karaoke,
    Outline,
    Box,
    Bold,
    Minimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "words", rename_all = "snake_case")]
pub enum Grouping {
    OneWord,
    UpTo(u8),
    Sentence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionAnimation {
    None,
    Pop,
    Fade,
    Typewriter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font: String,
    pub size: f32,
    pub color: Color,
    pub uppercase: bool,
    pub decoration: TextDecoration,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: "Space Grotesk".into(),
            size: 96.0,
            color: "#FFFFFFFF".into(),
            uppercase: true,
            decoration: TextDecoration::Plain,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDecoration {
    Plain,
    Chip,
    Outline,
    Shadow,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OverlayItem {
    pub id: Id,
    pub kind: OverlayKind,
    pub start_us: i64,
    pub end_us: i64,
    pub base: Transform,
    pub keyframes: Vec<AnimatedProperty>,
    pub path: Option<MotionPath>,
    pub anim_in: Option<PresetAnim>,
    pub anim_loop: Option<PresetAnim>,
    pub anim_out: Option<PresetAnim>,
    pub locked: bool,
    pub hidden: bool,
    /// Pictures only: drawn in this one colour, keeping their shape (T-1205).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Color>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayKind {
    Image {
        asset: Id,
    },
    AnimatedImage {
        asset: Id,
    },
    Video {
        media: Id,
        #[serde(default)]
        source_in_us: i64,
    },
    Text {
        content: String,
        style: TextStyle,
    },
    Shape {
        shape: ShapeKind,
        fill: Color,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Arrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec2,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub flip: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimatedProperty {
    pub property: Prop,
    pub keys: Vec<Key>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prop {
    X,
    Y,
    Scale,
    Rotation,
    Opacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub t_us: i64,
    pub value: f32,
    pub easing: Easing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Spring,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionPath {
    pub samples: Vec<PathSample>,
    pub smoothing: f32,
    pub orient_to_path: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathSample {
    pub t_us: i64,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetAnim {
    pub kind: PresetKind,
    pub duration_us: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    Pop,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Fade,
    Zoom,
    Stamp,
    Wipe,
    Shrink,
    Float,
    Pulse,
    Wiggle,
    Rotate,
    Bounce,
}

impl Project {
    /// An empty project. Id and times come from the caller: this crate has no clock.
    pub fn new(id: Id, title: String, now: DateTime) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            title,
            created_at: now.clone(),
            updated_at: now,
            canvas: Canvas::default(),
            media: Vec::new(),
            timeline: Timeline::default(),
            captions: CaptionSettings::default(),
            language: None,
        }
    }

    /// Sum of clip lengths minus transition overlaps.
    pub fn duration_us(&self) -> i64 {
        let clips: i64 = self.timeline.clips.iter().map(ClipItem::duration_us).sum();
        let overlaps: i64 = self
            .timeline
            .transitions
            .iter()
            .map(|t| t.duration_us)
            .sum();
        clips - overlaps
    }

    /// Parses and validates a schema version 1 document. Never panics on bad input.
    pub fn from_json(json: &str) -> Result<Self, ModelError> {
        if json.len() > crate::validate::MAX_DOCUMENT_BYTES {
            return Err(ModelError::DocumentTooLarge {
                limit: crate::validate::MAX_DOCUMENT_BYTES,
            });
        }
        let project: Self =
            serde_json::from_str(json).map_err(|error| ModelError::Malformed(error.to_string()))?;
        project.validate()?;
        Ok(project)
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        serde_json::to_string(self).map_err(|error| ModelError::Malformed(error.to_string()))
    }
}
