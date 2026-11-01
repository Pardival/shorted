//! One open project on disk (ADR-0005). Edits go to `journal.jsonl` from a background thread, so
//! nothing here blocks the caller; `project.json` is rewritten atomically from time to time.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use shorted_core::command::Command;
use shorted_core::editor::{ChangeEvent, Editor};
use shorted_core::error::ModelError;
use shorted_core::model::{Id, Project};
use shorted_core::validate::{MAX_DOCUMENT_BYTES, MAX_JOURNAL_LINE_BYTES};
use thiserror::Error;

use crate::migrations::migrate;

pub const PROJECT_FILE: &str = "project.json";
pub const JOURNAL_FILE: &str = "journal.jsonl";
pub const SNAPSHOT_EVERY_COMMANDS: u32 = 50;
pub const SNAPSHOT_EVERY: Duration = Duration::from_secs(30);
/// Below the 1 s of NFR-REL-01, leaving room for a slow disk.
const FLUSH_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("project file could not be read or written")]
    Io(#[from] io::Error),
    #[error("project is invalid")]
    Model(#[from] ModelError),
    #[error("project file is malformed")]
    Malformed,
    #[error("the save thread stopped")]
    WriterStopped,
    #[error("this project was saved by a newer version of the app")]
    NewerVersion { found: u32 },
}

/// Writes through a temporary file: a crash leaves the old file or the new one, never a mix.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    sync_directory(path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Snapshot<T> {
    revision: u64,
    #[serde(flatten)]
    project: T,
}

#[derive(Serialize, Deserialize)]
struct JournalLine<C> {
    rev: u64,
    ts: u64,
    cmd: C,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

pub(crate) fn snapshot_bytes(project: &Project, revision: u64) -> Result<Vec<u8>, StoreError> {
    let snapshot = Snapshot { revision, project };
    serde_json::to_vec(&snapshot).map_err(|_| StoreError::Malformed)
}

/// What the "Saved / Saving…" indicator shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Saving,
    Saved,
    Failed,
}

enum WriterMessage {
    Append(Vec<u8>),
    Snapshot { bytes: Vec<u8> },
    Close,
}

/// Through a separate handle: on Windows an append-only one cannot truncate.
fn empty_journal(folder: &Path) -> io::Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .open(folder.join(JOURNAL_FILE))?;
    file.set_len(0)?;
    file.sync_data()
}

/// Runs on its own thread: the UI path only queues bytes (NFR-PERF-11).
fn writer_loop(
    folder: PathBuf,
    mut journal: File,
    messages: mpsc::Receiver<WriterMessage>,
    status: Sender<SaveStatus>,
) {
    let mut unsynced_since: Option<Instant> = None;
    let report = |value| {
        // The UI may be gone while closing; saving goes on.
        let _ = status.send(value);
    };
    let sync = |journal: &File, pending: &mut Option<Instant>| {
        if pending.take().is_some() {
            match journal.sync_data() {
                Ok(()) => report(SaveStatus::Saved),
                Err(error) => {
                    tracing::error!(%error, "journal sync failed");
                    report(SaveStatus::Failed);
                }
            }
        }
    };
    loop {
        let wait = unsynced_since.map_or(Duration::from_secs(3600), |since| {
            FLUSH_DELAY.saturating_sub(since.elapsed())
        });
        match messages.recv_timeout(wait) {
            Ok(WriterMessage::Append(line)) => {
                report(SaveStatus::Saving);
                // In the OS at once: this already survives the app being killed.
                match journal.write_all(&line) {
                    Ok(()) => {
                        unsynced_since.get_or_insert_with(Instant::now);
                    }
                    Err(error) => {
                        tracing::error!(%error, "journal write failed");
                        report(SaveStatus::Failed);
                    }
                }
            }
            Ok(WriterMessage::Snapshot { bytes }) => {
                sync(&journal, &mut unsynced_since);
                let written = write_atomic(&folder.join(PROJECT_FILE), &bytes)
                    .and_then(|()| empty_journal(&folder));
                if let Err(error) = written {
                    // The journal stays valid: lines older than the snapshot are skipped on open.
                    tracing::error!(%error, "snapshot failed");
                    report(SaveStatus::Failed);
                }
            }
            Ok(WriterMessage::Close) | Err(RecvTimeoutError::Disconnected) => {
                sync(&journal, &mut unsynced_since);
                return;
            }
            Err(RecvTimeoutError::Timeout) => sync(&journal, &mut unsynced_since),
        }
    }
}

