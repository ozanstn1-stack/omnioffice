//! The cloudy outline a Square or Circle annotation asks for with `/BE`.
//!
//! `/BE` is a border-effect dictionary: `/S /C` is "cloudy" and `/I` an
//! intensity from 0 to 2. A viewer draws that frame as a chain of outward
//! scallops around the annotation's box - the shape a revision cloud uses - and
//! this module draws the same from the geometry, because the annotation has no
//! appearance stream to copy.
//!
//! Only `/S /C` is handled: every other effect leaves the plain frame alone
//! (the caller decides that). A shape whose perimeter cannot carry three
//! scallops, or one that would need more than [`MAX_SCALLOPS`], also falls back
//! to the plain frame: a one-bump cloud is not a cloud.

use lopdf::{Dictionary, Document};

use super::super::super::dests::resolve;
use super::super::Synth;
use super::shape::ShapeKind;
use super::{border, color, number_text, KAPPA};

/// A scallop is never smaller than this many points across, so a thin frame
/// still reads as cloudy rather than furry.
const MIN_SCALLOP: f64 = 3.0;

/// More bumps than this is a texture, not a border.
const MAX_SCALLOPS: usize = 2000;

/// What `/BE` says, once it has been recognised as a cloudy effect.
pub(super) struct Cloudy {
    /// `/I`, clamped to 0..=2.
    pub(super) intensity: f64,
}

/// Reads a cloudy `/BE`, or `None` for every other border effect.
pub(super) fn read(doc: &Document, annot: &Dictionary) -> Option<Cloudy> {
    let effect = annot.get(b"BE").ok().map(|value| resolve(doc, value))?;
    let effect = effect.as_dict().ok()?;
    if effect.get(b"S").ok().and_then(|value| value.as_name().ok()) != Some(b"C") {
        return None;
    }
    let intensity = effect
        .get(b"I")
        .ok()
        .and_then(|value| value.as_float().ok())
        .filter(|value| value.is_finite())
        .map_or(0.0, f64::from)
        .clamp(0.0, 2.0);
    Some(Cloudy { intensity })
}

/// Builds the cloudy frame around `rect`: filled with `/IC` and stroked with
/// `/C`, exactly like the plain frame it replaces.
pub(super) fn synthesize(
    doc: &Document,
    annot: &Dictionary,
    kind: ShapeKind,
    rect: [f64; 4],
    stroke: &border::Stroke,
    cloudy: &Cloudy,
) -> Option<Synth> {
    let [x0, y0, x1, y1] = rect;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let outline = color(doc, annot, b"C")?;
    let fill = color(doc, annot, b"IC")?;
    let has_stroke = outline.is_some() && stroke.width > 0.0;
    if outline.is_none() && fill.is_none() {
        return None;
    }
    // A heavier frame and a stronger `/I` make a coarser cloud. The floor keeps
    // a hairline frame from turning into a furry edge.
    let radius = (2.0 * stroke.width + 2.0).max(MIN_SCALLOP) * (1.0 + 0.5 * cloudy.intensity);
    // A bump as big as the shape is not a cloud: the plain frame is drawn.
    if x1 - x0 < 2.0 * radius || y1 - y0 < 2.0 * radius {
        return None;
    }
    let path = match kind {
        ShapeKind::Square => rect_path(rect, radius)?,
        ShapeKind::Circle => ellipse_path(rect, radius)?,
    };
    let mut ops = String::new();
    if has_stroke {
        if let Some(outline) = &outline {
            ops.push_str(&outline.render(outline.stroke_op()));
        }
        stroke.ops(&mut ops);
    }
    if let Some(fill) = &fill {
        ops.push_str(&fill.render(fill.fill_op()));
    }
    ops.push_str(&path);
    ops.push_str(if has_stroke && fill.is_some() {
        "B\n"
    } else if has_stroke {
        "S\n"
    } else {
        "f\n"
    });
    // The bumps bulge outward, so the box grows with them.
    let grown = [x0 - radius, y0 - radius, x1 + radius, y1 + radius];
    Some(Synth { ops: ops.into_bytes(), bbox: grown, multiply: false, font: None })
}

/// Scallops around a rectangle, walked counter-clockwise so the outward normal
/// is the right one.
fn rect_path(rect: [f64; 4], radius: f64) -> Option<String> {
    let [x0, y0, x1, y1] = rect;
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let mut path = String::new();
    let mut total = 0usize;
    for index in 0..4 {
        let from = corners[index];
        let to = corners[(index + 1) % 4];
        let count = ((to.0 - from.0).hypot(to.1 - from.1) / (2.0 * radius)).round().max(1.0) as usize;
        total += count;
        for step in 0..count {
            let start = lerp(from, to, step as f64 / count as f64);
            let end = lerp(from, to, (step + 1) as f64 / count as f64);
            scallop(&mut path, start, end);
        }
    }
    closed(path, total)
}

