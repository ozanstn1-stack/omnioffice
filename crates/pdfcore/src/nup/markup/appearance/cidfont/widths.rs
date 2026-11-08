//! `/W`, the per-CID advance widths of a composite font, and `/DW`, the width
//! for every CID the array does not mention.
//!
//! `/W` is an untrusted numeric array with two shapes that may be mixed in one
//! array: `c [w1 w2 …]` gives consecutive widths from CID `c`, and
//! `c_first c_last w` gives one width to a whole range. A malformed array
//! rejects the **font**, not just the entry: a dropped entry would silently fall
//! back to `/DW`, which is a guess about a width we cannot read.
//!
//! The runs are kept sorted and searched with a partition point, so memory
//! follows the number of entries the file actually wrote - a `HashMap` would
//! expand a single `0 65535 w` into 65 536 entries.

use lopdf::{Document, Object};

use super::super::super::super::dests::resolve;
use super::super::super::super::links::number;

/// The most numbers a `/W` array may mention, its outer and inner arrays
/// together.
pub(super) const MAX_W_ITEMS: usize = 65_536;

/// The widest advance accepted for one glyph: five ems.
pub(super) const MAX_ADVANCE: f32 = 5000.0;

/// One run of consecutive CIDs that share a width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Run {
    first: u16,
    last: u16,
    width: f32,
}

/// The widths of one font, sorted and non-overlapping.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Widths {
    runs: Vec<Run>,
    default: f32,
}

impl Widths {
    /// The advance of `cid` in 1/1000 em; `/DW` when no run covers it.
    pub(super) fn get(&self, cid: u16) -> f32 {
        let index = self.runs.partition_point(|run| run.last < cid);
        match self.runs.get(index) {
            Some(run) if run.first <= cid => run.width,
            _ => self.default,
        }
    }
}

/// Reads `/W` and `/DW`; `None` when either is malformed.
pub(super) fn parse(doc: &Document, w: Option<&Object>, dw: Option<&Object>) -> Option<Widths> {
    let default = match dw {
        None => 1000.0,
        Some(value) => advance(value)?,
    };
    let mut runs = match w {
        None => Vec::new(),
        Some(value) => read_w(doc, value)?,
    };
    runs.sort_unstable_by_key(|run| run.first);
    if runs.windows(2).any(|pair| pair[0].last >= pair[1].first) {
        // Viewers disagree on first-wins against last-wins: any choice is a
        // guess, so the font is refused instead.
        return None;
    }
    Some(Widths { runs, default })
}

/// The runs of one `/W` array, in the order the file wrote them.
fn read_w(doc: &Document, value: &Object) -> Option<Vec<Run>> {
    let items = resolve(doc, value).as_array().ok()?;
    let mut runs: Vec<Run> = Vec::new();
    let mut used = 0usize;
    let mut index = 0usize;
    while index < items.len() {
        let first = cid(&items[index])?;
        used += 1;
        if used > MAX_W_ITEMS {
            return None;
        }
        index += 1;
        match items.get(index).map(|item| resolve(doc, item)) {
            Some(Object::Array(inner)) => {
                used += inner.len();
                if inner.is_empty() || used > MAX_W_ITEMS {
                    return None;
                }
                let last = u32::from(first).checked_add(inner.len() as u32 - 1)?;
                if last > u32::from(u16::MAX) {
                    return None;
                }
                for (offset, item) in inner.iter().enumerate() {
                    let width = advance(item)?;
                    let cid = first + offset as u16;
                    runs.push(Run { first: cid, last: cid, width });
                }
                index += 1;
            }
            _ => {
                let last = cid(resolve(doc, items.get(index)?))?;
                let width = advance(resolve(doc, items.get(index + 1)?))?;
                used += 2;
                if used > MAX_W_ITEMS || last < first {
                    return None;
                }
                runs.push(Run { first, last, width });
                index += 2;
            }
        }
    }
    Some(runs)
}

/// A CID: an integer in the two-byte code space, never a real.
fn cid(value: &Object) -> Option<u16> {
    match value {
        Object::Integer(value) => u16::try_from(*value).ok(),
        _ => None,
    }
}

