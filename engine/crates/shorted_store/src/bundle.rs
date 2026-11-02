//! `.shorted` bundles (FR-PRJ-06, docs/06 §6): one ZIP holding `project.json`, `media/` and a
//! `manifest.json` of SHA-256 sums. Import follows docs/08 §3: nothing from the archive is used
//! before every path, size and sum has been checked.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shorted_core::model::{Id, MediaLocation, MediaRef, Project};
use shorted_core::validate::MAX_DOCUMENT_BYTES;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::migrations::migrate;
use crate::session::{PROJECT_FILE, Snapshot, project_folder, snapshot_bytes, write_atomic};

pub const BUNDLE_FORMAT: u32 = 1;
pub const BUNDLE_EXTENSION: &str = "shorted";
const MANIFEST_FILE: &str = "manifest.json";
const MEDIA_FOLDER: &str = "media";
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_FILES: usize = 2_000;
/// docs/08 §3: at most twice the archive, plus room for well-compressed text, and 5 GB.
const MAX_TOTAL_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const TEXT_SLACK_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("bundle file could not be read or written")]
    Io(#[from] io::Error),
    #[error("the bundle is unsafe: {0}")]
    Unsafe(&'static str),
    #[error("the bundle does not match its manifest")]
    Mismatch,
    #[error("the bundle is too large")]
    TooLarge,
    #[error("the bundle is not a Shorted project")]
    Malformed,
    #[error("a media file of the project is missing")]
    MissingMedia,
}

impl From<zip::result::ZipError> for BundleError {
    fn from(_: zip::result::ZipError) -> Self {
        Self::Malformed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub created_at: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.chars().filter(char::is_ascii_alphanumeric).collect())
        .unwrap_or_else(|| "bin".into())
}

/// Writes `project` and every media file into one bundle at `out`. Media are found by
/// `resolve`; inside the bundle they all live in `media/`, so the project needs nothing else.
pub fn export_bundle(
    project: &Project,
    resolve: impl Fn(&MediaRef) -> Option<PathBuf>,
    out: &Path,
    app_version: &str,
    created_at: &str,
) -> Result<(), BundleError> {
    let mut bundled = project.clone();
    let mut sources = Vec::new();
    for media in &mut bundled.media {
        let source = resolve(media).ok_or(BundleError::MissingMedia)?;
        let path = format!("{MEDIA_FOLDER}/{}.{}", media.id, extension(&source));
        media.location = MediaLocation::ProjectCopy {
            rel_path: path.clone(),
        };
        sources.push((path, source));
    }
    let temporary = out.with_extension("part");
    let result = write_bundle(&bundled, &sources, &temporary, app_version, created_at)
        .and_then(|()| fs::rename(&temporary, out).map_err(BundleError::from));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_bundle(
    project: &Project,
    sources: &[(String, PathBuf)],
    out: &Path,
    app_version: &str,
    created_at: &str,
) -> Result<(), BundleError> {
    let mut zip = ZipWriter::new(File::create(out)?);
    let text = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    // Media are compressed already: storing them is faster and loses nothing.
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true);
    let mut files = Vec::new();

    let document = snapshot_bytes(project, 0).map_err(|_| BundleError::Malformed)?;
    zip.start_file(PROJECT_FILE, text)?;
    zip.write_all(&document)?;
    files.push(ManifestFile {
        path: PROJECT_FILE.into(),
        sha256: sha256_hex(&document),
        size: document.len() as u64,
    });

    for (path, source) in sources {
        zip.start_file(path.as_str(), stored)?;
        let mut file = File::open(source)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 1024 * 1024];
        let mut size = 0;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            zip.write_all(&buffer[..read])?;
            size += read as u64;
        }
        files.push(ManifestFile {
            path: path.clone(),
            sha256: hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            size,
        });
    }

    let manifest = Manifest {
        format: BUNDLE_FORMAT,
        app_version: app_version.into(),
        created_at: created_at.into(),
        files,
    };
    zip.start_file(MANIFEST_FILE, text)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|_| BundleError::Malformed)?)?;
    zip.finish()?.sync_all()?;
    Ok(())
}

/// A path from the archive, only if it is plain and relative: no root, no `..`, no drive.
fn safe_path(name: &str) -> Result<PathBuf, BundleError> {
    if name.contains('\\') || name.contains('\0') {
        return Err(BundleError::Unsafe("bad characters in a path"));
    }
    let path = Path::new(name);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err(BundleError::Unsafe("a path leaves the bundle")),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(BundleError::Unsafe("an empty path"));
    }
    Ok(clean)
}

