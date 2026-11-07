//! Border width and dash pattern of an annotation.
//!
//! `/BS /W` is the current spelling, the third entry of a legacy `/Border`
//! array the old one; a dash pattern is only meaningful when `/BS /S` says
//! `/D`. A malformed pattern falls back to a solid line: the annotation is
//! still drawn, it just loses the dashes.

use super::super::super::dests::resolve;
use super::{number, number_text};
use lopdf::{Dictionary, Document, Object};

/// The `/BS /S` border style: how the frame is painted, not how thick it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Edge {
    /// A single line, the default.
    #[default]
    Plain,
    /// Two tones, light on the top and left, dark on the bottom and right.
    Beveled,
    /// The same two tones with the sides swapped, so the frame looks sunk.
    Inset,
    /// Only the bottom edge, the way a field underline is drawn.
    Underline,
}

/// A border: its width, a validated dash pattern and the style.
pub(super) struct Stroke {
    pub(super) width: f64,
    pub(super) dash: Option<Vec<f64>>,
    pub(super) edge: Edge,
}

impl Stroke {
    /// Writes the graphics state that every stroked path needs.
    pub(super) fn ops(&self, out: &mut String) {
        out.push_str(&format!("{} w\n", number_text(self.width)));
        if let Some(dash) = &self.dash {
            out.push('[');
            for (index, value) in dash.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                out.push_str(&number_text(*value));
            }
            out.push_str("] 0 d\n");
        }
    }
}

/// How far a bevel tone moves away from the stroke colour.
const BEVEL_TONE: f64 = 0.25;

/// The lighter tone of a bevel: RGB channels move toward white, a CMYK colour
/// is lightened by taking ink away.
pub(super) fn lighter(colour: &super::Color) -> super::Color {
    let values = colour
        .values
        .iter()
        .map(
            |value| {
                if colour.values.len() == 4 {
                    value * (1.0 - BEVEL_TONE)
                } else {
                    value + (1.0 - value) * BEVEL_TONE
                }
            },
        )
        .collect();
    super::Color { values }
}

/// The darker tone: RGB channels move toward black, a CMYK colour is darkened
/// by adding black ink only, which keeps the hue.
pub(super) fn darker(colour: &super::Color) -> super::Color {
    let mut values = colour.values.clone();
    if values.len() == 4 {
        let black = values[3];
        values[3] = black + (1.0 - black) * BEVEL_TONE;
    } else {
        for value in &mut values {
            *value *= 1.0 - BEVEL_TONE;
        }
    }
    super::Color { values }
}

/// Reads the border of one annotation; `None` when the width is unusable.
pub(super) fn stroke(doc: &Document, annot: &Dictionary) -> Option<Stroke> {
    let style = annot.get(b"BS").ok().map(|value| resolve(doc, value)).and_then(|value| value.as_dict().ok().cloned());
    let width = style
        .as_ref()
        .and_then(|dict| dict.get(b"W").ok().and_then(number))
        .or_else(|| {
            annot.get(b"Border").ok().and_then(|value| {
                let value = resolve(doc, value);
                let items = value.as_array().ok()?;
                items.get(2).and_then(number)
            })
        })
        .unwrap_or(1.0);
    if !width.is_finite() || !(0.0..=1_000.0).contains(&width) {
        return None;
    }
    let style_name =
        style.as_ref().and_then(|dict| dict.get(b"S").ok().and_then(|value| value.as_name().ok()).map(<[u8]>::to_vec));
    let edge = match style_name.as_deref() {
        Some(b"B") => Edge::Beveled,
        Some(b"I") => Edge::Inset,
        Some(b"U") => Edge::Underline,
        _ => Edge::Plain,
    };
    let dash = match style_name.as_deref() {
        // A dashed frame; any other style ignores a `/D` that may still be
        // there, as the specification says.
        Some(b"D") => match style.as_ref().and_then(|dict| dict.get(b"D").ok()) {
            Some(pattern) => valid_dash(doc, pattern),
            // `/S /D` without `/D` uses the specification's default.
            None => Some(vec![3.0]),
        },
        _ => None,
    };
    let dash = dash.or_else(|| legacy_dash(doc, annot));
    Some(Stroke { width, dash, edge })
}

