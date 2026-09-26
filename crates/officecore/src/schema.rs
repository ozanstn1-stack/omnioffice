//! Schema versioning and migrations for the native `.oswk` unit format.
//!
//! The unit envelope is JSON:
//!
//! ```json
//! { "format": "office-swiss-army-knife", "schemaVersion": 3, "kind": "writer", ... }
//! ```
//!
//! Older files written before V3 carry `"version": 2` (or no version at all).
//! [`migrate_unit`] upgrades a unit in place to the current schema, adding the
//! fields V3 understands with their defaults. Migrations never drop data:
//! unknown keys are preserved, and a model that cannot be migrated is reported
//! as an error instead of being rewritten.
//!
//! A unit written by a *newer* build cannot be safely interpreted, so it is
//! rejected with [`ErrorCode::UnsupportedFeature`] and the UI keeps the file
//! untouched.

use crate::error::{ErrorCode, OfficeError, OfficeResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Current schema version of the `.oswk` model.
pub const SCHEMA_VERSION: u32 = 3;

/// The result of migrating one unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub from_version: u32,
    pub to_version: u32,
    pub migrated: bool,
    pub notes: Vec<String>,
}

fn number(value: Option<&Value>) -> Option<u32> {
    value.and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok())
}

/// The schema version declared by a unit, or 1 when it predates versioning.
pub fn schema_version_of(unit: &Value) -> u32 {
    number(unit.get("schemaVersion"))
        .or_else(|| number(unit.get("version")))
        .unwrap_or(1)
}

fn ensure_object<'a>(model: &'a mut Value, note: &mut Option<String>) -> OfficeResult<&'a mut serde_json::Map<String, Value>> {
    model.as_object_mut().ok_or_else(|| OfficeError::corrupt("The document model is not a JSON object.")).map(|object| {
        let _ = note;
        object
    })
}

fn ensure_key(object: &mut serde_json::Map<String, Value>, key: &str, value: Value, notes: &mut Vec<String>, label: &str) {
    if !object.contains_key(key) {
        object.insert(key.to_string(), value);
        notes.push(format!("Added missing {label} field `{key}`."));
    }
}

fn ensure_array(object: &mut serde_json::Map<String, Value>, key: &str, notes: &mut Vec<String>, label: &str) {
    if !object.contains_key(key) {
        object.insert(key.to_string(), Value::Array(Vec::new()));
        notes.push(format!("Added missing {label} field `{key}`."));
    }
}

/// Migrates a Writer model in place, adding the V3 fields with defaults.
fn migrate_writer(model: &mut Value, notes: &mut Vec<String>) -> OfficeResult<()> {
    let object = ensure_object(model, &mut None)?;
    ensure_array(object, "footnotes", notes, "writer");
    ensure_array(object, "endnotes", notes, "writer");
    ensure_array(object, "bookmarks", notes, "writer");
    ensure_key(object, "trackChanges", Value::Bool(false), notes, "writer");
    ensure_key(object, "showRevisions", Value::Bool(true), notes, "writer");
    Ok(())
}

/// Migrates a Calc workbook in place (structured tables per sheet).
fn migrate_calc(model: &mut Value, notes: &mut Vec<String>) -> OfficeResult<()> {
    let object = ensure_object(model, &mut None)?;
    if let Some(sheets) = object.get_mut("sheets").and_then(Value::as_array_mut) {
        for sheet in sheets {
            if let Some(sheet) = sheet.as_object_mut() {
                ensure_array(sheet, "tables", notes, "sheet");
            }
        }
    }
    Ok(())
}