/// Scallops around an ellipse, sampled at equal angles.
fn ellipse_path(rect: [f64; 4], radius: f64) -> Option<String> {
    let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
    let (rx, ry) = ((rect[2] - rect[0]) / 2.0, (rect[3] - rect[1]) / 2.0);
    // Ramanujan's perimeter approximation: it only decides how many bumps fit.
    let perimeter = std::f64::consts::PI * (3.0 * (rx + ry) - ((3.0 * rx + ry) * (rx + 3.0 * ry)).sqrt());
    let count = (perimeter / (2.0 * radius)).round() as usize;
    if count > MAX_SCALLOPS {
        return None;
    }
    let point = |index: usize| {
        let angle = std::f64::consts::TAU * (index as f64) / count.max(1) as f64;
        (cx + rx * angle.cos(), cy + ry * angle.sin())
    };
    let mut path = String::new();
    for index in 0..count {
        scallop(&mut path, point(index), point(index + 1));
    }
    closed(path, count)
}

/// One outward bump from `start` to `end`: the semicircle on that chord, drawn
/// as two quarter arcs.
fn scallop(path: &mut String, start: (f64, f64), end: (f64, f64)) {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let chord = dx.hypot(dy);
    if chord < 1e-9 {
        return;
    }
    let (ux, uy) = (dx / chord, dy / chord);
    // A counter-clockwise walk keeps the inside on its left, so the outward
    // normal is the one on its right.
    let normal = (uy, -ux);
    let middle = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
    let half = chord / 2.0;
    arc(path, middle, (ux, uy), normal, half, 0.0, std::f64::consts::FRAC_PI_2, true);
    arc(path, middle, (ux, uy), normal, half, std::f64::consts::FRAC_PI_2, std::f64::consts::PI, false);
}

/// A quarter of the bump, from angle `t0` to `t1` around the chord's middle.
#[allow(clippy::too_many_arguments)]
fn arc(
    path: &mut String,
    middle: (f64, f64),
    along: (f64, f64),
    normal: (f64, f64),
    half: f64,
    t0: f64,
    t1: f64,
    starts_path: bool,
) {
    let point = |t: f64| {
        (
            middle.0 - along.0 * half * t.cos() + normal.0 * half * t.sin(),
            middle.1 - along.1 * half * t.cos() + normal.1 * half * t.sin(),
        )
    };
    let tangent = |t: f64| (along.0 * t.sin() + normal.0 * t.cos(), along.1 * t.sin() + normal.1 * t.cos());
    let (from, to) = (point(t0), point(t1));
    let k = KAPPA * half;
    let (out, into) = (tangent(t0), tangent(t1));
    if starts_path {
        path.push_str(&format!("{} {} m\n", number_text(from.0), number_text(from.1)));
    }
    path.push_str(&format!(
        "{} {} {} {} {} {} c\n",
        number_text(from.0 + out.0 * k),
        number_text(from.1 + out.1 * k),
        number_text(to.0 - into.0 * k),
        number_text(to.1 - into.1 * k),
        number_text(to.0),
        number_text(to.1)
    ));
}

