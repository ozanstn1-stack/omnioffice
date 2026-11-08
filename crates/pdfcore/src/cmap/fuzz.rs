//! Fuzz entry points for readers that have no public API.
//!
//! `cargo fuzz` compiles the crate with `--cfg fuzzing`; these wrappers keep the
//! parsers `pub(crate)` and give the targets plain types.

use super::cid;

/// Reads `program` as an embedded `/Encoding` CMap and answers a few probes, so
/// the target exercises the lookup without unwrapping a private type.
#[doc(hidden)]
pub fn fuzz_cid(program: &[u8], identity_base: bool) -> Option<usize> {
    let base = identity_base.then_some(&b"Identity-H"[..]);
    cid::read(program, base).map(|map| map.probe())
}