/// A width: a finite advance up to [`MAX_ADVANCE`].
fn advance(value: &Object) -> Option<f32> {
    let width = number(value)? as f32;
    (width.is_finite() && (0.0..=MAX_ADVANCE).contains(&width)).then_some(width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document::with_version("1.7")
    }

    fn integers(values: &[i64]) -> Object {
        Object::Array(values.iter().map(|value| Object::Integer(*value)).collect())
    }

    fn parsed(w: Object, dw: Option<i64>) -> Option<Widths> {
        let doc = doc();
        let dw = dw.map(Object::Integer);
        parse(&doc, Some(&w), dw.as_ref())
    }

    #[test]
    fn both_forms_are_read_and_mixed() {
        // `10 [500 600]` then `20 22 700`.
        let w = Object::Array(vec![
            Object::Integer(10),
            integers(&[500, 600]),
            Object::Integer(20),
            Object::Integer(22),
            Object::Integer(700),
        ]);
        let widths = parsed(w, None).expect("widths");
        assert_eq!(widths.get(10), 500.0);
        assert_eq!(widths.get(11), 600.0);
        assert_eq!(widths.get(20), 700.0);
        assert_eq!(widths.get(21), 700.0);
        assert_eq!(widths.get(22), 700.0);
        assert_eq!(widths.get(23), 1000.0, "a CID no run covers takes /DW");
    }

    #[test]
    fn the_default_width_is_read_and_bounded() {
        let w = Object::Array(vec![Object::Integer(1), integers(&[500])]);
        assert_eq!(parsed(w.clone(), Some(0)).expect("widths").get(9), 0.0);
        assert_eq!(parsed(w.clone(), Some(4321)).expect("widths").get(9), 4321.0);
        assert!(parsed(w.clone(), Some(-1)).is_none(), "a negative default is refused");
        assert!(parsed(w, Some(5001)).is_none(), "past five ems is not a glyph advance");
    }

    #[test]
    fn a_malformed_array_rejects_the_font() {
        assert!(parsed(integers(&[1]), None).is_none(), "a CID with no width");
        assert!(parsed(integers(&[1, 2]), None).is_none(), "a range with no width");
        assert!(parsed(integers(&[5, 3, 1000]), None).is_none(), "a reversed range");
        assert!(parsed(Object::Array(vec![Object::Integer(1), Object::Array(vec![])]), None).is_none());
        assert!(parsed(integers(&[65536, 65536, 500]), None).is_none(), "past the code space");
        assert!(parsed(integers(&[-1, 500]), None).is_none(), "a negative CID");
        assert!(parsed(integers(&[1, -1]), None).is_none(), "a negative width");
        assert!(parsed(integers(&[1, 5001]), None).is_none(), "a width past five ems");
        assert!(parsed(integers(&[1, 2, 3, 4, 5]), None).is_none(), "an odd tail");
        assert!(parsed(Object::Array(vec![Object::Real(1.5), Object::Integer(500)]), None).is_none());
        assert!(parsed(Object::Name(b"Identity".to_vec()), None).is_none(), "not an array");
    }

    #[test]
    fn an_inner_array_that_leaves_the_code_space_is_refused() {
        // Two CIDs from 65535 would need 65536.
        assert!(parsed(Object::Array(vec![Object::Integer(65535), integers(&[500, 600])]), None).is_none());
        // The last CID of the space is fine.
        let edge = parsed(integers(&[65534, 65535, 400]), None).expect("widths");
        assert_eq!(edge.get(65535), 400.0);
    }

    #[test]
    fn overlapping_runs_are_refused() {
        let w = Object::Array(vec![
            Object::Integer(10),
            Object::Integer(20),
            Object::Integer(500),
            Object::Integer(15),
            Object::Integer(600),
        ]);
        assert!(parsed(w, None).is_none(), "viewers disagree on which run wins");
    }

    #[test]
    fn an_absent_array_leaves_every_cid_at_the_default() {
        let doc = doc();
        let widths = parse(&doc, None, None).expect("widths");
        assert_eq!(widths.get(0), 1000.0);
        assert_eq!(widths.get(65535), 1000.0);
    }

    #[test]
    fn a_huge_inner_array_is_bounded() {
        let inner: Vec<Object> = (0..MAX_W_ITEMS + 1).map(|_| Object::Integer(500)).collect();
        let w = Object::Array(vec![Object::Integer(0), Object::Array(inner)]);
        assert!(parsed(w, None).is_none());
    }

    #[test]
    fn random_arrays_never_panic() {
        // A fixed-seed generator: the point is determinism, not statistics.
        let mut seed: u64 = 0x5DEE_CE66_D1CE_1234;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as i64
        };
        let doc = doc();
        for _ in 0..10_000 {
            let length = (next().unsigned_abs() % 12) as usize;
            let mut items = Vec::with_capacity(length);
            for _ in 0..length {
                let roll = next().unsigned_abs() % 4;
                let value = match roll {
                    0 => Object::Integer(next() % 70_000),
                    1 => Object::Integer(next() % 6_000),
                    2 => Object::Array(vec![Object::Integer(next() % 1_200); (next().unsigned_abs() % 4) as usize]),
                    _ => Object::Real(next() as f32 / 97.0),
                };
                items.push(value);
            }
            let w = Object::Array(items);
            if let Some(widths) = parse(&doc, Some(&w), None) {
                // Whatever it accepted must answer every CID without panicking.
                for cid in [0u16, 1, 3, 65_535] {
                    let width = widths.get(cid);
                    assert!(width.is_finite() && (0.0..=MAX_ADVANCE).contains(&width));
                }
            }
        }
    }
}
