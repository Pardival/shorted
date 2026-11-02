//! Projects as seen from Dart. One project is open at a time; all of its work runs on the command
//! thread (docs/05 §5). Dart receives view models, never project data to change.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::SystemTime;

use shorted_core::command::Command;
use shorted_core::evaluate::clip_positions;
use shorted_core::model::{AudioTrackRef, Id, Project};
use shorted_store::library::{self, ProjectEntry};
use shorted_store::session::{ProjectSession, SaveStatus, StoreError};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{EngineError, EngineFailure};
use crate::frb_generated::StreamSink;
use crate::runtime;

pub struct ProjectSummary {
    pub id: String,
    pub title: String,
    pub duration_us: i64,
    pub edited_at_ms: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrackKindView {
    Widgets,
    Video,
    Captions,
    VoiceOver,
    Audio,
}

pub struct TrackItemView {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
}

pub struct TrackView {
    pub kind: TrackKindView,
    /// Set for user audio tracks; the others get their name from the app's translations.
    pub name: Option<String>,
    pub items: Vec<TrackItemView>,
}

pub struct ProjectView {
    pub id: String,
    pub title: String,
    pub duration_us: i64,
    pub can_undo: bool,
    pub can_redo: bool,
    pub tracks: Vec<TrackView>,
}

#[derive(Clone, Copy)]
pub enum SaveStateView {
    Saving,
    Saved,
    Failed,
}

static ACTIVE: Mutex<Option<ProjectSession>> = Mutex::new(None);
static SAVE_EVENTS: Mutex<Option<StreamSink<SaveStateView>>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `work` on the command thread and waits. Called from bridge workers, never the UI isolate.
fn on_command_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, EngineError> + Send + 'static,
) -> Result<T, EngineFailure> {
    let (sender, receiver) = mpsc::sync_channel(1);
    runtime::global().commands.submit(move || {
        let _ = sender.send(work());
    })?;
    let outcome = receiver
        .recv()
        .map_err(|_| EngineError::Internal("command thread stopped".into()))?;
    Ok(outcome?)
}

fn parse_id(id: &str) -> Result<Id, EngineError> {
    id.parse()
        .map_err(|_| EngineError::Internal("bad project id".into()))
}

fn summary(entry: ProjectEntry) -> ProjectSummary {
    ProjectSummary {
        id: entry.id.to_string(),
        title: entry.title,
        duration_us: entry.duration_us,
        edited_at_ms: entry.edited_at_ms,
    }
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

fn save_channel() -> Sender<SaveStatus> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for status in receiver {
            let view = match status {
                SaveStatus::Saving => SaveStateView::Saving,
                SaveStatus::Saved => SaveStateView::Saved,
                SaveStatus::Failed => SaveStateView::Failed,
            };
            if let Some(sink) = lock(&SAVE_EVENTS).as_ref() {
                let _ = sink.add(view);
            }
        }
    });
    sender
}

fn close_active() -> Result<(), EngineError> {
    if let Some(session) = lock(&ACTIVE).take() {
        session.close()?;
    }
    Ok(())
}

/// The save state of the open project, for "Saved / Saving…". One listener at a time.
pub fn save_events(sink: StreamSink<SaveStateView>) {
    *lock(&SAVE_EVENTS) = Some(sink);
}

pub async fn list_projects(root: String) -> Result<Vec<ProjectSummary>, EngineFailure> {
    on_command_thread(move || {
        let root = PathBuf::from(root);
        library::purge_trash(&root, SystemTime::now()).map_err(EngineError::from)?;
        Ok(library::list_projects(&root)
            .map_err(EngineError::from)?
            .into_iter()
            .map(summary)
            .collect())
    })
}

pub async fn list_trash(root: String) -> Result<Vec<ProjectSummary>, EngineFailure> {
    on_command_thread(move || {
        Ok(library::list_trash(Path::new(&root))
            .map_err(EngineError::from)?
            .into_iter()
            .map(summary)
            .collect())
    })
}

/// Creates a project and opens it. Returns its id.
pub async fn create_project(root: String, title: String) -> Result<String, EngineFailure> {
    on_command_thread(move || {
        close_active()?;
        let project = Project::new(Uuid::now_v7(), title, now());
        let id = project.id.to_string();
        let session = ProjectSession::create(Path::new(&root), project, save_channel())
            .map_err(EngineError::from)?;
        *lock(&ACTIVE) = Some(session);
        Ok(id)
    })
}

pub async fn open_project(root: String, id: String) -> Result<(), EngineFailure> {
    on_command_thread(move || {
        let id = parse_id(&id)?;
        if lock(&ACTIVE)
            .as_ref()
            .is_some_and(|session| session.project().id == id)
        {
            return Ok(());
        }
        close_active()?;
        let session = ProjectSession::open(Path::new(&root), id, save_channel())
            .map_err(EngineError::from)?;
        *lock(&ACTIVE) = Some(session);
        Ok(())
    })
}

