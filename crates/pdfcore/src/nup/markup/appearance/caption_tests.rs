//! Tests for the Line caption: what it reads, where it sits and what it covers.

use super::super::Color;
use super::*;
use lopdf::{dictionary, Dictionary, Document, Object};

fn line_annot(extra: Vec<(&str, Object)>) -> Dictionary {
    let mut annot = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        "C" => vec![0.into(), 0.into(), 0.into()],
    };
    for (key, value) in extra {
        annot.set(key.as_bytes().to_vec(), value);
    }
    annot
}

fn black() -> Color {
    let doc = Document::with_version("2.0");
    let annot = line_annot(vec![]);
    super::super::color(&doc, &annot, b"C").flatten().expect("colour")
}

fn caption_of(annot: &Dictionary) -> Option<Caption> {
    read(&Document::with_version("2.0"), annot)
}

fn build(extra: Vec<(&str, Object)>) -> Option<CaptionBox> {
    let doc = Document::with_version("2.0");
    let annot = line_annot(extra);
    synthesize(&doc, &annot, (0.0, 10.0), (100.0, 10.0), 1.0, &black())
}

#[test]
fn a_line_without_the_flag_has_no_caption() {
    assert!(build(vec![("Contents", Object::string_literal("Hello"))]).is_none());
}

#[test]
fn a_flag_that_is_not_a_boolean_is_no_caption() {
    // /Cap is a boolean in the specification; a number is not "true".
    assert!(build(vec![("Cap", Object::Integer(1)), ("Contents", Object::string_literal("Hello"))]).is_none());
    assert!(build(vec![("Cap", Object::Boolean(false)), ("Contents", Object::string_literal("Hello"))]).is_none());
}

#[test]
fn an_empty_or_missing_text_is_no_caption() {
    for contents in [Object::string_literal(""), Object::string_literal("   "), Object::Name(b"Body".to_vec())] {
        assert!(build(vec![("Cap", true.into()), ("Contents", contents)]).is_none());
    }
    assert!(build(vec![("Cap", true.into())]).is_none());
}

#[test]
fn text_the_font_cannot_show_means_no_caption() {
    // A CJK character has no WinAnsi code and no /Differences glyph: the
    // caption is dropped instead of printed as '?'.
    assert!(build(vec![("Cap", true.into()), ("Contents", Object::string_literal("報告"))]).is_none());
    // Turkish letters do have one, so they survive.
    assert!(build(vec![("Cap", true.into()), ("Contents", Object::string_literal("Ölçüm ğ ş ı"))]).is_some());
}

#[test]
fn a_caption_winansi_cannot_show_is_set_in_the_bundled_face() {
    // Cyrillic has neither a WinAnsi code nor a /Differences glyph, but the
    // bundled face carries it: the caption is set in a subset instead of being
    // dropped, which is what used to happen.
    let caption = caption_of(&line_annot(vec![("Cap", true.into()), ("Contents", Object::string_literal("Привет Ω"))]))
        .expect("caption");
    let Codes::Subset { hex, chars } = &caption.codes else { panic!("expected the subset face") };
    assert_eq!(chars.len(), "Привет Ω".chars().count(), "every drawn character is in the subset");
    assert!(hex.starts_with('<') && hex.ends_with('>'), "{hex}");
    assert_eq!(hex.len(), 2 + chars.len() * 4, "two bytes per glyph id");
    assert!(caption.width > 0.0);
    // The glyph ids are the bundled face's own, not the character codes.
    let face = crate::textfont::SubsetFace::bundled().expect("bundled face");
    assert_eq!(hex, &face.encode("Привет Ω"));
}

#[test]
fn a_caption_the_bundled_face_cannot_show_is_still_dropped() {
    assert!(build(vec![("Cap", true.into()), ("Contents", Object::string_literal("報告書"))]).is_none());
}

#[test]
fn the_caption_keeps_one_line() {
    let caption = caption_of(&line_annot(vec![
        ("Cap", true.into()),
        ("Contents", Object::string_literal("first\r\nsecond\tthird")),
    ]))
    .expect("caption");
    let Codes::WinAnsi(codes) = &caption.codes else { panic!("WinAnsi") };
    assert_eq!(String::from_utf8_lossy(codes), "first second third");
}

#[test]
fn the_utf16_form_of_contents_is_read() {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in "Sıcak".encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    let caption = caption_of(&line_annot(vec![
        ("Cap", true.into()),
        ("Contents", Object::String(bytes, lopdf::StringFormat::Literal)),
    ]))
    .expect("caption");
    let Codes::WinAnsi(codes) = &caption.codes else { panic!("WinAnsi") };
    assert_eq!(codes.len(), 5, "one code per character");
}

#[test]
fn the_width_comes_from_the_afm_table() {
    let caption = caption_of(&line_annot(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]))
        .expect("caption");
    // 2278/1000 em at 9 pt.
    assert!((caption.width - 20.502).abs() < 1e-9, "{}", caption.width);
}