/// Restores a project: the snapshot, then journal lines newer than it. A torn last line, left by
/// a crash mid-write, is ignored.
pub(crate) fn load(folder: &Path) -> Result<(Project, u64), StoreError> {
    let mut json = String::new();
    File::open(folder.join(PROJECT_FILE))?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_string(&mut json)?;
    if json.len() > MAX_DOCUMENT_BYTES {
        return Err(ModelError::DocumentTooLarge {
            limit: MAX_DOCUMENT_BYTES,
        }
        .into());
    }
    let document = serde_json::from_str(&json).map_err(|_| StoreError::Malformed)?;
    let (document, migrated_from) = migrate(document)?;
    if let Some(version) = migrated_from {
        // The original stays usable if this version of the app turns out to be wrong.
        fs::copy(
            folder.join(PROJECT_FILE),
            folder.join(format!("project.v{version}.json")),
        )?;
    }
    let snapshot: Snapshot<Project> =
        serde_json::from_value(document).map_err(|_| StoreError::Malformed)?;
    let migrated = migrated_from.is_some();
    let mut project = snapshot.project;
    project.validate()?;
    let mut revision = snapshot.revision;

    let journal = match File::open(folder.join(JOURNAL_FILE)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((project, revision)),
        Err(error) => return Err(error.into()),
    };
    let mut reader = BufReader::new(journal);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .by_ref()
            .take(MAX_JOURNAL_LINE_BYTES as u64)
            .read_line(&mut line)?;
        if read == 0 {
            break;
        }
        // A bad line ends the replay: later commands were built on what it did.
        let Ok(entry) = serde_json::from_str::<JournalLine<Command>>(&line) else {
            tracing::warn!("journal replay stopped at an unreadable line");
            break;
        };
        if entry.rev <= revision {
            continue;
        }
        if migrated {
            // Commands of an older schema cannot be trusted against a migrated project.
            tracing::warn!("journal of an older schema was not replayed");
            break;
        }
        if entry.cmd.apply(&mut project).is_err() {
            tracing::warn!("journal replay stopped at an invalid command");
            break;
        }
        revision = entry.rev;
    }
    Ok((project, revision))
}

pub struct ProjectSession {
    editor: Editor,
    revision: u64,
    commands_since_snapshot: u32,
    last_snapshot: Instant,
    sender: Sender<WriterMessage>,
    writer: Option<JoinHandle<()>>,
}

impl ProjectSession {
    fn start(
        folder: PathBuf,
        project: Project,
        revision: u64,
        status: Sender<SaveStatus>,
    ) -> Result<Self, StoreError> {
        let journal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join(JOURNAL_FILE))?;
        let (sender, receiver) = mpsc::channel();
        let writer = thread::Builder::new()
            .name("shorted-journal".into())
            .spawn(move || writer_loop(folder, journal, receiver, status))?;
        Ok(Self {
            editor: Editor::new(project),
            revision,
            commands_since_snapshot: 0,
            last_snapshot: Instant::now(),
            sender,
            writer: Some(writer),
        })
    }

    /// Creates the project folder and its first snapshot, in `root`/`id`.
    pub fn create(
        root: &Path,
        project: Project,
        status: Sender<SaveStatus>,
    ) -> Result<Self, StoreError> {
        let folder = project_folder(root, project.id);
        fs::create_dir_all(folder.join("media"))?;
        fs::create_dir_all(folder.join("snapshots"))?;
        write_atomic(&folder.join(PROJECT_FILE), &snapshot_bytes(&project, 0)?)?;
        Self::start(folder, project, 0, status)
    }

    pub fn open(root: &Path, id: Id, status: Sender<SaveStatus>) -> Result<Self, StoreError> {
        let folder = project_folder(root, id);
        let (project, revision) = load(&folder)?;
        Self::start(folder, project, revision, status)
    }

    pub fn project(&self) -> &Project {
        self.editor.project()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    pub fn execute(&mut self, command: &Command) -> Result<ChangeEvent, StoreError> {
        let event = self.editor.execute(command)?;
        self.record(command)?;
        Ok(event)
    }

    /// A gesture update: shown, never saved.
    pub fn preview(&mut self, command: &Command) -> Result<ChangeEvent, StoreError> {
        Ok(self.editor.preview(command)?)
    }

    pub fn undo(&mut self) -> Result<Option<ChangeEvent>, StoreError> {
        let Some((event, command)) = self.editor.undo()? else {
            return Ok(None);
        };
        self.record(&command)?;
        Ok(Some(event))
    }

    pub fn redo(&mut self) -> Result<Option<ChangeEvent>, StoreError> {
        let Some((event, command)) = self.editor.redo()? else {
            return Ok(None);
        };
        self.record(&command)?;
        Ok(Some(event))
    }

    fn record(&mut self, command: &Command) -> Result<(), StoreError> {
        self.revision += 1;
        let entry = JournalLine {
            rev: self.revision,
            ts: now_ms(),
            cmd: command,
        };
        let mut line = serde_json::to_vec(&entry).map_err(|_| StoreError::Malformed)?;
        line.push(b'\n');
        self.send(WriterMessage::Append(line))?;
        self.commands_since_snapshot += 1;
        if self.commands_since_snapshot >= SNAPSHOT_EVERY_COMMANDS
            || self.last_snapshot.elapsed() >= SNAPSHOT_EVERY
        {
            self.snapshot()?;
        }
        Ok(())
    }

    fn snapshot(&mut self) -> Result<(), StoreError> {
        let bytes = snapshot_bytes(self.editor.project(), self.revision)?;
        self.send(WriterMessage::Snapshot { bytes })?;
        self.commands_since_snapshot = 0;
        self.last_snapshot = Instant::now();
        Ok(())
    }

    fn send(&self, message: WriterMessage) -> Result<(), StoreError> {
        self.sender
            .send(message)
            .map_err(|_| StoreError::WriterStopped)
    }

    /// Writes a final snapshot and waits for the disk.
    pub fn close(mut self) -> Result<(), StoreError> {
        self.snapshot()?;
        self.shut_down();
        Ok(())
    }

    fn shut_down(&mut self) {
        if let Some(writer) = self.writer.take() {
            let _ = self.sender.send(WriterMessage::Close);
            if writer.join().is_err() {
                tracing::error!("save thread panicked");
            }
        }
    }
}

impl Drop for ProjectSession {
    fn drop(&mut self) {
        self.shut_down();
    }
}

pub fn project_folder(root: &Path, id: Id) -> PathBuf {
    root.join("projects").join(id.to_string())
}
