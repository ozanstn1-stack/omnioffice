//! PDF forms fuzz target (E2.5): exercises form field discovery, validation,
//! page-object enumeration, and form filling on malformed PDFs without invoking
//! external native engines.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdfcore::forms::{apply_field_values, list_fields, list_page_objects, validate_fields, FieldValue};

fuzz_target!(|data: &[u8]| {
    if data.len() > 2 * 1024 * 1024 {
        return;
    }
    let Ok(mut doc) = lopdf::Document::load_mem(data) else {
        return;
    };
    if doc.objects.len() > 4096 {
        return;
    }
    let fields = list_fields(&doc);
    let _ = list_page_objects(&doc);

    let mut values: Vec<FieldValue> = fields
        .iter()
        .take(8)
        .enumerate()
        .map(|(index, field)| FieldValue {
            name: field.name.clone(),
            value: if index % 2 == 0 { "Yes".into() } else { "Sample".into() },
            values: if index % 3 == 0 { vec!["Opt1".into(), "Opt2".into()] } else { Vec::new() },
        })
        .collect();
    values.push(FieldValue { name: "nonexistent".into(), value: "value".into(), values: Vec::new() });

    let _ = validate_fields(&doc, &values);
    let _ = apply_field_values(&mut doc, &values);
});
