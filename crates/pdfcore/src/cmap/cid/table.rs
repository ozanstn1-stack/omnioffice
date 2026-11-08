//! The runs of one code width, and what a code of that width selects.
//!
//! A map keeps two of these: one for one-byte codes and one for two-byte codes.
//! Each is sorted and normalised once, so a lookup is a partition point rather
//! than a scan, and two entries that disagree are refused instead of one of them
//! winning by accident.

use super::super::Code;

/// One run of consecutive codes that select consecutive CIDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Run {
    pub(super) low: u16,
    pub(super) high: u16,
    /// The CID `low` selects; the rest follow it.
    pub(super) cid: u16,
}

/// The runs and the notdef range of one width.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Table {
    pub(super) runs: Vec<Run>,
    /// Codes the CMap sends to `.notdef`, which must never be drawn.
    pub(super) notdef: Vec<(u16, u16)>,
}

/// A code of the given width; one-byte values are truncated to their byte.
pub(super) fn code_of(width: u8, value: u16) -> Code {
    if width == 1 {
        Code::one(value as u8)
    } else {
        Code::two(value)
    }
}

impl Table {
    /// Sorts the runs and merges the ones that agree, refusing a conflict.
    pub(super) fn normalise(&mut self) -> Option<()> {
        self.runs.sort_unstable_by_key(|run| (run.low, run.high));
        let mut runs: Vec<Run> = Vec::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            match runs.last_mut() {
                // The same mapping written twice is fine, and so is a run that
                // continues the previous one with the same offset.
                Some(last) if last.high >= run.low => {
                    let last_offset = i64::from(last.cid) - i64::from(last.low);
                    let run_offset = i64::from(run.cid) - i64::from(run.low);
                    if last_offset != run_offset {
                        return None;
                    }
                    last.high = last.high.max(run.high);
                }
                _ => runs.push(run),
            }
        }
        self.runs = runs;
        self.notdef.sort_unstable();
        if self.notdef.windows(2).any(|pair| pair[0].1 >= pair[1].0) {
            return None;
        }
        // A notdef range that covers a mapped code would make the two
        // disagree, and viewers differ on which one wins.
        for (low, high) in &self.notdef {
            let index = self.runs.partition_point(|run| run.high < *low);
            if self.runs.get(index).is_some_and(|run| run.low <= *high) {
                return None;
            }
        }
        Some(())
    }
}