#[test]
fn an_inline_caption_breaks_the_line_around_it() {
    let caption = build(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]).expect("caption");
    let (from, to) = caption.gap.expect("the stroke breaks");
    assert!((from.0 - (50.0 - 20.502 / 2.0 - GAP)).abs() < 1e-6, "{from:?}");
    assert!((to.0 - (50.0 + 20.502 / 2.0 + GAP)).abs() < 1e-6, "{to:?}");
    assert!((from.1 - 10.0).abs() < 1e-9 && (to.1 - 10.0).abs() < 1e-9, "the break stays on the line");
    assert!(caption.ops.contains("<48656C6C6F> Tj"), "{}", caption.ops);
    assert!(caption.ops.contains("/FOmni0 9 Tf"), "{}", caption.ops);
}

#[test]
fn a_top_caption_sits_above_the_line_and_never_breaks_it() {
    let caption = build(vec![
        ("Cap", true.into()),
        ("CP", Object::Name(b"Top".to_vec())),
        ("Contents", Object::string_literal("Hello")),
    ])
    .expect("caption");
    assert!(caption.gap.is_none(), "a caption above the line does not need a gap");
    assert!(caption.bbox[1] > 10.0, "the box grew above the line: {:?}", caption.bbox);
    assert!(caption.bbox[3] > 10.0 + 6.0, "and it contains the text: {:?}", caption.bbox);
}

#[test]
fn an_unknown_position_falls_back_to_inline() {
    let caption = build(vec![
        ("Cap", true.into()),
        ("CP", Object::Name(b"Bogus".to_vec())),
        ("Contents", Object::string_literal("Hello")),
    ])
    .expect("caption");
    assert!(caption.gap.is_some(), "an unknown /CP behaves like /Inline");
}

#[test]
fn a_broken_offset_puts_the_caption_back_on_the_middle() {
    for value in [
        Object::Array(vec![1.into()]),
        Object::Array(vec![1.into(), 2.into(), 3.into()]),
        Object::string_literal("x"),
        Object::Array(vec![Object::Real(1e30), Object::Real(-1e30)]),
        Object::Array(vec![Object::Real(f32::INFINITY), Object::Real(f32::NAN)]),
    ] {
        let annot = line_annot(vec![("Cap", true.into()), ("CO", value), ("Contents", Object::string_literal("Hi"))]);
        let offset = offset_of(&Document::with_version("2.0"), &annot);
        assert!(offset.0.abs() <= MAX_OFFSET && offset.1.abs() <= MAX_OFFSET, "{offset:?}");
    }
    let annot = line_annot(vec![
        ("Cap", true.into()),
        ("CO", Object::Array(vec![Object::Real(10.0), Object::Real(5.0)])),
        ("Contents", Object::string_literal("Hi")),
    ]);
    assert_eq!(offset_of(&Document::with_version("2.0"), &annot), (10.0, 5.0));
}

#[test]
fn the_offset_moves_the_caption_along_and_across_the_line() {
    let plain = build(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]).expect("caption");
    let moved = build(vec![
        ("Cap", true.into()),
        ("CO", Object::Array(vec![10.into(), 5.into()])),
        ("Contents", Object::string_literal("Hello")),
    ])
    .expect("caption");
    assert!((moved.bbox[0] - (plain.bbox[0] + 10.0)).abs() < 1e-6, "{:?} {:?}", plain.bbox, moved.bbox);
    assert!((moved.bbox[1] - (plain.bbox[1] + 5.0)).abs() < 1e-6);
}

#[test]
fn a_diagonal_line_turns_the_text_with_it() {
    let doc = Document::with_version("2.0");
    let annot = line_annot(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hi"))]);
    let caption = synthesize(&doc, &annot, (0.0, 0.0), (100.0, 100.0), 1.0, &black()).expect("caption");
    // tm = [cos sin -sin cos ...] with 45 degrees.
    let tm: Vec<f64> = caption
        .ops
        .lines()
        .find(|line| line.ends_with(" Tm"))
        .expect("Tm")
        .split_whitespace()
        .take(4)
        .map(|value| value.parse().expect("number"))
        .collect();
    assert!((tm[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3, "{tm:?}");
    assert!((tm[1] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3, "{tm:?}");
}

#[test]
fn text_wider_than_the_line_is_drawn_without_breaking_it() {
    let doc = Document::with_version("2.0");
    let annot =
        line_annot(vec![("Cap", true.into()), ("Contents", Object::string_literal("a very long caption indeed"))]);
    let caption = synthesize(&doc, &annot, (0.0, 0.0), (10.0, 0.0), 1.0, &black()).expect("caption");
    assert!(caption.gap.is_none(), "there is no room for a gap, so the line stays whole");
}

#[test]
fn a_degenerate_line_has_no_caption() {
    let doc = Document::with_version("2.0");
    let annot = line_annot(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]);
    assert!(synthesize(&doc, &annot, (5.0, 5.0), (5.0, 5.0), 1.0, &black()).is_none());
}

#[test]
fn the_box_contains_the_whole_caption() {
    let caption = build(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]).expect("caption");
    let union = union_with([0.0, 9.0, 100.0, 11.0], &caption);
    assert!(union[1] <= caption.bbox[1] && union[3] >= caption.bbox[3], "{union:?}");
}
