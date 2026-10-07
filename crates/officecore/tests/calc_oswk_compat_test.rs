//! Backward compatibility of the Calc model additions (charts, conditional
//! formatting, comments, hyperlinks).
//!
//! A `.oswk` written before these fields existed must load unchanged, and a
//! workbook that does not use them must serialize without any of the new keys,
//! so an older build opening the file sees exactly what it always saw. The
//! JSON below is what the previous release wrote.

use officecore::model::*;
use serde_json::{json, Value};

/// The `.oswk` workbook model of the previous release: one chart, one rule,
/// one commented cell and one linked cell, with none of the new keys.
fn previous_release_workbook() -> Value {
    json!({
        "id": "wb-1",
        "title": "Old",
        "sheets": [{
            "id": "s1",
            "name": "Sheet1",
            "rowCount": 200,
            "colCount": 26,
            "cells": {
                "A1": {
                    "value": { "kind": "text", "value": "Region" },
                    "formula": null,
                    "style": {},
                    "comment": "check this",
                    "link": "https://example.org/report"
                },
                "B2": {
                    "value": { "kind": "number", "value": 12.5 },
                    "formula": null,
                    "style": {},
                    "comment": null,
                    "link": null
                }
            },
            "charts": [{
                "id": "c1",
                "chart": {
                    "kind": "column",
                    "title": "Sales",
                    "categories": "A2:A4",
                    "series": [{ "name": "S", "range": "B2:B4", "color": null }],
                    "legend": true,
                    "xTitle": "",
                    "yTitle": "",
                    "stacked": false,
                    "showLabels": false
                },
                "anchor": "D2",
                "widthPx": 420.0,
                "heightPx": 260.0
            }],
            "conditional": [
                { "id": "r1", "range": "B2:B4", "kind": "greater", "values": ["10"], "fill": "#FFC7CE",
                  "color": null, "topN": null, "stopIfTrue": false },
                { "id": "r2", "range": "B2:B4", "kind": "dataBar", "values": [], "fill": "#638EC6",
                  "color": null, "topN": null, "stopIfTrue": false }
            ]
        }]
    })
}

fn load(value: &Value) -> Workbook {
    serde_json::from_value(value.clone()).expect("the previous release's workbook must still load")
}

#[test]
fn a_workbook_from_the_previous_release_loads_with_the_new_fields_unset() {
    let workbook = load(&previous_release_workbook());
    let sheet = &workbook.sheets[0];

    let chart = &sheet.charts[0].chart;
    assert_eq!(chart.kind, "column");
    assert_eq!(chart.hole_size, None);
    assert_eq!(chart.scatter_style, None);

    assert_eq!(sheet.conditional.len(), 2);
    assert_eq!(sheet.conditional[0].kind, "greater");
    assert_eq!(sheet.conditional[1].kind, "dataBar");
    assert_eq!(sheet.conditional[1].fill.as_deref(), Some("#638EC6"));

    let commented = sheet.get("A1").expect("A1");
    assert_eq!(commented.comment.as_deref(), Some("check this"));
    assert_eq!(commented.link.as_deref(), Some("https://example.org/report"));
}

#[test]
fn a_workbook_without_the_new_features_serializes_without_the_new_keys() {
    let workbook = load(&previous_release_workbook());
    let text = serde_json::to_string(&workbook).unwrap();
    for key in [
        "holeSize",
        "scatterStyle",
        "thresholds",
        "iconSet",
        "reverseIcons",
        "hideValue",
        "formula\":\"",
        "bold\":true",
        "commentAuthor",
        "commentVisible",
        "linkDisplay",
        "linkTooltip",
    ] {
        assert!(!text.contains(key), "unused key `{key}` leaked into the serialized workbook: {text}");
    }
    // And the old file survives a load -> save -> load cycle unchanged.
    let again: Workbook = serde_json::from_str(&text).unwrap();
    assert_eq!(again, workbook);
}
