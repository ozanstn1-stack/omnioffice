//! Persistent hybrid search index for the document vault.
//!
//! Three pieces, kept apart so each is testable on its own:
//!
//! * [`Bm25Index`] — a real BM25 ranking (inverted index, IDF, length
//!   normalisation) over text chunks.
//! * [`VectorStore`] — per-chunk embeddings persisted as `embeddings.bin`
//!   (raw little-endian `f32`) plus the metadata in `index.json`. A vector is
//!   keyed by `model + sha256(chunk text)`, so an unchanged chunk is never
//!   re-embedded when the model is the same.
//! * [`reciprocal_rank_fusion`] — combines the two ranked lists.
//!
//! Everything is local: no network, no telemetry. The store lives next to the
//! vault config and can be deleted to rebuild it.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// BM25 term-frequency saturation.
const K1: f64 = 1.2;
/// BM25 length-normalisation strength.
const B: f64 = 0.75;
/// Reciprocal Rank Fusion constant (the value the original paper recommends).
const RRF_K: f64 = 60.0;
/// Characters per chunk; a chunk is embedded as a unit.
pub const CHUNK_CHARS: usize = 800;

/// One indexed text chunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub document_id: String,
    pub chunk: u32,
    pub text: String,
}

/// A persisted vector entry: the cache key plus the chunk it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VectorKey {
    pub key: String,
    pub document_id: String,
    pub chunk: u32,
}

/// Splits `text` into overlapping-free fixed windows on character boundaries.
pub fn chunk_text(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    chars.chunks(CHUNK_CHARS).map(|window| window.iter().collect()).collect()
}

/// Lowercase alphanumeric tokens (Unicode letters and digits).
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

/// The cache key of a chunk: the model plus a hash of the chunk text, so a
/// changed chunk gets a new key and an unchanged one is reused.
pub fn vector_key(model: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
    format!("{model}:{hex}")
}

// ---------------------------------------------------------------------------
// BM25
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Bm25Index {
    /// term -> (chunk index, term frequency)
    postings: HashMap<String, Vec<(u32, u32)>>,
    /// Chunk lengths in tokens.
    lengths: Vec<u32>,
    /// Number of chunks.
    count: usize,
    /// Average chunk length.
    average: f64,
}

impl Bm25Index {
    pub fn build(chunks: &[Chunk]) -> Self {
        let mut postings: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        let mut lengths = Vec::with_capacity(chunks.len());
        for (index, chunk) in chunks.iter().enumerate() {
            let tokens = tokenize(&chunk.text);
            lengths.push(tokens.len() as u32);
            let mut counts: HashMap<String, u32> = HashMap::new();
            for token in tokens {
                *counts.entry(token).or_insert(0) += 1;
            }
            for (term, frequency) in counts {
                postings.entry(term).or_default().push((index as u32, frequency));
            }
        }
        let total: u32 = lengths.iter().sum();
        let average = if lengths.is_empty() { 0.0 } else { total as f64 / lengths.len() as f64 };
        Self { postings, lengths, count: chunks.len(), average }
    }

