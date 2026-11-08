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

use super::super::drfont::DrFont;
use super::CidFont;

/// What the cache did, for the tests that prove it works.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontCacheStats {
    /// Fonts answered from the cache, including the ones known to be unusable.
    pub(crate) hits: u32,
    /// Fonts that had to be read.
    pub(crate) misses: u32,
    /// Composite fonts read successfully, whatever happened to them afterwards.
    pub(crate) parses: u32,
    /// Simple fonts read successfully.
    pub(crate) simple_parses: u32,
    /// Fonts read but not stored because the budget was full.
    pub(crate) bypassed: u32,
    /// Bytes the cache holds when the imposition returns.
    pub(crate) resident_bytes: usize,
    /// The most it held at once.
    pub(crate) peak_bytes: usize,
}

/// A stored answer: either a usable font or the fact that it is not usable.
enum Entry {
    Ready(Arc<CidFont>),
    Unsupported,
}

/// The composite fonts of one document.
pub(crate) struct FontCache {
    entries: HashMap<ObjectId, Entry>,
    /// Simple fonts, by object id; a refused font is stored as `None`.
    simple_fonts: HashMap<ObjectId, Option<Arc<DrFont>>>,
    bytes: usize,
    budget: usize,
    stats: FontCacheStats,
}

impl FontCache {
    /// A cache with this many bytes of room; zero stores nothing, which is how a
    /// test compares the cached output with the uncached one.
    /// A cache with the default budget, which is what an imposition uses.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new() -> Self {
        Self::with_budget(crate::nup::DEFAULT_FONT_CACHE_BYTES)
    }

    pub(crate) fn with_budget(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            simple_fonts: HashMap::new(),
            bytes: 0,
            budget,
            stats: FontCacheStats::default(),
        }
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
        self.stats.resident_bytes = self.bytes;
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.bytes);
        self.entries.insert(id, Entry::Ready(Arc::clone(&font)));
        Some(font)
    }
}

impl FontCache {
    /// The simple font `id` names, reading it the first time.
    ///
    /// The key is the font's object id, never its resource name: two pages may
    /// call different fonts `/F1`.
    pub(crate) fn simple(&mut self, doc: &Document, id: ObjectId, dict: &Dictionary) -> Option<Arc<DrFont>> {
        if let Some(entry) = self.simple_fonts.get(&id) {
            self.stats.hits += 1;
            return entry.clone();
        }
        self.stats.misses += 1;
        let font = super::super::drfont::parse_font(doc, id, dict);
        let Some(font) = font else {
            self.simple_fonts.insert(id, None);
            return None;
        };
        self.stats.simple_parses += 1;
        let font = Arc::new(font);
        let size = font.heap_bytes();
        if self.bytes.saturating_add(size) > self.budget {
            self.stats.bypassed += 1;
            return Some(font);
        }
        self.bytes += size;
        self.stats.resident_bytes = self.bytes;
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.bytes);
        self.simple_fonts.insert(id, Some(Arc::clone(&font)));
        Some(font)
    }
}

#[cfg(test)]
mod tests;
