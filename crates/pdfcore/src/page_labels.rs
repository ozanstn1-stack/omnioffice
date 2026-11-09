//! Keeping `/PageLabels` in step with the pages a plan keeps.
//!
//! `/PageLabels` maps a page *index* to the label a reader shows (ISO 32000-1 12.4.2,
//! Table 159). The indexes are positions, so removing a page silently moves every
//! label after it: a document numbered i, ii, 1, 2, 3 shows i, 1, 2, 3 only if the
//! range that started at index 2 starts at index 1 now.

use lopdf::{Dictionary, Document, Object};

use crate::docutil::PagePlanItem;

/// Rebuilds `/PageLabels` so the surviving pages keep the labels they had.
pub fn remap_page_labels(doc: &mut Document, plan: &[PagePlanItem]) -> bool {
    let Some(labels) = doc.catalog().ok().and_then(|catalog| catalog.get(b"PageLabels").ok().cloned()) else {
        return false;
    };
    let ranges = match &labels {
        Object::Dictionary(dict) => dict.clone(),
        Object::Reference(id) => match doc.get_dictionary(*id) {
            Ok(dict) => dict.clone(),
            Err(_) => return false,
        },
        _ => return false,
    };
    let Some(nums) = ranges.get(b"Nums").and_then(Object::as_array).ok().cloned() else { return false };
    // (start index, label dictionary), oldest first.
    let mut entries: Vec<(usize, Dictionary)> = Vec::new();
    for pair in nums.chunks(2) {
        let [start, dict] = pair else { continue };
        let Ok(start) = start.as_i64() else { continue };
        let Ok(dict) = dict.as_dict() else { continue };
        entries.push((start.max(0) as usize, dict.clone()));
    }
    if entries.is_empty() {
        return false;
    }

    // Which old index each kept page had, and how many pages before it are gone.
    let mut kept: Vec<(usize, usize)> = Vec::new(); // (new index, old index)
    let mut removed_before = 0usize;
    for (new_index, item) in plan.iter().enumerate() {
        if item.blank {
            continue;
        }
        let old_index = item.source_page.saturating_sub(1) as usize;
        kept.push((new_index, old_index));
        let _ = &mut removed_before;
    }
    let mut rebuilt: Vec<Object> = Vec::new();
    let mut last: Option<(usize, Dictionary)> = None;
    for (new_index, old_index) in &kept {
        let Some((start, dict)) = entries.iter().rev().find(|(start, _)| start <= old_index).cloned() else {
            continue;
        };
        // The label value that page had: the style's start plus its offset in the range.
        let offset = old_index - start;
        let mut dict = dict;
        match dict.get(b"S").and_then(Object::as_name).unwrap_or(b"") {
            b"D" | b"r" | b"R" | b"a" | b"A" | b"" => {
                let base = dict.get(b"St").and_then(Object::as_i64).unwrap_or(1);
                dict.set("St", Object::Integer(base + offset as i64));
            }
            _ => {}
        }
        let continues = match &last {
            // The same range continues: no new entry, and no `/St` needed.
            Some((previous_start, previous_dict)) => {
                previous_dict.get(b"S").ok() == dict.get(b"S").ok() && *previous_start + 1 == *new_index
            }
            None => false,
        };
        if !continues || last.is_none() {
            rebuilt.push(Object::Integer(*new_index as i64));
            rebuilt.push(Object::Dictionary(dict.clone()));
            last = Some((*new_index, dict));
        } else {
            last = Some((*new_index, dict));
        }
    }
    if rebuilt.is_empty() {
        return false;
    }
    // The first entry never needs a start marker; some producers still write it.
    match &labels {
        Object::Dictionary(dict) => {
            if let Ok(catalog) = doc.trailer.get(b"Root").and_then(Object::as_reference) {
                if let Ok(catalog) = doc.get_object_mut(catalog).and_then(Object::as_dict_mut) {
                    let mut new = dict.clone();
                    new.set("Nums", Object::Array(rebuilt));
                    catalog.set("PageLabels", Object::Dictionary(new));
                    return true;
                }
            }
        }
        Object::Reference(id) => {
            let id = *id;
            if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                entry.set("Nums", Object::Array(rebuilt));
                return true;
            }
        }
        _ => {}
    }
    false
}
