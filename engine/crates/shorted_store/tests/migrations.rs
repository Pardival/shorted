//! Old and newer project files (T-0105).

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;
use std::sync::mpsc;

use shorted_core::model::{Id, SCHEMA_VERSION};
use shorted_store::migrations::migrate;
use shorted_store::session::*;

const V0: &str = include_str!("../../../fixtures/migrations/v0_project.json");

fn install(root: &Path, json: &str) -> (Id, std::path::PathBuf) {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let id: Id = value["id"].as_str().unwrap().parse().unwrap();
    let folder = project_folder(root, id);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(PROJECT_FILE), json).unwrap();
    (id, folder)
}

fn open(root: &Path, id: Id) -> Result<ProjectSession, StoreError> {
    let (status, _) = mpsc::channel();
    ProjectSession::open(root, id, status)
}

#[test]
fn a_v0_project_migrates_and_keeps_its_data_and_revision() {
    let root = tempfile::tempdir().unwrap();
    let (id, folder) = install(root.path(), V0);
    let session = open(root.path(), id).unwrap();
    assert_eq!(session.project().schema_version, SCHEMA_VERSION);
    assert_eq!(session.project().title, "Empty");
    assert_eq!(session.project().canvas.fps_hint, 30);
    assert_eq!(session.revision(), 4);
    assert_eq!(
        fs::read_to_string(folder.join("project.v0.json")).unwrap(),
        V0
    );
}

#[test]
fn a_migrated_project_saves_in_the_current_schema() {
    let root = tempfile::tempdir().unwrap();
    let (id, _) = install(root.path(), V0);
    open(root.path(), id).unwrap().close().unwrap();
    let session = open(root.path(), id).unwrap();
    assert_eq!(session.project().title, "Empty");
}

#[test]
fn a_journal_older_than_a_migration_is_not_replayed() {
    let root = tempfile::tempdir().unwrap();
    let (id, folder) = install(root.path(), V0);
    fs::write(
        folder.join(JOURNAL_FILE),
        "{\"rev\":5,\"ts\":0,\"cmd\":{\"cmd\":\"rename\",\"title\":\"Old\"}}\n",
    )
    .unwrap();
    assert_eq!(open(root.path(), id).unwrap().project().title, "Empty");
}

#[test]
fn a_newer_project_gives_a_typed_error_and_stays_untouched() {
    let root = tempfile::tempdir().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(V0).unwrap();
    value["schema_version"] = serde_json::json!(SCHEMA_VERSION + 1);
    let json = value.to_string();
    let (id, folder) = install(root.path(), &json);
    assert!(matches!(
        open(root.path(), id),
        Err(StoreError::NewerVersion { found }) if found == SCHEMA_VERSION + 1
    ));
    assert_eq!(fs::read_to_string(folder.join(PROJECT_FILE)).unwrap(), json);
}

#[test]
fn documents_without_a_version_are_malformed() {
    for json in [
        "{}",
        "[]",
        "{\"schema_version\":\"1\"}",
        "{\"schema_version\":-1}",
    ] {
        let value = serde_json::from_str(json).unwrap();
        assert!(
            matches!(migrate(value), Err(StoreError::Malformed)),
            "{json}"
        );
    }
}