/// Unpacks a bundle as a new project named `new_id` under `root`, and returns it. On any
/// problem nothing is left behind.
pub fn import_bundle(root: &Path, bundle: &Path, new_id: Id) -> Result<Project, BundleError> {
    let archive_size = fs::metadata(bundle)?.len();
    let mut archive = ZipArchive::new(File::open(bundle)?)?;
    if archive.len() > MAX_FILES {
        return Err(BundleError::TooLarge);
    }

    // Every entry is checked before anything is written.
    let mut total = 0u64;
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        let name = entry.name()?.to_string();
        if entry.is_symlink() {
            return Err(BundleError::Unsafe("a symbolic link"));
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170_000;
            if kind != 0 && kind != 0o100_000 && kind != 0o040_000 {
                return Err(BundleError::Unsafe("a special file"));
            }
        }
        let path = safe_path(&name)?;
        if entry.is_dir() {
            continue;
        }
        total = total.saturating_add(entry.size());
        if entries.insert(path, index).is_some() {
            return Err(BundleError::Unsafe("a file appears twice"));
        }
    }
    let limit = archive_size
        .saturating_mul(2)
        .saturating_add(TEXT_SLACK_BYTES)
        .min(MAX_TOTAL_BYTES);
    if total > limit {
        return Err(BundleError::TooLarge);
    }

    let manifest_index = *entries
        .get(Path::new(MANIFEST_FILE))
        .ok_or(BundleError::Malformed)?;
    let manifest: Manifest = {
        let mut text = Vec::new();
        archive
            .by_index(manifest_index)?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut text)?;
        if text.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(BundleError::TooLarge);
        }
        serde_json::from_slice(&text).map_err(|_| BundleError::Malformed)?
    };
    if manifest.format != BUNDLE_FORMAT {
        return Err(BundleError::Malformed);
    }
    // The manifest names exactly the files there are, besides itself.
    let mut listed = HashMap::new();
    for file in &manifest.files {
        let path = safe_path(&file.path)?;
        if path == Path::new(MANIFEST_FILE) || listed.insert(path, file).is_some() {
            return Err(BundleError::Mismatch);
        }
    }
    if listed.len() + 1 != entries.len() || listed.keys().any(|p| !entries.contains_key(p)) {
        return Err(BundleError::Mismatch);
    }
    if !listed.contains_key(Path::new(PROJECT_FILE)) {
        return Err(BundleError::Malformed);
    }

    let folder = project_folder(root, new_id);
    if folder.exists() {
        return Err(BundleError::Unsafe("the project already exists"));
    }
    let staging = root.join("projects").join(format!(".import-{new_id}"));
    let result = unpack(&mut archive, &entries, &listed, &staging)
        .and_then(|()| finish(&staging, new_id))
        .and_then(|project| {
            fs::rename(&staging, &folder)?;
            Ok(project)
        });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn unpack(
    archive: &mut ZipArchive<File>,
    entries: &HashMap<PathBuf, usize>,
    listed: &HashMap<PathBuf, &ManifestFile>,
    staging: &Path,
) -> Result<(), BundleError> {
    fs::create_dir_all(staging)?;
    for (path, file) in listed {
        let index = entries[path];
        let mut entry = archive.by_index(index)?;
        let target = staging.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&target)?;
        let mut hasher = Sha256::new();
        // Never more than the manifest says: an entry lying about its size stops here.
        let mut limited = (&mut entry).take(file.size + 1);
        let mut buffer = vec![0; 1024 * 1024];
        let mut written = 0u64;
        loop {
            let read = limited.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            written += read as u64;
            if written > file.size {
                return Err(BundleError::Mismatch);
            }
            hasher.update(&buffer[..read]);
            out.write_all(&buffer[..read])?;
        }
        let sum: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if written != file.size || sum != file.sha256 {
            return Err(BundleError::Mismatch);
        }
    }
    Ok(())
}

/// Reads the unpacked project, gives it its new id, and checks its media stay inside it.
fn finish(staging: &Path, new_id: Id) -> Result<Project, BundleError> {
    let mut json = Vec::new();
    File::open(staging.join(PROJECT_FILE))?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut json)?;
    if json.len() > MAX_DOCUMENT_BYTES {
        return Err(BundleError::TooLarge);
    }
    let document = serde_json::from_slice(&json).map_err(|_| BundleError::Malformed)?;
    let (document, _) = migrate(document).map_err(|_| BundleError::Malformed)?;
    let snapshot: Snapshot<Project> =
        serde_json::from_value(document).map_err(|_| BundleError::Malformed)?;
    let mut project = snapshot.project;
    for media in &project.media {
        match &media.location {
            MediaLocation::ProjectCopy { rel_path } => {
                let path = safe_path(rel_path)?;
                if !staging.join(path).is_file() {
                    return Err(BundleError::MissingMedia);
                }
            }
            MediaLocation::External { .. } => {
                return Err(BundleError::Unsafe("media outside the bundle"));
            }
        }
    }
    project.id = new_id;
    project.validate().map_err(|_| BundleError::Malformed)?;
    write_atomic(
        &staging.join(PROJECT_FILE),
        &snapshot_bytes(&project, 0).map_err(|_| BundleError::Malformed)?,
    )?;
    fs::create_dir_all(staging.join("snapshots"))?;
    Ok(project)
}
