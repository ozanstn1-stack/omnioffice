//! Line annotation captions: `/Cap`, `/CP` and `/CO`.
//!
//! A Line annotation may carry text that is "replicated as a caption in the
//! appearance of the line" (`/Cap true`, ISO 32000-1 §12.5.6.7 Table 175). When
//! the document has an appearance stream the caption is already inside it and
//! this module never runs; when the imposition synthesizes the appearance
//! because there is none, the caption would otherwise be lost with it.
//!
//! `/CP` picks the position - `/Inline` (the default) puts the text on the line
//! and breaks the stroke around it, `/Top` puts it above - and `/CO` moves it
//! from the line's midpoint, `h` along the line and `v` across it. A Line has no
//! `/DA`, so the caption is set in Helvetica at [`SIZE`] and filled with the
//! line's own colour, which is what viewers draw.
//!
//! Text the standard font cannot show means no caption at all: printing
//! replacement glyphs where the user wrote something else is worse than leaving
//! the line bare, which is the rule the FreeText slice set for synthesized text.

use lopdf::{Dictionary, Document, Object};

use crate::docutil::pdf_text_value;

use super::super::super::dests::resolve;
use super::paths::Point;
use super::std14::{Family, Std14};
use super::textfit::Std14Metrics;
use super::{number, number_text, textfit, union, Color};

/// The font a caption is set in. Helvetica is the reference implementation's
/// choice for annotation text and needs no embedded program.
pub(super) const HELVETICA: Std14 = Std14 { family: Family::Helvetica, bold: false, italic: false };

/// Caption size in points: the specification gives none for a Line, so this is
/// the size viewers in the wild use for annotation text.
const SIZE: f64 = 9.0;

/// AFM `Descender` magnitude of Helvetica, in 1/1000 em.
const DESCENT: f64 = 207.0;

/// Clear space between the caption and the broken stroke, in points.
const GAP: f64 = 1.0;

/// A caption is a caption, not a document: longer text is truncated.
const MAX_TEXT: usize = 1024;

/// Largest offset a `/CO` entry may carry (the extent of a user-space unit).
const MAX_OFFSET: f64 = 14_400.0;

/// What `/Cap`, `/CP` and `/CO` say, once the text is known to be printable.
struct Caption {
    codes: Vec<u8>,
    width: f64,
    top: bool,
    offset: (f64, f64),
}

/// The caption: operators, the box they cover, and where the stroke breaks.
pub(super) struct CaptionBox {
    pub(super) ops: String,
    pub(super) bbox: [f64; 4],
    pub(super) gap: Option<(Point, Point)>,
}

/// Builds the caption of a synthesized Line appearance, or `None` when the
/// annotation has none, its text cannot be shown, or the geometry is degenerate.
pub(super) fn synthesize(
    doc: &Document,
    annot: &Dictionary,
    start: Point,
    end: Point,
    stroke: f64,
    colour: &Color,
) -> Option<CaptionBox> {
    let caption = read(doc, annot)?;
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length = dx.hypot(dy);
    if length < 1e-9 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy, ux);
    let mid = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
    let ascent = SIZE * f64::from(HELVETICA.ascent()) / 1000.0;
    let descent = SIZE * DESCENT / 1000.0;

    // The baseline origin starts at the midpoint, shifted so the text is
    // centred on the line (`/Inline`) or sitting just above it (`/Top`).
    let (mut ox, mut oy) = (mid.0 - ux * caption.width / 2.0, mid.1 - uy * caption.width / 2.0);
    let mut gap = None;
    if caption.top {
        let lift = stroke / 2.0 + descent + GAP;
        ox += nx * lift;
        oy += ny * lift;
    } else {
        ox -= nx * ascent / 2.0;
        oy -= ny * ascent / 2.0;
        let (from, to) = (length / 2.0 - caption.width / 2.0 - GAP, length / 2.0 + caption.width / 2.0 + GAP);
        if from > 0.0 && to < length {
            gap = Some(((start.0 + ux * from, start.1 + uy * from), (start.0 + ux * to, start.1 + uy * to)));
        }
    }
    ox += ux * caption.offset.0 + nx * caption.offset.1;
    oy += uy * caption.offset.0 + ny * caption.offset.1;

    // The glyph box: along the line for the advance, and from the descender to
    // the ascender so a tail ('p', 'y') stays inside the box too.
    let across = |distance: f64| (ox + nx * distance, oy + ny * distance);
    let along = |distance: f64| (distance * ux, distance * uy);
    let (ax, ay) = along(caption.width);
    let corners = [
        across(-descent),
        (across(-descent).0 + ax, across(-descent).1 + ay),
        (across(ascent).0 + ax, across(ascent).1 + ay),
        across(ascent),
    ];
    let bbox = corners.iter().fold([corners[0].0, corners[0].1, corners[0].0, corners[0].1], |box_, corner| {
        [box_[0].min(corner.0), box_[1].min(corner.1), box_[2].max(corner.0), box_[3].max(corner.1)]
    });

    let mut ops = String::from("BT\n");
    ops.push_str(&format!("/FOmni0 {} Tf\n", number_text(SIZE)));
    ops.push_str(&colour.render(colour.fill_op()));
    ops.push_str(&format!(
        "{} {} {} {} {} {} Tm\n<{}> Tj\nET\n",
        number_text(ux),
        number_text(uy),
        number_text(-uy),
        number_text(ux),
        number_text(ox),
        number_text(oy),
        hex(&caption.codes)
    ));
    Some(CaptionBox { ops, bbox, gap })
}

