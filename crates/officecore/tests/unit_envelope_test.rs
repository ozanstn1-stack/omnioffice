//! Canonical `.oswk` envelope contract tests.
//!
//! These pin the v3.x envelope: stable checksum, document type, feature
//! manifest, extension preservation and corrupt-file detection. They exercise
//! `officecore::unit` directly (the same functions `src-tauri` uses), so they
//! run on every platform without the Tauri shell.

use officecore::schema;
use officecore::unit;
use serde_json::{json, Map, Value};

/// A representative writer model touching several features.
fn writer_model() -> Value {
    json!({
        "id": "unit-writer",
        "title": "Canonical",
        "blocks": [
            { "type": "paragraph", "runs": [{ "text": "Hello" }] },
            { "type": "table", "table": { "rows": [] } }
        ],
        "footnotes": [{ "id": "fn-1", "runs": [] }],
        "comments": [{ "id": "c-1", "text": "note" }],
        "bookmarks": [{ "id": "b-1", "name": "mark" }],
        "sections": [{ "id": "s-1" }],
        "trackChanges": true,
        "showRevisions": true,
        "watermark": { "text": "DRAFT" }
    })
}

fn envelope(kind: &str, model: Value) -> Value {
    let checksum = unit::checksum_of(&model);
    let features = unit::feature_manifest(kind, &model);
    json!({
        "format": unit::UNIT_FORMAT,
        "version": schema::SCHEMA_VERSION,
        "schemaVersion": schema::SCHEMA_VERSION,
        "documentType": kind,
        "kind": kind,
        "applicationVersion": "3.2.1",
        "title": "Canonical",
        "savedAt": "2026-01-01T00:00:00Z",
        "checksum": checksum,
        "featureManifest": features,
        "extensions": {},
        "warnings": [],
        "model": model
    })
}

#[test]
fn envelope_round_trips_with_a_stable_checksum() {
    let unit_value = envelope("writer", writer_model());
    assert_eq!(unit_value.get("format").and_then(Value::as_str), Some(unit::UNIT_FORMAT));
    assert_eq!(unit::document_type_of(&unit_value).as_deref(), Some("writer"));
    assert!(unit::verify_checksum(&unit_value).is_ok());

    // Serialize and parse again: the checksum still matches because it is over
    // the canonical (key-sorted) model, not the envelope's insertion order.
    let bytes = serde_json::to_vec_pretty(&unit_value).unwrap();
    let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
    let report = schema::migrate_unit(&mut raw).unwrap();
    assert!(!report.migrated);
    assert!(unit::verify_checksum(&raw).is_ok());
}

#[test]
fn checksum_mismatch_is_a_corrupt_document() {
    let mut unit_value = envelope("writer", writer_model());
    // A byte changed after the checksum was written.
    unit_value["model"]["blocks"][0]["runs"][0]["text"] = json!("Tampered");
    let error = unit::verify_checksum(&unit_value).expect_err("tampering must be caught");
    assert_eq!(error.code, "corrupt_document");
    assert!(error.message.to_lowercase().contains("checksum"));
}

#[test]
fn feature_manifest_lists_the_writer_features_in_use() {
    let features = unit::feature_manifest("writer", &writer_model());
    for expected in ["text", "tables", "notes", "comments", "bookmarks", "sections", "track-changes", "watermark"] {
        assert!(features.contains(&expected.to_string()), "{expected} missing from {features:?}");
    }
    // A plain paragraph does not claim table/image support.
    let plain = json!({ "blocks": [{ "type": "paragraph", "runs": [] }] });
    let plain_features = unit::feature_manifest("writer", &plain);
    assert!(!plain_features.contains(&"tables".to_string()));
    assert!(!plain_features.contains(&"watermark".to_string()));
    assert!(plain_features.contains(&"text".to_string()));
}

#[test]
fn calc_and_impress_feature_manifests() {
    let workbook = json!({
        "sheets": [{
            "name": "Sheet1",
            "tables": [{ "id": "t1" }],
            "charts": [{ "id": "ch1" }],
            "validations": [{ "id": "v1" }],
            "conditionalFormats": [{ "id": "cf1" }]
        }]
    });
    let features = unit::feature_manifest("calc", &workbook);
    for expected in ["sheets", "tables", "charts", "validation", "conditional-formatting"] {
        assert!(features.contains(&expected.to_string()), "{expected} missing from {features:?}");
    }

    let deck = json!({
        "masters": [{ "id": "m1" }],
        "footer": { "enabled": true, "text": "Acme" },
        "slides": [
            {
                "animations": [{ "id": "a1" }],
                "notes": "hello",
                "objects": [
                    { "kind": "chart", "chart": { "kind": "column" } },
                    { "kind": "group", "children": [{ "kind": "chart", "chart": { "kind": "pie" } }] }
                ]
            },
            { "hidden": true, "objects": [] }
        ]
    });
    let features = unit::feature_manifest("impress", &deck);
    for expected in ["slides", "masters", "animations", "notes", "charts", "hiddenSlides", "footer"] {
        assert!(features.contains(&expected.to_string()), "{expected} missing from {features:?}");
    }
    // A deck with no hidden slides and no footer does not claim them.
    let plain = json!({ "slides": [{ "objects": [] }] });
    let plain_features = unit::feature_manifest("impress", &plain);
    assert!(!plain_features.contains(&"hiddenSlides".to_string()));
    assert!(!plain_features.contains(&"footer".to_string()));
    assert!(!plain_features.contains(&"charts".to_string()));
}

#[test]
fn unknown_envelope_fields_survive_a_migration() {
    // The schema migration must not prune keys it does not understand, so a
    // future build can add fields without this build destroying them.
    let mut raw = envelope("writer", writer_model());
    let mut extensions = Map::new();
    extensions.insert("future.field".into(), json!({ "kept": true }));
    raw["extensions"] = Value::Object(extensions);
    raw["unknownTopLevel"] = json!([1, 2, 3]);

    let report = schema::migrate_unit(&mut raw).unwrap();
    assert!(!report.migrated, "a current-schema unit must not be rewritten");
    assert_eq!(raw["unknownTopLevel"], json!([1, 2, 3]), "unknown top-level keys are preserved");
    assert_eq!(raw["extensions"]["future.field"]["kept"], json!(true));
}

#[test]
fn future_schema_is_refused_not_rewritten() {
    let mut raw = envelope("writer", writer_model());
    raw["schemaVersion"] = json!(schema::SCHEMA_VERSION + 7);
    let error = schema::migrate_unit(&mut raw).expect_err("a newer schema must be refused");
    assert_eq!(error.code, "unsupported_feature");
    assert_eq!(raw["schemaVersion"].as_u64(), Some((schema::SCHEMA_VERSION + 7) as u64));
}

#[test]
fn a_unit_without_a_checksum_predates_the_field_and_is_accepted() {
    let mut raw = envelope("writer", writer_model());
    raw.as_object_mut().unwrap().remove("checksum");
    assert!(unit::verify_checksum(&raw).is_ok());
}
