//! Schema migrations (NFR-REL-03). Each step turns version N into N+1 on raw JSON, so old files
//! never need old Rust types.

use serde_json::{Value, json};
use shorted_core::model::SCHEMA_VERSION;

use crate::session::StoreError;

type Step = fn(Value) -> Result<Value, StoreError>;

/// `STEPS[n]` migrates version `n` to `n + 1`.
const STEPS: [Step; SCHEMA_VERSION as usize] = [v0_to_v1];

/// Version 0 never shipped: it only proves the chain, with a `name` field and no canvas fps.
fn v0_to_v1(mut document: Value) -> Result<Value, StoreError> {
    let root = document.as_object_mut().ok_or(StoreError::Malformed)?;
    let title = root.remove("name").ok_or(StoreError::Malformed)?;
    root.insert("title".into(), title);
    root.entry("canvas")
        .or_insert(json!({"width": 1080, "height": 1920, "fps_hint": 30}));
    Ok(document)
}

fn version_of(document: &Value) -> Result<u32, StoreError> {
    document
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or(StoreError::Malformed)
}

/// Brings a document to the current schema. Also returns the version it came from, if older.
/// A newer document is refused with a typed error: the caller opens it read-only or not at all,
/// and it is never rewritten.
pub fn migrate(mut document: Value) -> Result<(Value, Option<u32>), StoreError> {
    let found = version_of(&document)?;
    if found > SCHEMA_VERSION {
        return Err(StoreError::NewerVersion { found });
    }
    for version in found..SCHEMA_VERSION {
        document = STEPS[version as usize](document)?;
        document["schema_version"] = json!(version + 1);
    }
    Ok((document, (found < SCHEMA_VERSION).then_some(found)))
}
