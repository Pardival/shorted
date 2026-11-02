//! Undo/redo, revisions, change events and gestures around one project.

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::error::ModelError;
use crate::model::Project;

/// Well above the 200 steps of FR-PRJ-03.
pub const MAX_UNDO_STEPS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangedArea {
    Project,
    Media,
    Clips,
    Transitions,
    Audio,
    Tracks,
    Overlays,
    Captions,
}

/// What the UI must refresh. `transient` updates come from a gesture in progress and are neither
/// journaled nor undoable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub revision: u64,
    pub changed: Vec<ChangedArea>,
    pub transient: bool,
}

fn changed_areas(before: &Project, after: &Project) -> Vec<ChangedArea> {
    let (old, new) = (&before.timeline, &after.timeline);
    [
        (
            before.title != after.title
                || before.language != after.language
                || before.canvas != after.canvas,
            ChangedArea::Project,
        ),
        (before.media != after.media, ChangedArea::Media),
        (old.clips != new.clips, ChangedArea::Clips),
        (old.transitions != new.transitions, ChangedArea::Transitions),
        (old.audio != new.audio, ChangedArea::Audio),
        (
            old.audio_tracks != new.audio_tracks || old.track_flags != new.track_flags,
            ChangedArea::Tracks,
        ),
        (old.overlays != new.overlays, ChangedArea::Overlays),
        (before.captions != after.captions, ChangedArea::Captions),
    ]
    .into_iter()
    .filter_map(|(changed, area)| changed.then_some(area))
    .collect()
}

#[derive(Debug)]
pub struct Editor {
    project: Project,
    revision: u64,
    undo: Vec<Command>,
    redo: Vec<Command>,
    /// The project as it was when the gesture started.
    gesture_base: Option<Project>,
}

impl Editor {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture_base: None,
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn event(&mut self, before: &Project, transient: bool) -> ChangeEvent {
        self.revision += 1;
        ChangeEvent {
            revision: self.revision,
            changed: changed_areas(before, &self.project),
            transient,
        }
    }

    /// Commits one edit: one undo step. Ends any gesture, starting again from its base.
    pub fn execute(&mut self, command: &Command) -> Result<ChangeEvent, ModelError> {
        let before = self
            .gesture_base
            .take()
            .unwrap_or_else(|| self.project.clone());
        let mut next = before.clone();
        let inverse = match command.apply(&mut next) {
            Ok(inverse) => inverse,
            Err(error) => {
                self.project = before;
                return Err(error);
            }
        };
        self.project = next;
        self.undo.push(inverse);
        if self.undo.len() > MAX_UNDO_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(self.event(&before, false))
    }

    /// Previews a gesture's current state. Each update starts from the gesture's base, so updates
    /// replace one another and nothing accumulates.
    pub fn preview(&mut self, command: &Command) -> Result<ChangeEvent, ModelError> {
        let base = self
            .gesture_base
            .get_or_insert_with(|| self.project.clone());
        let mut next = base.clone();
        command.apply(&mut next)?;
        let before = std::mem::replace(&mut self.project, next);
        Ok(self.event(&before, true))
    }

    /// Abandons a gesture and restores the project from before it.
    pub fn cancel_gesture(&mut self) -> Result<ChangeEvent, ModelError> {
        let base = self.gesture_base.take().ok_or(ModelError::NoGesture)?;
        let before = std::mem::replace(&mut self.project, base);
        Ok(self.event(&before, true))
    }

    pub fn undo(&mut self) -> Result<Option<ChangeEvent>, ModelError> {
        self.step(true)
    }

    pub fn redo(&mut self) -> Result<Option<ChangeEvent>, ModelError> {
        self.step(false)
    }

    fn step(&mut self, undo: bool) -> Result<Option<ChangeEvent>, ModelError> {
        if self.gesture_base.is_some() {
            self.cancel_gesture()?;
        }
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(command) = from.pop() else {
            return Ok(None);
        };
        let before = self.project.clone();
        match command.apply(&mut self.project) {
            Ok(inverse) => to.push(inverse),
            Err(error) => {
                from.push(command);
                return Err(error);
            }
        }
        Ok(Some(self.event(&before, false)))
    }
}
