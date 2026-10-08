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

#[test]
fn the_new_fields_round_trip_through_json_under_camel_case_keys() {
    let mut workbook = load(&previous_release_workbook());
    let sheet = &mut workbook.sheets[0];
    sheet.charts[0].chart.kind = "doughnut".into();
    sheet.charts[0].chart.hole_size = Some(65);
    sheet.charts.push(ChartPlacement {
        id: "c2".into(),
        chart: ChartData { kind: "scatter".into(), scatter_style: Some("smoothMarker".into()), ..Default::default() },
        ..Default::default()
    });
    sheet.conditional.push(CondRule {
        id: "r3".into(),
        range: "B2:B4".into(),
        kind: "iconSet".into(),
        icon_set: Some("3Arrows".into()),
        reverse_icons: true,
        hide_value: true,
        thresholds: vec![CondThreshold { kind: "percent".into(), value: "33".into(), color: None }],
        ..Default::default()
    });
    sheet.conditional.push(CondRule {
        id: "r4".into(),
        range: "B2:B4".into(),
        kind: "expression".into(),
        formula: Some("$B2>1".into()),
        bold: true,
        italic: true,
        ..Default::default()
    });
    let cell = sheet.cells.get_mut("A1").unwrap();
    cell.comment_author = Some("Ada".into());
    cell.comment_visible = true;
    cell.link_display = Some("Open".into());
    cell.link_tooltip = Some("Tip".into());

    let value = serde_json::to_value(&workbook).unwrap();
    let json = value["sheets"][0].clone();
    assert_eq!(json["charts"][0]["chart"]["holeSize"], 65);
    assert_eq!(json["charts"][1]["chart"]["scatterStyle"], "smoothMarker");
    assert_eq!(json["conditional"][2]["iconSet"], "3Arrows");
    assert_eq!(json["conditional"][2]["reverseIcons"], true);
    assert_eq!(json["conditional"][2]["hideValue"], true);
    assert_eq!(json["conditional"][2]["thresholds"][0]["kind"], "percent");
    assert_eq!(json["conditional"][3]["formula"], "$B2>1");
    assert_eq!(json["conditional"][3]["bold"], true);
    assert_eq!(json["conditional"][3]["italic"], true);
    assert_eq!(json["cells"]["A1"]["commentAuthor"], "Ada");
    assert_eq!(json["cells"]["A1"]["commentVisible"], true);
    assert_eq!(json["cells"]["A1"]["linkDisplay"], "Open");
    assert_eq!(json["cells"]["A1"]["linkTooltip"], "Tip");
    // Rules and cells that do not use the new fields stay as they were.
    assert!(json["conditional"][0].get("thresholds").is_none());
    assert!(json["cells"]["B2"].get("commentAuthor").is_none());

    let back: Workbook = serde_json::from_value(value).unwrap();
    assert_eq!(back, workbook);
}
