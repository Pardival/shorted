//! Autosave, recovery and the kill test (T-0104).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use shorted_core::command::Command;
use shorted_core::model::{Id, Project};
use shorted_store::session::*;
use uuid::Uuid;

fn id() -> Id {
    Uuid::from_u128(7)
}

fn new_project() -> Project {
    Project::new(id(), "Start".into(), "2026-11-02T10:00:00Z".into())
}

fn create(root: &Path) -> ProjectSession {
    let (status, _) = mpsc::channel();
    ProjectSession::create(root, new_project(), status).unwrap()
}

fn open(root: &Path) -> ProjectSession {
    let (status, _) = mpsc::channel();
    ProjectSession::open(root, id(), status).unwrap()
}

fn rename(title: &str) -> Command {
    Command::Rename {
        title: title.into(),
    }
}

fn journal(root: &Path) -> std::path::PathBuf {
    project_folder(root, id()).join(JOURNAL_FILE)
}

#[test]
fn closing_and_reopening_keeps_the_edits_and_empties_the_journal() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    session.execute(&rename("Edited")).unwrap();
    session.close().unwrap();
    assert_eq!(fs::metadata(journal(root.path())).unwrap().len(), 0);
    assert_eq!(open(root.path()).project().title, "Edited");
}

#[test]
fn edits_survive_without_a_clean_close() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    for step in 0..5 {
        session.execute(&rename(&step.to_string())).unwrap();
    }
    drop(session);
    let reopened = open(root.path());
    assert_eq!(reopened.project().title, "4");
    assert_eq!(reopened.revision(), 5);
}

#[test]
fn undo_and_redo_are_saved_too() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    session.execute(&rename("A")).unwrap();
    session.execute(&rename("B")).unwrap();
    session.undo().unwrap();
    drop(session);
    assert_eq!(open(root.path()).project().title, "A");
}

#[test]
fn gesture_previews_are_not_saved() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    session.preview(&rename("Dragging")).unwrap();
    drop(session);
    assert_eq!(open(root.path()).project().title, "Start");
}

#[test]
fn a_torn_last_line_is_ignored() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    session.execute(&rename("Good")).unwrap();
    drop(session);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(journal(root.path()))
        .unwrap();
    file.write_all(b"{\"rev\":2,\"ts\":1,\"cmd\":{\"cmd\":\"rena")
        .unwrap();
    drop(file);
    let reopened = open(root.path());
    assert_eq!(reopened.project().title, "Good");
    assert_eq!(reopened.revision(), 1);
}

#[test]
fn lines_already_in_the_snapshot_are_not_replayed() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    session.execute(&rename("Old")).unwrap();
    let lines = {
        drop(session);
        fs::read(journal(root.path())).unwrap()
    };
    let mut session = open(root.path());
    session.execute(&rename("New")).unwrap();
    session.close().unwrap();
    // As if the crash came after the snapshot, before the journal was emptied.
    fs::write(journal(root.path()), lines).unwrap();
    assert_eq!(open(root.path()).project().title, "New");
}

#[test]
fn a_snapshot_is_taken_every_50_commands() {
    let root = tempfile::tempdir().unwrap();
    let mut session = create(root.path());
    for step in 0..SNAPSHOT_EVERY_COMMANDS {
        session.execute(&rename(&step.to_string())).unwrap();
    }
    drop(session);
    assert_eq!(fs::metadata(journal(root.path())).unwrap().len(), 0);
    assert_eq!(
        open(root.path()).revision(),
        u64::from(SNAPSHOT_EVERY_COMMANDS)
    );
}

#[test]
fn a_corrupted_snapshot_gives_an_error_and_is_kept() {
    let root = tempfile::tempdir().unwrap();
    drop(create(root.path()));
    let path = project_folder(root.path(), id()).join(PROJECT_FILE);
    fs::write(&path, "{broken").unwrap();
    let (status, _) = mpsc::channel();
    assert!(matches!(
        ProjectSession::open(root.path(), id(), status),
        Err(StoreError::Malformed)
    ));
    assert_eq!(fs::read_to_string(path).unwrap(), "{broken");
}

#[test]
fn status_goes_from_saving_to_saved_within_a_second() {
    let root = tempfile::tempdir().unwrap();
    let (status, events) = mpsc::channel();
    let mut session = ProjectSession::create(root.path(), new_project(), status).unwrap();
    session.execute(&rename("A")).unwrap();
    assert_eq!(
        events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SaveStatus::Saving
    );
    assert_eq!(
        events.recv_timeout(Duration::from_secs(1)).unwrap(),
        SaveStatus::Saved
    );
}

#[test]
fn replay_of_1000_lines_takes_under_200_ms() {
    let root = tempfile::tempdir().unwrap();
    drop(create(root.path()));
    let lines: String = (1..=1000)
        .map(|rev| {
            format!(
                "{{\"rev\":{rev},\"ts\":0,\"cmd\":{{\"cmd\":\"rename\",\"title\":\"{rev}\"}}}}\n"
            )
        })
        .collect();
    fs::write(journal(root.path()), lines).unwrap();
    let start = Instant::now();
    let session = open(root.path());
    let elapsed = start.elapsed();
    assert_eq!(session.project().title, "1000");
    assert!(elapsed < Duration::from_millis(200), "{elapsed:?}");
}

#[test]
fn killing_the_process_loses_at_most_2_s_of_commands() {
    let root = tempfile::tempdir().unwrap();
    drop(create(root.path()));
    let mut child = Process::new(env!("CARGO_BIN_EXE_store_kill_helper"))
        .arg(root.path())
        .arg(id().to_string())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    let start = Instant::now();
    let mut acknowledged: Vec<(u64, Instant)> = Vec::new();
    while start.elapsed() < Duration::from_millis(1500) {
        line.clear();
        reader.read_line(&mut line).unwrap();
        acknowledged.push((line.trim().parse().unwrap(), Instant::now()));
    }
    // A hard kill: no destructor runs.
    child.kill().unwrap();
    let killed_at = Instant::now();
    child.wait().unwrap();

    let recovered = open(root.path()).revision();
    let (_, recovered_at) = acknowledged
        .iter()
        .rev()
        .find(|(revision, _)| *revision <= recovered)
        .expect("something was recovered");
    let lost = killed_at - *recovered_at;
    assert!(
        lost <= Duration::from_secs(2),
        "lost {lost:?}, recovered {recovered}"
    );
    assert!(recovered > 100, "only {recovered} commands ran");
}
