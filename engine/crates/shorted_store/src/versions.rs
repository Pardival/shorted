//! Version history (FR-PRJ-04, docs/06 §4): full copies of the project in `snapshots/`, kept apart
//! from the autosave so that restoring one is an ordinary, undoable edit.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use shorted_core::error::ModelError;
use shorted_core::model::Project;
use shorted_core::validate::MAX_DOCUMENT_BYTES;

use crate::migrations::migrate;
use crate::session::{Snapshot, StoreError, write_atomic};

pub const VERSIONS_FOLDER: &str = "snapshots";
pub const VERSIONS_KEPT: usize = 20;

/// Why a version was kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionReason {
    /// After ten minutes of editing.
    Activity,
    /// Just before an AI change.
    BeforeAi,
    /// Just before another version was restored.
    BeforeRestore,
}

impl VersionReason {
    fn tag(self) -> &'static str {
        match self {
            Self::Activity => "auto",
            Self::BeforeAi => "ai",
            Self::BeforeRestore => "restore",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "auto" => Some(Self::Activity),
            "ai" => Some(Self::BeforeAi),
            "restore" => Some(Self::BeforeRestore),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionEntry {
    /// The file name: what `load_version` takes.
    pub name: String,
    pub saved_at_ms: u64,
    pub reason: VersionReason,
}

fn parse_name(name: &str) -> Option<VersionEntry> {
    let stem = name.strip_suffix(".json")?;
    let (ms, tag) = stem.split_once('-')?;
    Some(VersionEntry {
        name: name.to_string(),
        saved_at_ms: ms.parse().ok()?,
        reason: VersionReason::from_tag(tag)?,
    })
}

/// Newest first.
pub fn list_versions(folder: &Path) -> Vec<VersionEntry> {
    let Ok(entries) = fs::read_dir(folder.join(VERSIONS_FOLDER)) else {
        return Vec::new();
    };
    let mut versions: Vec<VersionEntry> = entries
        .flatten()
        .filter_map(|entry| parse_name(entry.file_name().to_str()?))
        .collect();
    versions.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms).then(b.name.cmp(&a.name)));
    versions
}

/// Writes a version, then removes the oldest beyond `VERSIONS_KEPT`.
pub fn write_version(
    folder: &Path,
    saved_at_ms: u64,
    reason: VersionReason,
    bytes: &[u8],
) -> Result<(), StoreError> {
    let directory = folder.join(VERSIONS_FOLDER);
    fs::create_dir_all(&directory)?;
    write_atomic(
        &directory.join(format!("{saved_at_ms}-{}.json", reason.tag())),
        bytes,
    )?;
    for old in list_versions(folder).into_iter().skip(VERSIONS_KEPT) {
        fs::remove_file(directory.join(old.name))?;
    }
    Ok(())
}

/// Reads a version, migrated and validated like a project file. Only names that
/// `list_versions` gives are accepted, so the name cannot point elsewhere.
pub fn load_version(folder: &Path, name: &str) -> Result<Project, StoreError> {
    if parse_name(name).is_none() || name.contains(['/', '\\']) {
        return Err(StoreError::Malformed);
    }
    let mut json = String::new();
    File::open(folder.join(VERSIONS_FOLDER).join(name))?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_string(&mut json)?;
    if json.len() > MAX_DOCUMENT_BYTES {
        return Err(ModelError::DocumentTooLarge {
            limit: MAX_DOCUMENT_BYTES,
        }
        .into());
    }
    let document = serde_json::from_str(&json).map_err(|_| StoreError::Malformed)?;
    let (document, _) = migrate(document)?;
    let snapshot: Snapshot<Project> =
        serde_json::from_value(document).map_err(|_| StoreError::Malformed)?;
    let project = snapshot.project;
    project.validate()?;
    Ok(project)
}
