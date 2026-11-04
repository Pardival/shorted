//! `.shorted` bundles (T-0108): a round trip keeps the project whole, and every unsafe or
//! inconsistent archive of docs/08 §3 is refused without leaving anything behind.

#![allow(clippy::unwrap_used)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use shorted_core::model::*;
use shorted_store::bundle::*;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

fn sum(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A project with one picture, copied in its folder.
fn project_with_picture(folder: &Path) -> Project {
    fs::create_dir_all(folder.join("media")).unwrap();
    fs::write(folder.join("media/logo.png"), b"not really a png").unwrap();
    let mut project = Project::new(id(1), "Trip".into(), "2026-11-02T10:00:00Z".into());
    project.media.push(MediaRef {
        id: id(2),
        kind: MediaKind::Image,
        location: MediaLocation::ProjectCopy {
            rel_path: "media/logo.png".into(),
        },
        content_hash: "abc".into(),
        probe: MediaProbe {
            duration_us: 3_000_000,
            width: 10,
            height: 10,
            rotation: 0,
            fps: 0.0,
            has_audio: false,
            codec: String::new(),
            color: ColorRange::Sdr,
        },
        captured_at: None,
        name: None,
    });
    project.validate().unwrap();
    project
}

fn export(project: &Project, folder: &Path, out: &Path) {
    export_bundle(
        project,
        |media| match &media.location {
            MediaLocation::ProjectCopy { rel_path } => Some(folder.join(rel_path)),
            MediaLocation::External { .. } => None,
        },
        out,
        "0.1.0",
        "2026-11-02T10:00:00Z",
    )
    .unwrap();
}

#[test]
fn a_round_trip_keeps_the_project_and_its_media() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let project = project_with_picture(source.path());
    let bundle = source.path().join("trip.shorted");
    export(&project, source.path(), &bundle);
    assert!(!source.path().join("trip.part").exists());

    let imported = import_bundle(target.path(), &bundle, id(9)).unwrap();
    assert_eq!(imported.id, id(9));
    assert_eq!(imported.title, "Trip");
    assert_eq!(imported.timeline, project.timeline);
    let MediaLocation::ProjectCopy { rel_path } = &imported.media[0].location else {
        panic!("media outside the project");
    };
    let folder = target.path().join("projects").join(id(9).to_string());
    assert_eq!(
        fs::read(folder.join(rel_path)).unwrap(),
        b"not really a png"
    );
    assert!(folder.join("project.json").is_file());
}

/// Builds an archive from `files`, with a manifest of `listed` (path, bytes) entries.
fn craft(out: &Path, files: &[(&str, &[u8])], listed: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(out).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (path, bytes) in files {
        zip.start_file(*path, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    let manifest = Manifest {
        format: BUNDLE_FORMAT,
        app_version: "0.1.0".into(),
        created_at: "now".into(),
        files: listed
            .iter()
            .map(|(path, bytes)| ManifestFile {
                path: (*path).into(),
                sha256: sum(bytes),
                size: bytes.len() as u64,
            })
            .collect(),
    };
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.finish().unwrap();
}

fn project_json() -> Vec<u8> {
    let project = Project::new(id(1), "x".into(), "2026-11-02T10:00:00Z".into());
    let mut value = serde_json::to_value(&project).unwrap();
    value["revision"] = 0.into();
    serde_json::to_vec(&value).unwrap()
}

fn refused(build: impl FnOnce(&Path)) -> BundleError {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("evil.shorted");
    build(&bundle);
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("projects")).unwrap();
    let error = import_bundle(&root, &bundle, id(5)).unwrap_err();
    let left: Vec<PathBuf> = fs::read_dir(root.join("projects"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(left.is_empty(), "left behind: {left:?}");
    assert!(!dir.path().join("evil.txt").exists());
    error
}

#[test]
fn a_well_formed_crafted_bundle_imports() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("ok.shorted");
    let json = project_json();
    craft(
        &bundle,
        &[("project.json", &json)],
        &[("project.json", &json)],
    );
    assert!(import_bundle(dir.path(), &bundle, id(5)).is_ok());
}

#[test]
fn zip_slip_and_absolute_paths_are_refused() {
    let json = project_json();
    for evil in ["../evil.txt", "media/../../evil.txt", "/evil.txt"] {
        let error = refused(|bundle| {
            craft(
                bundle,
                &[("project.json", &json), (evil, b"x")],
                &[("project.json", &json), (evil, b"x")],
            );
        });
        assert!(matches!(error, BundleError::Unsafe(_)), "{evil}: {error:?}");
    }
}

#[test]
fn symlinks_are_refused() {
    let json = project_json();
    let error = refused(|bundle| {
        let mut zip = ZipWriter::new(File::create(bundle).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file("project.json", options).unwrap();
        zip.write_all(&json).unwrap();
        zip.add_symlink("media/link", "/etc/passwd", options)
            .unwrap();
        zip.finish().unwrap();
    });
    assert!(matches!(error, BundleError::Unsafe(_)), "{error:?}");
}

#[test]
fn a_manifest_that_does_not_match_is_refused() {
    let json = project_json();
    // A changed byte.
    let error = refused(|bundle| {
        craft(
            bundle,
            &[("project.json", &json), ("media/a.png", b"tampered")],
            &[("project.json", &json), ("media/a.png", b"original")],
        );
    });
    assert!(matches!(error, BundleError::Mismatch), "{error:?}");
    // A file the manifest does not name.
    let error = refused(|bundle| {
        craft(
            bundle,
            &[("project.json", &json), ("media/extra.png", b"x")],
            &[("project.json", &json)],
        );
    });
    assert!(matches!(error, BundleError::Mismatch), "{error:?}");
    // A file the manifest names but the archive lacks.
    let error = refused(|bundle| {
        craft(
            bundle,
            &[("project.json", &json)],
            &[("project.json", &json), ("media/gone.png", b"x")],
        );
    });
    assert!(matches!(error, BundleError::Mismatch), "{error:?}");
}

#[test]
fn a_zip_bomb_is_refused_before_unpacking() {
    let json = project_json();
    let zeros = vec![0u8; 64 * 1024 * 1024];
    let error = refused(|bundle| {
        craft(
            bundle,
            &[("project.json", &json), ("media/bomb.bin", &zeros)],
            &[("project.json", &json), ("media/bomb.bin", &zeros)],
        );
    });
    assert!(matches!(error, BundleError::TooLarge), "{error:?}");
}

#[test]
fn media_pointing_outside_the_bundle_are_refused() {
    let mut project = Project::new(id(1), "x".into(), "2026-11-02T10:00:00Z".into());
    let template = project_with_picture(tempfile::tempdir().unwrap().path()).media[0].clone();
    project.media.push(MediaRef {
        location: MediaLocation::External {
            abs_path: "/Users/someone/secret.mov".into(),
            bookmark: None,
        },
        ..template
    });
    let mut value = serde_json::to_value(&project).unwrap();
    value["revision"] = 0.into();
    let json = serde_json::to_vec(&value).unwrap();
    let error = refused(|bundle| {
        craft(
            bundle,
            &[("project.json", &json)],
            &[("project.json", &json)],
        );
    });
    assert!(matches!(error, BundleError::Unsafe(_)), "{error:?}");
}

#[test]
fn something_that_is_not_a_zip_is_malformed() {
    let error = refused(|bundle| fs::write(bundle, b"just text").unwrap());
    assert!(matches!(error, BundleError::Malformed), "{error:?}");
}