fn lerp(from: (f64, f64), to: (f64, f64), t: f64) -> (f64, f64) {
    (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
}

/// Closes the path, or refuses it when it cannot be a cloud.
fn closed(path: String, count: usize) -> Option<String> {
    if !(3..=MAX_SCALLOPS).contains(&count) || path.is_empty() {
        return None;
    }
    Some(format!("{path}h\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object};

    fn annot(effect: Dictionary) -> Dictionary {
        dictionary! {
            "Type" => "Annot",
            "Subtype" => "Square",
            "Rect" => vec![0.into(), 0.into(), 200.into(), 100.into()],
            "C" => vec![0.into(), 0.into(), 0.into()],
            "BE" => Object::Dictionary(effect),
        }
    }

    fn read_effect(intensity: Object) -> Option<Cloudy> {
        let doc = Document::with_version("2.0");
        let annotation = annot(dictionary! { "S" => "C", "I" => intensity });
        read(&doc, &annotation)
    }

    #[test]
    fn only_a_cloudy_effect_is_a_cloud() {
        assert!(read_effect(1.into()).is_some());
        let doc = Document::with_version("2.0");
        let solid = annot(dictionary! { "S" => "S" });
        assert!(read(&doc, &solid).is_none(), "/S /S is the plain frame");
        let missing = dictionary! { "Type" => "Annot", "Subtype" => "Square" };
        assert!(read(&doc, &missing).is_none(), "no effect dictionary at all");
    }

    #[test]
    fn the_intensity_is_clamped_and_defaulted() {
        assert_eq!(read_effect((-5).into()).expect("cloudy").intensity, 0.0);
        assert_eq!(read_effect(7.into()).expect("cloudy").intensity, 2.0);
        assert_eq!(read_effect(Object::Real(1.5)).expect("cloudy").intensity, 1.5);
        assert_eq!(read_effect(Object::string_literal("x")).expect("cloudy").intensity, 0.0);
    }

    fn cloud(kind: ShapeKind, rect: [f64; 4], intensity: f64) -> Option<Synth> {
        let doc = Document::with_version("2.0");
        let annotation = annot(dictionary! { "S" => "C", "I" => Object::Real(intensity as f32) });
        let stroke = border::Stroke { width: 1.0, dash: None, edge: border::Edge::Plain };
        synthesize(&doc, &annotation, kind, rect, &stroke, &read(&doc, &annotation).expect("cloudy"))
    }

    #[test]
    fn a_cloudy_square_is_a_closed_chain_of_scallops() {
        let synth = cloud(ShapeKind::Square, [0.0, 0.0, 200.0, 100.0], 0.0).expect("cloud");
        let ops = String::from_utf8(synth.ops).expect("utf8");
        assert_eq!(ops.matches(" c\n").count(), 2 * synth_scallops(&ops), "two arcs per bump: {ops:?}");
        assert!(ops.ends_with("h\nS\n"), "the path is closed and stroked: {ops:?}");
        assert!(ops.starts_with("0 0 0 RG\n1 w\n"), "the stroke colour and width come first: {ops:?}");
        // Every bump stays inside the box the caller was told about.
        assert!(synth.bbox[0] < 0.0 && synth.bbox[1] < 0.0 && synth.bbox[2] > 200.0 && synth.bbox[3] > 100.0);
    }

    fn synth_scallops(ops: &str) -> usize {
        ops.matches(" c\n").count() / 2
    }

    #[test]
    fn a_stronger_intensity_makes_fewer_bumps() {
        let weak = cloud(ShapeKind::Square, [0.0, 0.0, 400.0, 200.0], 0.0).expect("cloud");
        let strong = cloud(ShapeKind::Square, [0.0, 0.0, 400.0, 200.0], 2.0).expect("cloud");
        let bumps = |synth: &Synth| synth_scallops(&String::from_utf8(synth.ops.clone()).expect("utf8"));
        assert!(bumps(&weak) > bumps(&strong), "{} vs {}", bumps(&weak), bumps(&strong));
    }

    #[test]
    fn an_ellipse_carries_scallops_too() {
        let synth = cloud(ShapeKind::Circle, [0.0, 0.0, 200.0, 100.0], 0.0).expect("cloud");
        let ops = String::from_utf8(synth.ops).expect("utf8");
        assert!(synth_scallops(&ops) >= 3, "{ops:?}");
        assert!(ops.ends_with("h\nS\n"));
    }

    #[test]
    fn a_degenerate_box_has_no_cloud() {
        assert!(cloud(ShapeKind::Square, [10.0, 10.0, 10.0, 50.0], 1.0).is_none());
        assert!(cloud(ShapeKind::Circle, [10.0, 10.0, 10.0, 10.0], 1.0).is_none());
        // A box smaller than its own bumps falls back to the plain frame.
        assert!(cloud(ShapeKind::Square, [0.0, 0.0, 4.0, 4.0], 0.0).is_none());
        assert!(cloud(ShapeKind::Circle, [0.0, 0.0, 200.0, 4.0], 0.0).is_none());
    }

    #[test]
    fn the_fill_and_the_stroke_share_one_path() {
        let doc = Document::with_version("2.0");
        let mut annot = annot(dictionary! { "S" => "C" });
        annot.set("IC", Object::Array(vec![Object::Real(1.0), Object::Real(1.0), Object::Real(1.0)]));
        let stroke = border::Stroke { width: 2.0, dash: None, edge: border::Edge::Plain };
        let synth =
            synthesize(&doc, &annot, ShapeKind::Square, [0.0, 0.0, 200.0, 100.0], &stroke, &Cloudy { intensity: 0.0 })
                .expect("cloud");
        let ops = String::from_utf8(synth.ops).expect("utf8");
        assert!(ops.contains("1 1 1 rg\n"), "the interior colour is set: {ops:?}");
        assert!(ops.ends_with("B\n"), "one path, painted both ways");
    }
}
