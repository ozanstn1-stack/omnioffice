//! Canonical `.oswk` unit envelope (v3.x).
//!
//! The unit is JSON:
//!
//! ```json
//! {
//!   "format": "office-swiss-army-knife",
//!   "schemaVersion": 3,
//!   "documentType": "writer",
//!   "kind": "writer",
//!   "applicationVersion": "3.2.1",
//!   "title": "...",
//!   "savedAt": "2026-01-01T00:00:00Z",
//!   "checksum": "<sha256 of model>",
//!   "featureManifest": ["text", "tables"],
//!   "extensions": {},
//!   "model": { ... }
//! }
//! ```
//!
//! Design rules:
//! * the model is the source of truth; the envelope is metadata,
//! * **unknown keys are preserved**, never pruned by a save,
//! * a unit with a `checksum` is verified before it is trusted,
//! * a unit from a newer schema version is refused, not migrated down,
//! * `batch_to_value` is the canonical (key-sorted) serialization used for the
//!   checksum, so the digest is stable regardless of writer order.

use crate::error::{ErrorCode, OfficeError, OfficeResult};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Current envelope schema version. This is the model schema version; the
/// envelope and the model move together.
pub const UNIT_SCHEMA_VERSION: u32 = crate::schema::SCHEMA_VERSION;

/// The envelope format marker written into every unit.
pub const UNIT_FORMAT: &str = "office-swiss-army-knife";

/// Canonicalizes a JSON value: objects are key-sorted recursively, arrays keep
/// their order. Used so the checksum does not depend on serialization order.
pub fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), canonicalize(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

/// SHA-256 hex digest over the canonical serialization of a value.
pub fn checksum_of(value: &Value) -> String {
    let canonical = canonicalize(value);
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The document family (`writer` / `calc` / `impress`), preferring the
/// canonical `documentType` field and falling back to the legacy `kind`.
pub fn document_type_of(unit: &Value) -> Option<String> {
    unit.get("documentType")
        .and_then(Value::as_str)
        .or_else(|| unit.get("kind").and_then(Value::as_str))
        .map(str::to_string)
}

/// The feature names a model actually uses, used as a compatibility hint.
pub fn feature_manifest(kind: &str, model: &Value) -> Vec<String> {
    let mut features: Vec<String> = Vec::new();
    match kind {
        "writer" => {
            features.push("text".into());
            let has_blocks = |kind: &str| {
                model
                    .get("blocks")
                    .and_then(Value::as_array)
                    .map(|blocks| blocks.iter().any(|block| block.get("type").and_then(Value::as_str) == Some(kind)))
                    .unwrap_or(false)
            };
            if has_blocks("table") {
                features.push("tables".into());
            }
            if has_blocks("image") {
                features.push("images".into());
            }
            if !empty(model.get("footnotes")) || !empty(model.get("endnotes")) {
                features.push("notes".into());
            }
            if !empty(model.get("comments")) {
                features.push("comments".into());
            }
            if !empty(model.get("bookmarks")) {
                features.push("bookmarks".into());
            }
            if !empty(model.get("sections")) {
                features.push("sections".into());
            }
            if model.get("trackChanges").and_then(Value::as_bool).unwrap_or(false) {
                features.push("track-changes".into());
            }
            if !empty(model.get("header")) || !empty(model.get("footer")) {
                features.push("headers-footers".into());
            }
        }
        "calc" => {
            features.push("sheets".into());
            if let Some(sheets) = model.get("sheets").and_then(Value::as_array) {
                if sheets.iter().any(|sheet| !empty(sheet.get("tables"))) {
                    features.push("tables".into());
                }
                if sheets.iter().any(|sheet| !empty(sheet.get("charts"))) {
                    features.push("charts".into());
                }
                if sheets.iter().any(|sheet| !empty(sheet.get("validations"))) {
                    features.push("validation".into());
                }
                if sheets
                    .iter()
                    .any(|sheet| !empty(sheet.get("conditionalFormats")) || !empty(sheet.get("conditional_formats")))
                {
                    features.push("conditional-formatting".into());
                }
                if sheets.iter().any(|sheet| !empty(sheet.get("pivotTables")) || !empty(sheet.get("pivot_tables"))) {
                    features.push("pivots".into());
                }
            }
        }
        "impress" => {
            features.push("slides".into());
            if !empty(model.get("masters")) {
                features.push("masters".into());
            }
            if let Some(slides) = model.get("slides").and_then(Value::as_array) {
                if slides.iter().any(|slide| !empty(slide.get("animations"))) {
                    features.push("animations".into());
                }
                if slides.iter().any(|slide| !empty(slide.get("notes"))) {
                    features.push("notes".into());
                }
                if slides.iter().any(|slide| !empty(slide.get("charts"))) {
                    features.push("charts".into());
                }
            }
        }
        _ => {}
    }
    features.sort();
    features.dedup();
    features
}

fn empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Array(items)) => items.is_empty(),
        Some(Value::Object(map)) => map.is_empty(),
        _ => false,
    }
}

/// Verifies the `checksum` of a parsed unit. `Ok(())` when the digest matches
/// or the unit predates checksums; an error when it does not (corruption or
/// tampering, reported as `corrupt_document`).
pub fn verify_checksum(unit: &Value) -> OfficeResult<()> {
    let Some(expected) = unit.get("checksum").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(model) = unit.get("model") else {
        return Err(OfficeError::corrupt("The unit has a checksum but no document model."));
    };
    let actual = checksum_of(model);
    if actual == expected {
        Ok(())
    } else {
        Err(OfficeError::new(
            ErrorCode::CorruptDocument,
            "The document checksum does not match its content; the file is damaged or was modified outside the app.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checksum_is_stable_across_key_order() {
        let a = json!({ "b": 1, "a": { "y": 2, "x": [1, 2] } });
        let b = json!({ "a": { "x": [1, 2], "y": 2 }, "b": 1 });
        assert_eq!(checksum_of(&a), checksum_of(&b));
        assert_eq!(checksum_of(&a).len(), 64);
    }

    #[test]
    fn checksum_detects_tampering() {
        let mut unit = json!({ "model": { "a": 1 } });
        unit["checksum"] = json!(checksum_of(&unit["model"]));
        assert!(verify_checksum(&unit).is_ok());
        unit["model"]["a"] = json!(2);
        let error = verify_checksum(&unit).expect_err("tampering must be detected");
        assert_eq!(error.code, "corrupt_document");
    }

    #[test]
    fn units_without_a_checksum_are_accepted() {
        let unit = json!({ "model": { "a": 1 } });
        assert!(verify_checksum(&unit).is_ok());
    }

    #[test]
    fn document_type_prefers_the_canonical_field() {
        assert_eq!(document_type_of(&json!({ "kind": "writer" })).as_deref(), Some("writer"));
        assert_eq!(document_type_of(&json!({ "documentType": "calc", "kind": "writer" })).as_deref(), Some("calc"));
        assert_eq!(document_type_of(&json!({})), None);
    }

    #[test]
    fn feature_manifest_reports_what_is_used() {
        let writer = json!({
            "blocks": [{ "type": "table" }, { "type": "paragraph" }],
            "footnotes": [{ "id": "1" }],
            "trackChanges": true
        });
        let features = feature_manifest("writer", &writer);
        assert!(features.contains(&"tables".to_string()));
        assert!(features.contains(&"notes".to_string()));
        assert!(features.contains(&"track-changes".to_string()));
        assert!(!features.contains(&"comments".to_string()));
    }
}
