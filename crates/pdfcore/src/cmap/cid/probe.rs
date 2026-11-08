//! A read-only sweep of a map, for the fuzz target and the unit tests.
//!
//! A map that built successfully must answer every boundary code without
//! panicking; the fuzz target cannot see the private fields, so the sweep lives
//! here and returns how many boundaries resolved.

use super::{code_of, CidMap};

impl CidMap {
    /// Visits every run boundary; used by the fuzz target to prove that a map
    /// which built successfully never panics while being read.
    #[cfg(any(test, fuzzing))]
    pub(crate) fn probe(&self) -> usize {
        let mut seen = 0usize;
        for (table, width) in [(&self.wide, 2u8), (&self.narrow, 1)] {
            for run in &table.runs {
                for value in [run.low.saturating_sub(1), run.low, run.high, run.high.saturating_add(1)] {
                    if self.cid(code_of(width, value)).is_some() {
                        seen += 1;
                    }
                }
            }
            for (low, high) in &table.notdef {
                let _ = self.cid(code_of(width, *low));
                let _ = self.cid(code_of(width, *high));
            }
        }
        seen
    }
}