/// Reads the caption a Line annotation describes, if it has one worth drawing.
fn read(doc: &Document, annot: &Dictionary) -> Option<Caption> {
    // `/Cap` is a boolean, and anything but `true` means "no caption".
    if !matches!(annot.get(b"Cap").ok(), Some(Object::Boolean(true))) {
        return None;
    }
    // `/Contents` is a text string; a name or a number there is not a caption.
    let contents = resolve(doc, annot.get(b"Contents").ok()?);
    if !matches!(contents, Object::String(_, _)) {
        return None;
    }
    let text = pdf_text_value(contents)?;
    let flat = text.chars().map(|symbol| if symbol.is_control() { ' ' } else { symbol }).collect::<String>();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    // A character the standard font cannot show means no caption: the rule for
    // synthesized text is to omit it rather than print a replacement glyph.
    let mut codes = textfit::encode(&flat)?;
    codes.truncate(MAX_TEXT);
    let width = f64::from(textfit::width_of(&Std14Metrics(HELVETICA), &codes)) * SIZE / 1000.0;
    if !width.is_finite() || width <= 0.0 {
        return None;
    }
    let top =
        matches!(annot.get(b"CP").ok().map(|value| resolve(doc, value)), Some(Object::Name(name)) if name == b"Top");
    Some(Caption { codes, width, top, offset: offset_of(doc, annot) })
}

/// `/CO`, clamped: a missing, malformed or non-finite entry means the middle.
fn offset_of(doc: &Document, annot: &Dictionary) -> (f64, f64) {
    let items = annot.get(b"CO").ok().map(|value| resolve(doc, value)).and_then(|value| value.as_array().ok().cloned());
    let Some(items) = items.filter(|items| items.len() == 2) else { return (0.0, 0.0) };
    let read = |value: &Object| {
        number(value).filter(|number| number.is_finite()).unwrap_or(0.0).clamp(-MAX_OFFSET, MAX_OFFSET)
    };
    (read(&items[0]), read(&items[1]))
}

/// Hex, because a hex string needs no escaping for `(`, `)` or `\`.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// The widest the caption makes the annotation's box: used by the caller to
/// keep `/BBox` around the text as well as the line.
pub(super) fn union_with(box_: [f64; 4], caption: &CaptionBox) -> [f64; 4] {
    union(box_, caption.bbox)
}

#[cfg(test)]
mod tests {
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
    fn the_caption_keeps_one_line() {
        let caption = caption_of(&line_annot(vec![
            ("Cap", true.into()),
            ("Contents", Object::string_literal("first\r\nsecond\tthird")),
        ]))
        .expect("caption");
        assert_eq!(String::from_utf8_lossy(&caption.codes), "first second third");
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
        assert_eq!(caption.codes.len(), 5, "one code per character");
    }

    #[test]
    fn the_width_comes_from_the_afm_table() {
        let caption =
            caption_of(&line_annot(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]))
                .expect("caption");
        // 2278/1000 em at 9 pt.
        assert!((caption.width - 20.502).abs() < 1e-9, "{}", caption.width);
    }

    #[test]
    fn an_inline_caption_breaks_the_line_around_it() {
        let caption =
            build(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]).expect("caption");
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
            let annot =
                line_annot(vec![("Cap", true.into()), ("CO", value), ("Contents", Object::string_literal("Hi"))]);
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
        let caption =
            build(vec![("Cap", true.into()), ("Contents", Object::string_literal("Hello"))]).expect("caption");
        let union = union_with([0.0, 9.0, 100.0, 11.0], &caption);
        assert!(union[1] <= caption.bbox[1] && union[3] >= caption.bbox[3], "{union:?}");
    }
}
