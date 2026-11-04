//! Files in a project: classification, content hash, copies into the project folder.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use shorted_core::model::{Id, MediaKind};

const HASHED_BYTES: u64 = 4 * 1024 * 1024;

/// Decides from the extension. Real content is checked by probing.
pub fn classify(path: &Path) -> Option<MediaKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "mov" | "mp4" | "m4v" | "webm" | "mkv" => Some(MediaKind::Video),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "webp" => Some(MediaKind::Image),
        "gif" => Some(MediaKind::AnimatedImage),
        "m4a" | "aac" | "mp3" | "wav" | "flac" | "ogg" | "opus" => Some(MediaKind::Audio),
        _ => None,
    }
}

/// BLAKE3 of the first 4 MB and the size (docs/06 §3): fast on large videos.
pub fn content_hash(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    io::copy(&mut file.take(HASHED_BYTES), &mut hasher)?;
    hasher.update(&size.to_le_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

/// Copies a file into `folder/media` and returns its path relative to `folder`. The original name
/// is not kept: it is untrusted, and only the extension is needed.
pub fn copy_into_project(folder: &Path, source: &Path, id: Id) -> io::Result<String> {
    let extension: String = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let relative = format!("media/{id}.{extension}");
    fs::create_dir_all(folder.join("media"))?;
    fs::copy(source, folder.join(&relative))?;
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn classification_ignores_case_and_refuses_the_rest() {
        assert_eq!(classify(Path::new("a.MOV")), Some(MediaKind::Video));
        assert_eq!(classify(Path::new("a.HeIc")), Some(MediaKind::Image));
        assert_eq!(classify(Path::new("a.gif")), Some(MediaKind::AnimatedImage));
        assert_eq!(classify(Path::new("a.txt")), None);
        assert_eq!(classify(Path::new("noextension")), None);
    }

    #[test]
    fn hash_covers_content_and_size_beyond_the_hashed_part() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.bin");
        let mut bytes = vec![7u8; HASHED_BYTES as usize + 10];
        fs::write(&path, &bytes).unwrap();
        let first = content_hash(&path).unwrap();
        assert_eq!(first, content_hash(&path).unwrap());
        // Beyond 4 MB, same size: same hash, by design.
        bytes[HASHED_BYTES as usize + 5] = 8;
        fs::write(&path, &bytes).unwrap();
        assert_eq!(first, content_hash(&path).unwrap());
        bytes.push(0);
        fs::write(&path, &bytes).unwrap();
        assert_ne!(first, content_hash(&path).unwrap());
    }

    #[test]
    fn a_copy_uses_the_id_and_a_clean_extension() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("my clip.m/ov".replace('/', ""));
        fs::write(&source, b"x").unwrap();
        let id = Uuid::from_u128(3);
        let relative = copy_into_project(folder.path(), &source, id).unwrap();
        assert_eq!(relative, format!("media/{id}.mov"));
        assert!(folder.path().join(relative).exists());
    }
}
