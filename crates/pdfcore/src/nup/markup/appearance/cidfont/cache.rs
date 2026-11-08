//! One document's composite fonts, read once.
//!
//! A page of notes may use the same `/DA` font hundreds of times, and every use
//! used to re-read that font's `/ToUnicode` (bounded at a megabyte), its
//! `/Encoding` CMap, its `/CIDToGIDMap` (128 KiB) and its `/W`. A hostile
//! document could make that expensive on purpose, and an ordinary one pays for it
//! on every note.
//!
//! The cache is created once per document, inside the imposition call, and lives
//! only as long as that call: an object id means something in one document and
//! nothing in another, so a cache that outlived the call would be a bug waiting
//! to happen. Nothing is evicted - a font that would take the total past the
//! budget is simply not stored, and the next note parses it again.

use std::collections::HashMap;
use std::sync::Arc;

use lopdf::{Dictionary, Document, ObjectId};

use super::CidFont;

/// The most the cached fonts of one document may hold.
pub(crate) const DEFAULT_BUDGET_BYTES: usize = 32 * 1024 * 1024;

/// What the cache did, for the tests that prove it works.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontCacheStats {
    /// Fonts answered from the cache, including the ones known to be unusable.
    pub(crate) hits: u32,
    /// Fonts that had to be read.
    pub(crate) misses: u32,
    /// Fonts read successfully, whatever happened to them afterwards.
    pub(crate) parses: u32,
    /// Fonts read but not stored because the budget was full.
    pub(crate) bypassed: u32,
}

/// A stored answer: either a usable font or the fact that it is not usable.
enum Entry {
    Ready(Arc<CidFont>),
    Unsupported,
}

/// The composite fonts of one document.
pub(crate) struct FontCache {
    entries: HashMap<ObjectId, Entry>,
    bytes: usize,
    budget: usize,
    stats: FontCacheStats,
}

impl FontCache {
    pub(crate) fn new() -> Self {
        Self::with_budget(DEFAULT_BUDGET_BYTES)
    }

    /// A cache with this many bytes of room; zero stores nothing, which is how a
    /// test compares the cached output with the uncached one.
    pub(crate) fn with_budget(budget: usize) -> Self {
        Self { entries: HashMap::new(), bytes: 0, budget, stats: FontCacheStats::default() }
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> FontCacheStats {
        self.stats
    }

    /// The font `id` names, reading it the first time.
    ///
    /// `None` means the font cannot be used for drawing - now and for every later
    /// note, which is why the refusal is stored too.
    pub(crate) fn get_or_load(&mut self, doc: &Document, id: ObjectId, dict: &Dictionary) -> Option<Arc<CidFont>> {
        match self.entries.get(&id) {
            Some(Entry::Ready(font)) => {
                self.stats.hits += 1;
                return Some(Arc::clone(font));
            }
            Some(Entry::Unsupported) => {
                self.stats.hits += 1;
                return None;
            }
            None => {}
        }
        self.stats.misses += 1;
        let Some(font) = CidFont::load(doc, dict) else {
            // A refusal is worth remembering: the next note would read the same
            // streams to learn the same thing.
            self.entries.insert(id, Entry::Unsupported);
            return None;
        };
        self.stats.parses += 1;
        let font = Arc::new(font);
        let size = font.heap_bytes();
        if self.bytes.saturating_add(size) > self.budget {
            self.stats.bypassed += 1;
            return Some(font);
        }
        self.bytes += size;
        self.entries.insert(id, Entry::Ready(Arc::clone(&font)));
        Some(font)
    }
}

#[cfg(test)]
mod tests;