pub async fn close_project() -> Result<(), EngineFailure> {
    on_command_thread(close_active)
}

pub async fn duplicate_project(
    root: String,
    id: String,
    title: String,
) -> Result<String, EngineFailure> {
    on_command_thread(move || {
        let id = parse_id(&id)?;
        close_active()?;
        let copy = library::duplicate_project(Path::new(&root), id, Uuid::now_v7(), title)
            .map_err(EngineError::from)?;
        Ok(copy.id.to_string())
    })
}

pub async fn trash_project(root: String, id: String) -> Result<(), EngineFailure> {
    on_command_thread(move || {
        let id = parse_id(&id)?;
        close_active()?;
        library::trash_project(Path::new(&root), id).map_err(EngineError::from)
    })
}

pub async fn restore_project(root: String, id: String) -> Result<(), EngineFailure> {
    on_command_thread(move || {
        let id = parse_id(&id)?;
        library::restore_project(Path::new(&root), id).map_err(EngineError::from)
    })
}

/// Renames a project, open or not.
pub async fn rename_project(root: String, id: String, title: String) -> Result<(), EngineFailure> {
    on_command_thread(move || {
        let id = parse_id(&id)?;
        let command = Command::Rename { title };
        if lock(&ACTIVE)
            .as_ref()
            .is_some_and(|session| session.project().id == id)
        {
            with_active(|session| session.execute(&command).map(|_| ()))?;
            return Ok(());
        }
        let session = ProjectSession::open(Path::new(&root), id, save_channel());
        let mut session = session.map_err(EngineError::from)?;
        session.execute(&command).map_err(EngineError::from)?;
        session.close().map_err(EngineError::from)
    })
}

fn with_active<T>(
    work: impl FnOnce(&mut ProjectSession) -> Result<T, StoreError>,
) -> Result<T, EngineError> {
    let mut active = lock(&ACTIVE);
    let session = active
        .as_mut()
        .ok_or_else(|| EngineError::Internal("no project is open".into()))?;
    Ok(work(session)?)
}

fn track_view(session: &ProjectSession) -> Vec<TrackView> {
    let project = session.project();
    let timeline = &project.timeline;
    let item = |id: Id, start_us, end_us| TrackItemView {
        id: id.to_string(),
        start_us,
        end_us,
    };
    let mut overlays: Vec<TrackItemView> = timeline
        .overlays
        .iter()
        .map(|o| item(o.id, o.start_us, o.end_us))
        .collect();
    overlays.sort_by_key(|o| o.start_us);
    let audio_on = |track: AudioTrackRef| {
        let mut items: Vec<TrackItemView> = timeline
            .audio
            .iter()
            .filter(|a| a.track == track)
            .map(|a| item(a.id, a.start_us, a.end_us()))
            .collect();
        items.sort_by_key(|a| a.start_us);
        items
    };
    let mut tracks = vec![
        TrackView {
            kind: TrackKindView::Widgets,
            name: None,
            items: overlays,
        },
        TrackView {
            kind: TrackKindView::Video,
            name: None,
            items: clip_positions(project)
                .into_iter()
                .map(|(id, start, end)| item(id, start, end))
                .collect(),
        },
        TrackView {
            kind: TrackKindView::Captions,
            name: None,
            items: Vec::new(),
        },
        TrackView {
            kind: TrackKindView::VoiceOver,
            name: None,
            items: audio_on(AudioTrackRef::VoiceOver),
        },
    ];
    tracks.extend(timeline.audio_tracks.iter().map(|track| TrackView {
        kind: TrackKindView::Audio,
        name: Some(track.name.clone()),
        items: audio_on(AudioTrackRef::User(track.id)),
    }));
    tracks
}

fn view_of(session: &ProjectSession) -> ProjectView {
    let project = session.project();
    ProjectView {
        id: project.id.to_string(),
        title: project.title.clone(),
        duration_us: project.duration_us(),
        can_undo: session.can_undo(),
        can_redo: session.can_redo(),
        tracks: track_view(session),
    }
}

pub async fn project_view() -> Result<ProjectView, EngineFailure> {
    on_command_thread(|| with_active(|session| Ok(view_of(session))))
}

pub async fn undo() -> Result<ProjectView, EngineFailure> {
    on_command_thread(|| {
        with_active(|session| {
            session.undo()?;
            Ok(view_of(session))
        })
    })
}

pub async fn redo() -> Result<ProjectView, EngineFailure> {
    on_command_thread(|| {
        with_active(|session| {
            session.redo()?;
            Ok(view_of(session))
        })
    })
}
