//! Import of pivot caches and tables, preserved as raw parts for re-export.

use super::*;

// ---------------------------------------------------------------------------
// Import: pivot caches and tables
// ---------------------------------------------------------------------------

/// Reads the pivot tables linked from a worksheet and preserves their raw
/// parts. The rendered grid is already in the sheet cells from the calamine
/// pass; nothing here recomputes an aggregation.
pub(super) fn apply_pivot_tables(
    zip: &crate::zip::ZipReader,
    part: &str,
    sheet: &Sheet,
    preserved: &mut Vec<PreservedPivot>,
    warnings: &mut Vec<String>,
) {
    let relationships = read_relationships(zip, part);
    for relationship in relationships.values() {
        if !relationship.kind.ends_with("/pivotTable") {
            continue;
        }
        let target = resolve_part(part, &relationship.target);
        if let Some(pivot) = parse_pivot_table(zip, &target, &sheet.name, warnings) {
            // Two sheets can share one cache; the table part itself is unique.
            if !preserved.iter().any(|existing| existing.table_xml == pivot.table_xml) {
                preserved.push(pivot);
            }
        }
    }
}

fn parse_pivot_table(
    zip: &crate::zip::ZipReader,
    target: &str,
    sheet_name: &str,
    warnings: &mut Vec<String>,
) -> Option<PreservedPivot> {
    let table_xml = match zip.read_text(target) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("A pivot table part could not be read ({error})."));
            return None;
        }
    };
    if table_xml.len() > MAX_DETAIL_PART_BYTES {
        warnings.push("A pivot table part is too large to inspect and was kept in the original file only.".to_string());
        return None;
    }
    let root = match parse_xml(&table_xml) {
        Ok(root) => root,
        Err(_) => {
            warnings.push("A pivot table part could not be parsed and was kept in the original file only.".to_string());
            return None;
        }
    };
    let mut name = root.attr("name").unwrap_or("").trim().to_string();
    if name.is_empty() {
        name = target.rsplit('/').next().unwrap_or("PivotTable").trim_end_matches(".xml").to_string();
    }
    let cache_id = parse_u32_attr(&root, "cacheId").unwrap_or(0);
    let relationships = read_relationships(zip, target);
    let Some(definition_relationship) =
        relationships.values().find(|relationship| relationship.kind.ends_with("/pivotCacheDefinition"))
    else {
        warnings
            .push(format!("Pivot table \"{name}\" has no cache definition and was kept in the original file only."));
        return None;
    };
    let definition_part = resolve_part(target, &definition_relationship.target);
    let definition_xml = match zip.read_text(&definition_part) {
        Ok(text) if text.len() <= MAX_DETAIL_PART_BYTES => text,
        _ => {
            warnings.push(format!(
                "The pivot cache definition for \"{name}\" could not be read and was kept in the original file only."
            ));
            return None;
        }
    };
    let Ok(definition_root) = parse_xml(&definition_xml) else {
        warnings.push(format!(
            "The pivot cache definition for \"{name}\" could not be parsed and was kept in the original file only."
        ));
        return None;
    };
    let source = definition_root
        .child("cacheSource")
        .and_then(|source| source.child("worksheetSource"))
        .map(|worksheet| {
            let reference = worksheet.attr("ref").unwrap_or("").trim().replace('$', "");
            let source_sheet = worksheet.attr("sheet").unwrap_or("").trim();
            if source_sheet.is_empty() || reference.is_empty() {
                reference
            } else {
                format!("{source_sheet}!{reference}")
            }
        })
        .unwrap_or_default();
    let fields: Vec<String> = definition_root
        .child("cacheFields")
        .map(|block| {
            block
                .children_of("cacheField")
                .into_iter()
                .take(MAX_PIVOT_FIELDS)
                .map(|field| field.attr("name").unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    let definition_relationships = read_relationships(zip, &definition_part);
    let records_part = definition_relationships
        .values()
        .find(|relationship| relationship.kind.ends_with("/pivotCacheRecords"))
        .map(|relationship| resolve_part(&definition_part, &relationship.target));
    let records_base64 = records_part
        .as_ref()
        .and_then(|part| zip.read(part).ok())
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
    Some(PreservedPivot {
        name,
        sheet: sheet_name.to_string(),
        cache_id,
        definition_xml,
        records_base64,
        table_xml,
        records_part,
        source,
        fields,
    })
}
