//! Project list, duplication and trash (T-1002).

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use shorted_core::command::Command;
use shorted_core::model::{Id, Project};
use shorted_store::library::*;
use shorted_store::session::*;
use uuid::Uuid;

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

fn create(root: &Path, value: u128, title: &str) {
    let project = Project::new(id(value), title.into(), "2026-11-02T10:00:00Z".into());
    let (status, _) = mpsc::channel();
    ProjectSession::create(root, project, status)
        .unwrap()
        .close()
        .unwrap();
}

#[test]
fn an_empty_library_is_an_empty_list() {
    let root = tempfile::tempdir().unwrap();
    assert!(list_projects(root.path()).unwrap().is_empty());
    assert!(list_trash(root.path()).unwrap().is_empty());
}

#[test]
fn list_is_newest_first_and_covers_journal_only_edits() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "One");
    std::thread::sleep(Duration::from_millis(20));
    create(root.path(), 2, "Two");
    std::thread::sleep(Duration::from_millis(20));
    let (status, _) = mpsc::channel();
    let mut session = ProjectSession::open(root.path(), id(1), status).unwrap();
    session
        .execute(&Command::Rename {
            title: "Edited".into(),
        })
        .unwrap();
    drop(session);
    let list = list_projects(root.path()).unwrap();
    assert_eq!(list[0].title, "Edited");
    assert_eq!(list[1].title, "Two");
}

#[test]
fn a_damaged_project_does_not_hide_the_others() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "Good");
    create(root.path(), 2, "Bad");
    fs::write(project_folder(root.path(), id(2)).join(PROJECT_FILE), "{").unwrap();
    assert_eq!(list_projects(root.path()).unwrap().len(), 1);
}

#[test]
fn folders_with_other_names_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "Good");
    fs::create_dir_all(root.path().join("projects/not-a-project")).unwrap();
    assert_eq!(list_projects(root.path()).unwrap().len(), 1);
}

#[test]
fn trash_and_restore_bring_the_project_back_intact() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "One");
    trash_project(root.path(), id(1)).unwrap();
    assert!(list_projects(root.path()).unwrap().is_empty());
    assert_eq!(list_trash(root.path()).unwrap()[0].title, "One");
    restore_project(root.path(), id(1)).unwrap();
    assert_eq!(list_projects(root.path()).unwrap()[0].title, "One");
    assert!(list_trash(root.path()).unwrap().is_empty());
}

#[test]
fn trash_keeps_projects_for_7_days_only() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "One");
    trash_project(root.path(), id(1)).unwrap();
    let now = SystemTime::now();
    assert_eq!(
        purge_trash(root.path(), now + Duration::from_secs(6 * 86_400)).unwrap(),
        0
    );
    assert_eq!(
        purge_trash(root.path(), now + Duration::from_secs(8 * 86_400)).unwrap(),
        1
    );
    assert!(list_trash(root.path()).unwrap().is_empty());
}

#[test]
fn duplicate_copies_content_under_a_new_identity() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "One");
    fs::write(
        project_folder(root.path(), id(1)).join("media/clip.mov"),
        b"data",
    )
    .unwrap();
    duplicate_project(root.path(), id(1), id(2), "One copy".into()).unwrap();
    let (status, _) = mpsc::channel();
    let session = ProjectSession::open(root.path(), id(2), status).unwrap();
    assert_eq!(session.project().id, id(2));
    assert_eq!(session.project().title, "One copy");
    assert_eq!(
        fs::read(project_folder(root.path(), id(2)).join("media/clip.mov")).unwrap(),
        b"data"
    );
    assert_eq!(list_projects(root.path()).unwrap().len(), 2);
}

#[test]
fn duplicating_keeps_journal_only_edits() {
    let root = tempfile::tempdir().unwrap();
    create(root.path(), 1, "One");
    let (status, _) = mpsc::channel();
    let mut session = ProjectSession::open(root.path(), id(1), status).unwrap();
    session.execute(&Command::SetCanvasFps { fps: 60 }).unwrap();
    drop(session);
    let copy = duplicate_project(root.path(), id(1), id(2), "Copy".into()).unwrap();
    assert_eq!(copy.canvas.fps_hint, 60);
}
