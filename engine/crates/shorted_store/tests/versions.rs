//! Version history (T-0107): kept, pruned, and restored as an undoable edit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use shorted_core::command::Command;
use shorted_core::model::{AudioTrack, Id, Project, TrackFlags};
use shorted_store::session::*;
use shorted_store::versions::*;
use uuid::Uuid;

fn id() -> Id {
    Uuid::from_u128(9)
}

fn create(root: &Path) -> ProjectSession {
    let (status, _) = mpsc::channel();
    let project = Project::new(id(), "Start".into(), "2026-11-02T10:00:00Z".into());
    ProjectSession::create(root, project, status).unwrap()
}

fn add_track(value: u128) -> Command {
    Command::AddAudioTrack {
        track: AudioTrack {
            id: Uuid::from_u128(value),
            name: format!("Track {value}"),
            flags: TrackFlags::default(),
        },
    }
}

/// Versions are written by the save thread: closing waits for it.
fn settle(session: ProjectSession) {
    session.close().unwrap();
}

#[test]
fn only_the_newest_twenty_versions_are_kept() {
    let folder = tempfile::tempdir().unwrap();
    for at in 0..25 {
        write_version(folder.path(), 1_000 + at, VersionReason::Activity, b"{}").unwrap();
    }
    let versions = list_versions(folder.path());
    assert_eq!(versions.len(), VERSIONS_KEPT);
    assert_eq!(versions[0].saved_at_ms, 1_024);
    assert_eq!(versions.last().unwrap().saved_at_ms, 1_005);
}

#[test]
fn names_that_were_not_listed_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    for name in [
        "../project.json",
        "1-auto.json/../x",
        "notes.txt",
        "12-other.json",
    ] {
        assert!(load_version(folder.path(), name).is_err(), "{name}");
    }
}

#[test]
fn an_ai_change_keeps_a_version_first() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    let folder = session.folder().to_path_buf();
    session
        .execute(&Command::Group {
            label: format!("{AI_GROUP_PREFIX}arrange"),
            commands: vec![add_track(1)],
        })
        .unwrap();
    session.execute(&add_track(2)).unwrap();
    settle(session);
    let versions = list_versions(&folder);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].reason, VersionReason::BeforeAi);
    let before = load_version(&folder, &versions[0].name).unwrap();
    assert!(before.timeline.audio_tracks.is_empty());
}

#[test]
fn restoring_a_version_is_one_undoable_step_and_keeps_the_present() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    let folder = session.folder().to_path_buf();
    session.execute(&add_track(1)).unwrap();
    session.keep_version(VersionReason::Activity).unwrap();
    session.execute(&add_track(2)).unwrap();
    session.execute(&add_track(3)).unwrap();
    // The save thread writes the version; give it a moment before reading the folder.
    for _ in 0..50 {
        if !list_versions(&folder).is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let kept = list_versions(&folder)[0].name.clone();

    session.restore_version(&kept).unwrap();
    assert_eq!(session.project().timeline.audio_tracks.len(), 1);
    assert_eq!(session.project().title, "Start");

    session.undo().unwrap();
    assert_eq!(session.project().timeline.audio_tracks.len(), 3);
    settle(session);
    let reasons: Vec<_> = list_versions(&folder).iter().map(|v| v.reason).collect();
    assert!(reasons.contains(&VersionReason::BeforeRestore));
}