/// Migrates an Impress deck in place (masters, layouts, groups, animations).
fn migrate_impress(model: &mut Value, notes: &mut Vec<String>) -> OfficeResult<()> {
    let object = ensure_object(model, &mut None)?;
    ensure_array(object, "masters", notes, "deck");
    if let Some(slides) = object.get_mut("slides").and_then(Value::as_array_mut) {
        for slide in slides {
            if let Some(slide) = slide.as_object_mut() {
                ensure_array(slide, "animations", notes, "slide");
                ensure_key(slide, "masterId", Value::Null, notes, "slide");
                ensure_key(slide, "layoutId", Value::Null, notes, "slide");
                if let Some(objects) = slide.get_mut("objects").and_then(Value::as_array_mut) {
                    for object in objects {
                        if let Some(object) = object.as_object_mut() {
                            ensure_array(object, "children", notes, "shape");
                            ensure_key(object, "placeholder", Value::Null, notes, "shape");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Migrates one model to the current schema. `kind` is `writer`, `calc` or
/// `impress`.
pub fn migrate_model(kind: &str, model: &mut Value, from_version: u32) -> OfficeResult<Vec<String>> {
    if from_version > SCHEMA_VERSION {
        return Err(OfficeError::new(
            ErrorCode::UnsupportedFeature,
            format!(
                "This document was written by a newer version of the suite (schema {from_version}, this build supports {SCHEMA_VERSION}). Update the application to open it safely."
            ),
        ));
    }
    let mut notes = Vec::new();
    if from_version < 2 {
        notes.push("Document predates versioned schema; defaults were applied.".to_string());
    }
    match kind {
        "writer" => migrate_writer(model, &mut notes)?,
        "calc" => migrate_calc(model, &mut notes)?,
        "impress" => migrate_impress(model, &mut notes)?,
        other => {
            return Err(OfficeError::new(
                ErrorCode::UnsupportedFormat,
                format!("Unknown document kind '{other}'."),
            ))
        }
    }
    Ok(notes)
}

/// Migrates a full unit envelope in place and returns what changed. The unit's
/// `model` field is migrated according to its `kind`; the version is stamped
/// at the current schema.
pub fn migrate_unit(unit: &mut Value) -> OfficeResult<MigrationReport> {
    let from_version = schema_version_of(unit);
    if from_version > SCHEMA_VERSION {
        return Err(OfficeError::new(
            ErrorCode::UnsupportedFeature,
            format!(
                "This file was written by a newer version of the suite (schema {from_version}, this build supports {SCHEMA_VERSION}). Update the application to open it safely."
            ),
        ));
    }
    let kind = unit.get("kind").and_then(Value::as_str).unwrap_or("writer").to_string();
    let mut notes = Vec::new();
    if let Some(model) = unit.get_mut("model") {
        notes.extend(migrate_model(&kind, model, from_version)?);
    } else {
        return Err(OfficeError::corrupt("The unit does not contain a document model."));
    }
    let migrated = from_version < SCHEMA_VERSION;
    if let Some(object) = unit.as_object_mut() {
        object.insert("schemaVersion".into(), json!(SCHEMA_VERSION));
        // The old `version` key was the schema marker before V3; keep writing
        // it for readers that still look at it.
        object.insert("version".into(), json!(SCHEMA_VERSION));
    }
    Ok(MigrationReport { from_version, to_version: SCHEMA_VERSION, migrated, notes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TextDocument, Workbook, Deck};

    fn v2_writer_unit() -> Value {
        let mut document = TextDocument::new_blank("Legacy");
        document.blocks.push(crate::model::Block::heading("Intro", 1));
        let mut model = serde_json::to_value(&document).unwrap();
        // Strip the V3 fields to simulate a 2.x file.
        let object = model.as_object_mut().unwrap();
        object.remove("footnotes");
        object.remove("endnotes");
        object.remove("bookmarks");
        object.remove("trackChanges");
        object.remove("showRevisions");
        json!({
            "format": "office-swiss-army-knife",
            "version": 2,
            "kind": "writer",
            "title": "Legacy",
            "savedAt": "2025-01-01T00:00:00Z",
            "model": model
        })
    }

    #[test]
    fn v2_writer_migrates_without_data_loss() {
        let mut unit = v2_writer_unit();
        let report = migrate_unit(&mut unit).unwrap();
        assert!(report.migrated);
        assert_eq!(report.from_version, 2);
        assert_eq!(report.to_version, SCHEMA_VERSION);
        assert_eq!(unit.get("schemaVersion").and_then(Value::as_u64), Some(3));
        let document: TextDocument = serde_json::from_value(unit["model"].clone()).unwrap();
        assert_eq!(document.blocks.len(), 2);
        assert!(document.footnotes.is_empty());
        assert!(document.show_revisions);
        assert_eq!(document.plain_text().trim(), "Intro");
    }

    #[test]
    fn migration_is_idempotent() {
        let mut unit = v2_writer_unit();
        let first = migrate_unit(&mut unit).unwrap();
        assert!(first.migrated);
        let second = migrate_unit(&mut unit).unwrap();
        assert!(!second.migrated);
        assert_eq!(second.from_version, SCHEMA_VERSION);
        assert!(second.notes.is_empty());
    }

    #[test]
    fn future_versions_are_rejected_not_rewritten() {
        let mut unit = v2_writer_unit();
        unit["schemaVersion"] = json!(99);
        let error = migrate_unit(&mut unit).expect_err("future schema must be rejected");
        assert_eq!(error.code, "unsupported_feature");
        assert!(error.message.contains("newer version"));
        // The unit was not modified.
        assert_eq!(unit.get("schemaVersion").and_then(Value::as_u64), Some(99));
    }

    #[test]
    fn corrupt_models_are_reported() {
        let mut unit = json!({ "kind": "writer", "version": 2, "model": "not-an-object" });
        let error = migrate_unit(&mut unit).expect_err("corrupt model must fail");
        assert_eq!(error.code, "corrupt_document");
        let mut unit = json!({ "kind": "writer", "version": 2 });
        let error = migrate_unit(&mut unit).expect_err("missing model must fail");
        assert_eq!(error.code, "corrupt_document");
        let mut unit = json!({ "kind": "mystery", "version": 2, "model": {} });
        let error = migrate_unit(&mut unit).expect_err("unknown kind must fail");
        assert_eq!(error.code, "unsupported_format");
    }

    #[test]
    fn calc_and_impress_gain_their_v3_containers() {
        let workbook = Workbook::new_blank("Legacy");
        let mut calc_unit = json!({ "kind": "calc", "version": 2, "model": serde_json::to_value(&workbook).unwrap() });
        migrate_unit(&mut calc_unit).unwrap();
        let workbook: Workbook = serde_json::from_value(calc_unit["model"].clone()).unwrap();
        assert!(workbook.sheets[0].tables.is_empty());

        let deck = Deck::new_blank("Legacy");
        let mut impress_unit = json!({ "kind": "impress", "version": 2, "model": serde_json::to_value(&deck).unwrap() });
        migrate_unit(&mut impress_unit).unwrap();
        let deck: Deck = serde_json::from_value(impress_unit["model"].clone()).unwrap();
        assert!(deck.masters.is_empty());
        assert!(deck.slides[0].animations.is_empty());
    }
}