    fn idf(&self, term: &str) -> f64 {
        let df = self.postings.get(term).map(|list| list.len()).unwrap_or(0) as f64;
        // BM25+ style IDF, always positive so a term present in every chunk
        // does not push the score negative.
        ((self.count as f64 - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// BM25 score per chunk, sorted by descending score.
    pub fn search(&self, query: &str, limit: usize) -> Vec<(u32, f64)> {
        if self.count == 0 {
            return Vec::new();
        }
        let terms = tokenize(query);
        if terms.is_empty() {
            return Vec::new();
        }
        let mut scores: HashMap<u32, f64> = HashMap::new();
        for term in &terms {
            let Some(postings) = self.postings.get(term) else { continue };
            let idf = self.idf(term);
            for (chunk, frequency) in postings {
                let length = self.lengths.get(*chunk as usize).copied().unwrap_or(0) as f64;
                let norm = if self.average > 0.0 { 1.0 - B + B * length / self.average } else { 1.0 };
                let tf = *frequency as f64;
                let score = idf * (tf * (K1 + 1.0)) / (tf + K1 * norm);
                *scores.entry(*chunk).or_insert(0.0) += score;
            }
        }
        let mut ranked: Vec<(u32, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        ranked.truncate(limit);
        ranked
    }
}

// ---------------------------------------------------------------------------
// Vectors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct VectorStore {
    /// Model the vectors belong to; a different model invalidates the cache.
    pub model: String,
    /// Vector dimension.
    pub dim: usize,
    pub keys: Vec<VectorKey>,
    /// Flattened `keys.len() * dim` vectors.
    pub vectors: Vec<f32>,
}

impl VectorStore {
    pub fn new(model: &str, dim: usize) -> Self {
        Self { model: model.to_string(), dim, keys: Vec::new(), vectors: Vec::new() }
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.keys.iter().position(|entry| entry.key == key)
    }

    /// True when the exact model + chunk hash is already stored.
    pub fn contains(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// Inserts or replaces a vector for `key`.
    pub fn set(&mut self, key: String, document_id: &str, chunk: u32, vector: &[f32]) {
        if vector.len() != self.dim {
            return;
        }
        if let Some(position) = self.position(&key) {
            let start = position * self.dim;
            if let Some(slice) = self.vectors.get_mut(start..start + self.dim) {
                slice.copy_from_slice(vector);
                return;
            }
            // The stored vector is missing (short/corrupt file): append it so
            // the key gets a vector again instead of leaving a hole.
        }
        self.keys.push(VectorKey { key, document_id: document_id.to_string(), chunk });
        self.vectors.extend_from_slice(vector);
    }

    /// Drops entries that no longer belong to any current key.
    pub fn retain(&mut self, keep: &std::collections::HashSet<String>) {
        let mut keys = Vec::new();
        let mut vectors = Vec::new();
        for (position, entry) in self.keys.iter().enumerate() {
            if !keep.contains(&entry.key) {
                continue;
            }
            let start = position * self.dim;
            let Some(slice) = self.vectors.get(start..start + self.dim) else {
                // No vector for this key: drop it so it is re-embedded.
                continue;
            };
            keys.push(entry.clone());
            vectors.extend_from_slice(slice);
        }
        self.keys = keys;
        self.vectors = vectors;
    }

    /// Cosine similarity search over the stored vectors.
    pub fn search(&self, query: &[f32], limit: usize) -> Vec<(usize, f64)> {
        if self.dim == 0 || query.len() != self.dim || self.keys.is_empty() {
            return Vec::new();
        }
        let query_norm = (query.iter().map(|value| (*value as f64) * (*value as f64)).sum::<f64>()).sqrt();
        if query_norm == 0.0 {
            return Vec::new();
        }
        let mut scored: Vec<(usize, f64)> = Vec::with_capacity(self.keys.len());
        for position in 0..self.keys.len() {
            let start = position * self.dim;
            let Some(vector) = self.vectors.get(start..start + self.dim) else {
                // A missing vector (short file) is skipped, never a panic.
                continue;
            };
            let mut dot = 0.0f64;
            let mut norm = 0.0f64;
            for (a, b) in query.iter().zip(vector) {
                dot += (*a as f64) * (*b as f64);
                norm += (*b as f64) * (*b as f64);
            }
            let denom = query_norm * norm.sqrt();
            if denom > 0.0 {
                scored.push((position, dot / denom));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        scored.truncate(limit);
        scored
    }
}

// ---------------------------------------------------------------------------
// Fusion
// ---------------------------------------------------------------------------

/// Reciprocal Rank Fusion of ranked chunk lists (each `(chunk index, score)`).
pub fn reciprocal_rank_fusion(lists: &[Vec<(u32, f64)>], limit: usize) -> Vec<(u32, f64)> {
    let mut fused: HashMap<u32, f64> = HashMap::new();
    for list in lists {
        for (rank, (chunk, _)) in list.iter().enumerate() {
            *fused.entry(*chunk).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }
    let mut ranked: Vec<(u32, f64)> = fused.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    ranked.truncate(limit);
    ranked
}

// ---------------------------------------------------------------------------
// The persistent index
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct IndexMeta {
    version: u32,
    model: String,
    dim: usize,
    /// Hash of the indexed document set + model; a change rebuilds the index.
    fingerprint: String,
    chunks: Vec<Chunk>,
    keys: Vec<VectorKey>,
}

#[derive(Debug, Clone, Default)]
pub struct VaultSearchIndex {
    pub chunks: Vec<Chunk>,
    pub bm25: Bm25Index,
    pub vectors: VectorStore,
    /// Hash of the document set the index was built from (empty when unknown).
    pub fingerprint: String,
}

impl VaultSearchIndex {
    /// Builds the BM25 side and an empty vector store for `model`.
    pub fn build(chunks: Vec<Chunk>, model: &str, dim: usize) -> Self {
        let bm25 = Bm25Index::build(&chunks);
        Self { chunks, bm25, vectors: VectorStore::new(model, dim), fingerprint: String::new() }
    }

    /// The chunks that still need an embedding for `model` (cache miss), as
    /// `(key, chunk index, text)`.
    pub fn pending(&self, model: &str) -> Vec<(String, usize, String)> {
        self.chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| {
                let key = vector_key(model, &chunk.text);
                if self.vectors.model == model && self.vectors.contains(&key) {
                    None
                } else {
                    Some((key, index, chunk.text.clone()))
                }
            })
            .collect()
    }

    /// Stores an embedding for a chunk (its key must come from [`vector_key`]).
    pub fn set_embedding(&mut self, key: &str, chunk_index: usize, vector: &[f32]) {
        let Some(chunk) = self.chunks.get(chunk_index) else { return };
        self.vectors.set(key.to_string(), &chunk.document_id, chunk.chunk, vector);
    }

    /// Drops vectors that no longer match a current chunk key.
    pub fn prune_vectors(&mut self, model: &str) {
        if self.vectors.model != model {
            self.vectors = VectorStore::new(model, self.vectors.dim);
            return;
        }
        let keep: std::collections::HashSet<String> =
            self.chunks.iter().map(|chunk| vector_key(model, &chunk.text)).collect();
        self.vectors.retain(&keep);
    }

    /// Hybrid search: BM25 and, when a query vector is given, cosine ranking,
    /// fused with RRF. Returns chunk indices with their fused score.
    pub fn hybrid_search(&self, query: &str, query_vector: Option<&[f32]>, limit: usize) -> Vec<(u32, f64)> {
        let keyword = self.bm25.search(query, limit.max(20));
        let semantic: Vec<(u32, f64)> = match query_vector {
            Some(vector) if !self.vectors.keys.is_empty() => self
                .vectors
                .search(vector, limit.max(20))
                .into_iter()
                .filter_map(|(position, score)| self.vectors.keys.get(position).map(|key| (position, key, score)))
                .filter_map(|(_, key, score)| {
                    self.chunks
                        .iter()
                        .position(|chunk| chunk.document_id == key.document_id && chunk.chunk == key.chunk)
                        .map(|position| (position as u32, score))
                })
                .collect(),
            _ => Vec::new(),
        };
        if semantic.is_empty() {
            keyword.into_iter().take(limit).collect()
        } else {
            reciprocal_rank_fusion(&[keyword, semantic], limit)
        }
    }

    pub fn save(&self, directory: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let meta = IndexMeta {
            version: 1,
            model: self.vectors.model.clone(),
            dim: self.vectors.dim,
            fingerprint: self.fingerprint.clone(),
            chunks: self.chunks.clone(),
            keys: self.vectors.keys.clone(),
        };
        let json = serde_json::to_vec(&meta).map_err(|error| std::io::Error::other(error.to_string()))?;
        std::fs::write(directory.join("index.json"), json)?;
        let mut bytes = Vec::with_capacity(self.vectors.vectors.len() * 4);
        for value in &self.vectors.vectors {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(directory.join("embeddings.bin"), bytes)?;
        Ok(())
    }

    pub fn load(directory: &Path) -> Option<Self> {
        let json = std::fs::read(directory.join("index.json")).ok()?;
        let meta: IndexMeta = serde_json::from_slice(&json).ok()?;
        let bm25 = Bm25Index::build(&meta.chunks);
        let mut vectors = VectorStore::new(&meta.model, meta.dim);
        vectors.keys = meta.keys;
        if meta.dim > 0 {
            let bytes = std::fs::read(directory.join("embeddings.bin")).unwrap_or_default();
            vectors.vectors = (0..bytes.len() / 4)
                .map(|index| {
                    let slice = &bytes[index * 4..index * 4 + 4];
                    f32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
                })
                .collect();
            // A truncated file would misalign the keys; drop the tail.
            let expected = vectors.keys.len() * meta.dim;
            vectors.vectors.truncate(expected);
            if vectors.vectors.len() != expected {
                // The file is shorter than the keys need: the index is corrupt,
                // and a partial store would panic on the first key-indexed
                // slice. Rebuild instead of loading it.
                return None;
            }
        }
        Some(Self { chunks: meta.chunks, bm25, vectors, fingerprint: meta.fingerprint })
    }

    pub fn directory(root: &Path) -> PathBuf {
        root.join("vault_index")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(texts: &[&str]) -> Vec<Chunk> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| Chunk { document_id: format!("doc{index}"), chunk: 0, text: (*text).to_string() })
            .collect()
    }

    #[test]
    fn bm25_ranks_a_relevant_chunk_first() {
        let index = Bm25Index::build(&chunks(&[
            "kedi köpek kuş",
            "fatura vergi kdv hesaplama",
            "vergi beyannamesi vergi dairesi",
        ]));
        let ranked = index.search("vergi beyannamesi", 3);
        assert!(!ranked.is_empty());
        // The chunk with both terms and the shorter length wins.
        assert_eq!(ranked[0].0, 2, "ranked: {ranked:?}");
        // A term present in every chunk must not produce a negative score.
        let common = index.search("vergi", 3);
        assert!(common.iter().all(|(_, score)| *score >= 0.0), "{common:?}");
    }

    #[test]
    fn bm25_prefers_the_shorter_document_for_the_same_term() {
        let index = Bm25Index::build(&chunks(&[
            "vergi",
            "vergi ve daha birçok alakasız kelime burada uzun bir metin oluşturuyor",
        ]));
        let ranked = index.search("vergi", 2);
        assert_eq!(ranked[0].0, 0, "the short document is the better match: {ranked:?}");
    }

    #[test]
    fn rrf_combines_two_rankings() {
        // A appears in both lists, B only in the first, C only in the second.
        let keyword = vec![(0u32, 9.0), (1, 5.0)];
        let semantic = vec![(0u32, 0.9), (2, 0.8)];
        let fused = reciprocal_rank_fusion(&[keyword, semantic], 3);
        assert_eq!(fused[0].0, 0, "the item in both lists wins: {fused:?}");
        assert_eq!(fused.len(), 3);
    }

    #[test]
    fn vector_search_uses_cosine_similarity() {
        let mut store = VectorStore::new("test", 3);
        store.set("a".into(), "doc", 0, &[1.0, 0.0, 0.0]);
        store.set("b".into(), "doc", 1, &[0.0, 1.0, 0.0]);
        let ranked = store.search(&[0.9, 0.1, 0.0], 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, 0, "the closer vector wins: {ranked:?}");
    }

    #[test]
    fn a_changed_chunk_gets_a_new_key_and_the_same_chunk_is_reused() {
        let first = vector_key("model-x", "aynı içerik");
        let second = vector_key("model-x", "aynı içerik");
        let other_model = vector_key("model-y", "aynı içerik");
        let changed = vector_key("model-x", "değişmiş içerik");
        assert_eq!(first, second, "the same model + content must reuse the cache");
        assert_ne!(first, other_model, "a different model must not reuse it");
        assert_ne!(first, changed, "changed content must not reuse it");
    }

    #[test]
    fn persists_and_reloads_without_recomputing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault_index");
        let mut index = VaultSearchIndex::build(chunks(&["vergi beyannamesi", "kedi köpek"]), "test-model", 2);
        index.set_embedding(&vector_key("test-model", "vergi beyannamesi"), 0, &[1.0, 0.0]);
        index.set_embedding(&vector_key("test-model", "kedi köpek"), 1, &[0.0, 1.0]);
        index.save(&path).unwrap();

        let loaded = VaultSearchIndex::load(&path).expect("load");
        assert_eq!(loaded.chunks.len(), 2);
        assert_eq!(loaded.vectors.keys.len(), 2);
        // Nothing is pending: the cache is warm.
        assert!(loaded.pending("test-model").is_empty(), "cache must be warm");
        // A different model invalidates every vector.
        assert_eq!(loaded.pending("other-model").len(), 2);
        // BM25 still ranks after reload.
        let ranked = loaded.hybrid_search("vergi", None, 2);
        assert_eq!(ranked[0].0, 0);
    }

    #[test]
    fn hybrid_search_uses_both_lists() {
        let mut index = VaultSearchIndex::build(chunks(&["vergi beyannamesi", "kedi köpek kuş"]), "test", 2);
        index.set_embedding(&vector_key("test", "vergi beyannamesi"), 0, &[1.0, 0.0]);
        index.set_embedding(&vector_key("test", "kedi köpek kuş"), 1, &[0.0, 1.0]);
        // A keyword match ranks chunk 0; the vector also favours chunk 0.
        let fused = index.hybrid_search("vergi", Some(&[1.0, 0.0]), 2);
        assert_eq!(fused[0].0, 0, "{fused:?}");
    }

    #[test]
    fn chunking_uses_character_boundaries() {
        let text = "a".repeat(CHUNK_CHARS + 10);
        let parts = chunk_text(&text);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].chars().count(), CHUNK_CHARS);
        // A multi-byte character is never split.
        let turkish = "ş".repeat(CHUNK_CHARS + 1);
        let parts = chunk_text(&turkish);
        assert!(parts.iter().all(|part| part.chars().count() <= CHUNK_CHARS));
    }

    #[test]
    fn scale_5000_chunks_query_under_300ms() {
        // 5.000 chunks of ~120 words each, a realistic vault size.
        let texts: Vec<String> = (0..5_000)
            .map(|index| {
                let mut text = String::new();
                for word in 0..120 {
                    text.push_str(&format!("kelime{} belge{} ortak ", word % 50, index % 200));
                }
                text
            })
            .collect();
        let chunk_list: Vec<Chunk> = texts
            .iter()
            .enumerate()
            .map(|(index, text)| Chunk { document_id: format!("doc{index}"), chunk: 0, text: text.clone() })
            .collect();
        let index = VaultSearchIndex::build(chunk_list, "test", 0);
        let started = std::time::Instant::now();
        let ranked = index.hybrid_search("belge42 ortak", None, 20);
        let elapsed = started.elapsed();
        assert!(!ranked.is_empty());
        assert!(elapsed.as_millis() < 300, "5000-chunk query took {} ms", elapsed.as_millis());
    }

    #[test]
    fn a_truncated_embeddings_file_is_rejected_not_panicked_on() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault_index");
        let mut index = VaultSearchIndex::build(chunks(&["bir", "iki"]), "m", 2);
        index.set_embedding(&vector_key("m", "bir"), 0, &[1.0, 0.0]);
        index.set_embedding(&vector_key("m", "iki"), 1, &[0.0, 1.0]);
        index.save(&path).unwrap();
        // Drop one vector's worth of bytes: keys * dim no longer fits.
        std::fs::write(path.join("embeddings.bin"), [0u8; 4]).unwrap();
        assert!(VaultSearchIndex::load(&path).is_none(), "a short file must not load");
    }

    #[test]
    fn vector_search_skips_missing_vectors_instead_of_panicking() {
        let mut store = VectorStore::new("m", 4);
        store.keys = vec![
            VectorKey { key: "a".into(), document_id: "d".into(), chunk: 0 },
            VectorKey { key: "b".into(), document_id: "d".into(), chunk: 1 },
        ];
        store.vectors = vec![1.0, 0.0, 0.0, 0.0];
        let scored = store.search(&[1.0, 0.0, 0.0, 0.0], 5);
        assert_eq!(scored.len(), 1, "only the key with a vector is scored");
    }
}