/// The fourth entry of a legacy `/Border [h v w dash]` array.
fn legacy_dash(doc: &Document, annot: &Dictionary) -> Option<Vec<f64>> {
    let value = annot.get(b"Border").ok()?;
    let value = resolve(doc, value);
    let items = value.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    valid_dash(doc, &items[3])
}

/// A dash pattern the viewer can honour: one to sixteen finite, non-negative
/// numbers that do not add up to zero.
fn valid_dash(doc: &Document, pattern: &Object) -> Option<Vec<f64>> {
    let items = resolve(doc, pattern).as_array().ok()?;
    if items.is_empty() || items.len() > 16 {
        return None;
    }
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let value = number(item)?;
        if value < 0.0 {
            return None;
        }
        values.push(value);
    }
    (values.iter().sum::<f64>() > 0.0).then_some(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn doc() -> Document {
        Document::with_version("1.7")
    }

    fn annot(key: &str, value: Object) -> Dictionary {
        let mut dict = Dictionary::new();
        dict.set(key, value);
        dict
    }

    fn real(value: f64) -> Object {
        Object::Real(value as f32)
    }

    #[test]
    fn a_plain_annotation_gets_a_hairline() {
        let plain = stroke(&doc(), &Dictionary::new()).expect("stroke");
        assert_eq!(plain.width, 1.0);
        assert!(plain.dash.is_none());
    }

    #[test]
    fn a_dashed_style_needs_a_pattern() {
        let doc = doc();
        let with = annot(
            "BS",
            Object::Dictionary(
                dictionary! { "S" => "D", "W" => 2i64, "D" => Object::Array(vec![real(3.0), real(2.0)]) },
            ),
        );
        let parsed = stroke(&doc, &with).expect("stroke");
        assert_eq!(parsed.dash, Some(vec![3.0, 2.0]));
        assert_eq!(parsed.width, 2.0);

        let default = annot("BS", Object::Dictionary(dictionary! { "S" => "D" }));
        assert_eq!(stroke(&doc, &default).expect("stroke").dash, Some(vec![3.0]));

        let solid_with_pattern =
            annot("BS", Object::Dictionary(dictionary! { "S" => "S", "D" => Object::Array(vec![real(3.0)]) }));
        assert!(stroke(&doc, &solid_with_pattern).expect("stroke").dash.is_none());
    }

    #[test]
    fn a_broken_pattern_falls_back_to_a_solid_line() {
        let doc = doc();
        let cases = [
            vec![real(-1.0), real(2.0)],
            vec![real(0.0), real(0.0)],
            vec![Object::Name(b"X".to_vec())],
            Vec::new(),
            vec![real(1.0); 17],
        ];
        for case in cases {
            let annot = annot("BS", Object::Dictionary(dictionary! { "S" => "D", "D" => Object::Array(case.clone()) }));
            assert!(stroke(&doc, &annot).expect("stroke").dash.is_none(), "pattern {case:?} is invalid");
        }
    }

    #[test]
    fn the_legacy_border_carries_the_pattern() {
        let doc = doc();
        let annot = annot(
            "Border",
            Object::Array(vec![real(0.0), real(0.0), real(1.0), Object::Array(vec![real(4.0), real(1.0)])]),
        );
        let parsed = stroke(&doc, &annot).expect("stroke");
        assert_eq!(parsed.width, 1.0);
        assert_eq!(parsed.dash, Some(vec![4.0, 1.0]));
    }

    #[test]
    fn the_operator_keeps_a_solid_border_byte_identical() {
        let mut out = String::new();
        Stroke { width: 2.0, dash: None, edge: Edge::Plain }.ops(&mut out);
        assert_eq!(out, "2 w\n");
        let mut out = String::new();
        Stroke { width: 1.5, dash: Some(vec![3.0, 2.0]), edge: Edge::Plain }.ops(&mut out);
        assert_eq!(out, "1.5 w\n[3 2] 0 d\n");
    }
}
