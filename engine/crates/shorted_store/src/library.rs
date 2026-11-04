//! The list of projects on disk: summaries, duplication and the 7-day trash (FR-HOME-05).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use shorted_core::model::{Id, Project};

use crate::session::{
    JOURNAL_FILE, PROJECT_FILE, StoreError, load, project_folder, snapshot_bytes, write_atomic,
};

pub const TRASH_KEPT_FOR: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEntry {
    pub id: Id,
    pub title: String,
    pub duration_us: i64,
    /// From the file times: they also cover edits only in the journal.
    pub edited_at_ms: i64,
}

fn trash_folder(root: &Path, id: Id) -> PathBuf {
    root.join("trash").join(id.to_string())
}

fn ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn modified_ms(path: &Path) -> i64 {
    fs::metadata(path).and_then(|m| m.modified()).map_or(0, ms)
}

fn entries(parent: &Path) -> io::Result<Vec<(Id, PathBuf)>> {
    let listing = match fs::read_dir(parent) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    for entry in listing {
        let path = entry?.path();
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.parse().ok());
        if let Some(id) = id {
            found.push((id, path));
        }
    }
    Ok(found)
}

fn summarise(id: Id, folder: &Path) -> Option<ProjectEntry> {
    // A damaged project must not hide the others.
    let (project, _) = load(folder)
        .inspect_err(|error| tracing::warn!(%error, "project skipped in the list"))
        .ok()?;
    Some(ProjectEntry {
        id,
        title: project.title.clone(),
        duration_us: project.duration_us(),
        edited_at_ms: modified_ms(&folder.join(PROJECT_FILE))
            .max(modified_ms(&folder.join(JOURNAL_FILE))),
    })
}

fn list_in(parent: &Path) -> Result<Vec<ProjectEntry>, StoreError> {
    let mut list: Vec<ProjectEntry> = entries(parent)?
        .iter()
        .filter_map(|(id, folder)| summarise(*id, folder))
        .collect();
    list.sort_by_key(|entry| std::cmp::Reverse(entry.edited_at_ms));
    Ok(list)
}

/// Projects by last edit, newest first.
pub fn list_projects(root: &Path) -> Result<Vec<ProjectEntry>, StoreError> {
    list_in(&root.join("projects"))
}

pub fn list_trash(root: &Path) -> Result<Vec<ProjectEntry>, StoreError> {
    list_in(&root.join("trash"))
}

pub fn trash_project(root: &Path, id: Id) -> Result<(), StoreError> {
    let target = trash_folder(root, id);
    fs::create_dir_all(root.join("trash"))?;
    fs::rename(project_folder(root, id), &target)?;
    // The folder's own time is what `purge_trash` compares.
    fs::write(target.join(".trashed"), [])?;
    Ok(())
}

pub fn restore_project(root: &Path, id: Id) -> Result<(), StoreError> {
    let source = trash_folder(root, id);
    fs::rename(&source, project_folder(root, id))?;
    let _ = fs::remove_file(project_folder(root, id).join(".trashed"));
    Ok(())
}

/// Permanently removes projects trashed more than 7 days before `now`.
pub fn purge_trash(root: &Path, now: SystemTime) -> Result<usize, StoreError> {
    let mut removed = 0;
    for (_, folder) in entries(&root.join("trash"))? {
        let trashed = fs::metadata(folder.join(".trashed")).and_then(|m| m.modified())?;
        if now
            .duration_since(trashed)
            .is_ok_and(|age| age > TRASH_KEPT_FOR)
        {
            fs::remove_dir_all(folder)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        // Project folders hold no links: anything else than a folder or file is left out.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copies a project, media included, under a new id and title.
/// A closed project as it was last saved, journal included.
pub fn load_project(root: &Path, id: Id) -> Result<Project, StoreError> {
    load(&project_folder(root, id)).map(|(project, _)| project)
}

pub fn duplicate_project(
    root: &Path,
    id: Id,
    new_id: Id,
    title: String,
) -> Result<Project, StoreError> {
    let source = project_folder(root, id);
    let (mut project, _) = load(&source)?;
    project.id = new_id;
    project.title = title;
    project.validate()?;
    let target = project_folder(root, new_id);
    copy_folder(&source, &target)?;
    write_atomic(&target.join(PROJECT_FILE), &snapshot_bytes(&project, 0)?)?;
    fs::write(target.join(JOURNAL_FILE), [])?;
    Ok(project)
}
