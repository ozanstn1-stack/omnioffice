//! PDF redaction pattern/classifier fuzz target (E2.5): feeds arbitrary byte
//! streams turned into character geometry into `classify`, the PII validators,
//! `detect_sensitive`, and `detect_patterns` with zero engine dependencies.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdfcore::redact::{
    classify, detect_patterns, detect_sensitive, looks_like_card, looks_like_email, looks_like_iban,
    looks_like_passport, looks_like_phone, looks_like_tckn, looks_like_tr_iban, looks_like_tr_phone,
    looks_like_tr_plate, looks_like_vkn, PatternOptions,
};
use pdfcore::textbox::{CharBox, PageChars, TextChar};

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    for token in text.split_whitespace().take(32) {
        let _ = classify(token);
        let _ = looks_like_tckn(token);
        let _ = looks_like_vkn(token);
        let _ = looks_like_tr_iban(token);
        let _ = looks_like_tr_plate(token);
        let _ = looks_like_tr_phone(token);
        let _ = looks_like_email(token);
        let _ = looks_like_iban(token);
        let _ = looks_like_card(token);
        let _ = looks_like_phone(token);
        let _ = looks_like_passport(token);
    }

    let chars: Vec<TextChar> = text
        .chars()
        .take(512)
        .enumerate()
        .map(|(index, ch)| TextChar {
            code: ch as u32,
            index: index as u32,
            box_rect: CharBox {
                left: (index as f64) * 6.0,
                bottom: 100.0,
                right: (index as f64 + 1.0) * 6.0,
                top: 112.0,
            },
            font_size_pt: 12.0,
            font_name: "Helvetica".into(),
            invisible: index % 17 == 0,
            generated: ch == ' ' && index % 5 == 0,
            hyphen: false,
        })
        .collect();
    let page = PageChars { page: 1, chars };
    let _ = detect_sensitive(&page);

    let pattern: String = text.chars().take(48).collect();
    let keyword: String = text.split_whitespace().next().unwrap_or("").chars().take(32).collect();
    let options = PatternOptions {
        keywords: if keyword.is_empty() { Vec::new() } else { vec![keyword] },
        pattern: if pattern.is_empty() { None } else { Some(pattern) },
        case_sensitive: data.first().is_some_and(|b| b & 1 == 1),
    };
    let _ = detect_patterns(&page, &options);
});
