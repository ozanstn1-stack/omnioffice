//! Local document version history ring (25 versions per document) with
//! atomic index updates and unchanged-save deduplication.

use super::timestamp;
use officecore::error::{OfficeError, OfficeResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub version: u32,
    pub saved_at: String,
    pub title: String,
    pub kind: String,
    #[cfg_attr(feature = "bindings", specta(type = specta_typescript::Number))]
    pub size: u64,
}

/// How many versions a document keeps.
pub(super) const HISTORY_LIMIT: usize = 25;

/// Writes a new version file and updates the index, keeping the newest
/// `limit` versions. If the newest stored version already has the identical
/// `(kind, title, model)` payload, returns the existing entry without creating
/// a duplicate snapshot or evicting older versions from the ring.
/// `write` is the atomic file writer (injected so a test can make the index
/// write fail).
pub(super) fn push_version(
    dir: &Path,
    kind: String,
    title: String,
    model: &Value,
    limit: usize,
    write: impl Fn(&Path, &[u8]) -> OfficeResult<()>,
) -> OfficeResult<HistoryEntry> {
    std::fs::create_dir_all(dir).map_err(|error| OfficeError::from_io(error, dir))?;
    let mut index = read_history_index(dir);
    if let Some(latest) = index.iter().max_by_key(|entry| entry.version) {
        let latest_path = dir.join(format!("v{}.json", latest.version));
        if let Ok(existing_bytes) = std::fs::read(&latest_path) {
            if let Ok(existing_json) = serde_json::from_slice::<Value>(&existing_bytes) {
                if existing_json.get("kind").and_then(Value::as_str) == Some(kind.as_str())
                    && existing_json.get("title").and_then(Value::as_str) == Some(title.as_str())
                    && existing_json.get("model") == Some(model)
                {
                    return Ok(latest.clone());
                }
            }
        }
    }
    let version = index.iter().map(|entry| entry.version).max().unwrap_or(0) + 1;
    let payload_json = serde_json::json!({
        "version": version,
        "savedAt": timestamp(),
        "kind": kind,
        "title": title,
        "model": model,
    });
    let bytes = serde_json::to_vec(&payload_json)
        .map_err(|error| OfficeError::internal(format!("Could not encode the version: {error}")))?;
    write(&dir.join(format!("v{version}.json")), &bytes)?;
    let entry = HistoryEntry { version, saved_at: timestamp(), title, kind, size: bytes.len() as u64 };
    index.push(entry.clone());
    // Keep the newest versions.
    index.sort_by_key(|entry| entry.version);
    let mut pruned = Vec::new();
    while index.len() > limit {
        pruned.push(index.remove(0).version);
    }
    let index_bytes = serde_json::to_vec(&index)
        .map_err(|error| OfficeError::internal(format!("Could not encode the history index: {error}")))?;
    // The new index goes first: a crash before it leaves the old index, whose
    // files all still exist; a crash after it leaves at most an orphaned file.
    write(&dir.join("index.json"), &index_bytes)?;
    for version in &pruned {
        let _ = std::fs::remove_file(dir.join(format!("v{version}.json")));
    }
    Ok(entry)
}

pub(super) fn read_history_index(dir: &Path) -> Vec<HistoryEntry> {
    let path = dir.join("index.json");
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<HistoryEntry>>(&bytes).ok())
        .unwrap_or_default()
}

pub(super) fn list_versions(dir: &Path) -> Vec<HistoryEntry> {
    let mut index = read_history_index(dir);
    index.sort_by_key(|entry| std::cmp::Reverse(entry.version));
    index
}

pub(super) fn load_version(dir: &Path, version: u32) -> OfficeResult<Value> {
    let path = dir.join(format!("v{version}.json"));
    let bytes = officecore::io::read_bytes(&path)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| OfficeError::corrupt(format!("The saved version is damaged: {error}")))?;
    Ok(value.get("model").cloned().unwrap_or(Value::Null))
}

pub(super) fn clear_versions(dir: &Path) -> OfficeResult<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|error| OfficeError::from_io(error, dir))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(dir: &Path, text: &str, limit: usize) -> OfficeResult<HistoryEntry> {
        push_version(dir, "writer".into(), "Doc".into(), &serde_json::json!({ "text": text }), limit, |path, bytes| {
            officecore::io::write_atomic(path, bytes)
        })
    }

    fn version_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn history_keeps_the_newest_versions_and_nothing_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        for round in 1..=5 {
            let entry = push(dir.path(), &format!("text {round}"), 3).unwrap();
            assert_eq!(entry.version, round);
        }
        let index = read_history_index(dir.path());
        assert_eq!(index.iter().map(|entry| entry.version).collect::<Vec<_>>(), vec![3, 4, 5]);
        // Pruned files are gone and no temp file is left behind.
        assert_eq!(version_files(dir.path()), vec!["index.json", "v3.json", "v4.json", "v5.json"]);
        let newest: Value = serde_json::from_slice(&std::fs::read(dir.path().join("v5.json")).unwrap()).unwrap();
        assert_eq!(newest["model"]["text"], "text 5");
    }

    #[test]
    fn history_never_deletes_a_version_before_the_new_index_is_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        for round in 1..=3 {
            push(dir.path(), &format!("text {round}"), 3).unwrap();
        }
        let before = std::fs::read(dir.path().join("index.json")).unwrap();
        // The fourth push would prune v1, but the index write fails.
        let failed =
            push_version(dir.path(), "writer".into(), "Doc".into(), &serde_json::json!({}), 3, |path, bytes| {
                if path.file_name().is_some_and(|name| name == "index.json") {
                    return Err(OfficeError::internal("disk full"));
                }
                officecore::io::write_atomic(path, bytes)
            });
        assert!(failed.is_err());
        // The old index is untouched and every row it lists still has its file.
        assert_eq!(std::fs::read(dir.path().join("index.json")).unwrap(), before);
        for entry in read_history_index(dir.path()) {
            assert!(dir.path().join(format!("v{}.json", entry.version)).exists(), "v{} is missing", entry.version);
        }
        // The next successful push reuses the number of the failed one and
        // prunes down to the limit.
        let retry = push(dir.path(), "text 4", 3).unwrap();
        assert_eq!(retry.version, 4);
        let versions: Vec<u32> = read_history_index(dir.path()).iter().map(|entry| entry.version).collect();
        assert_eq!(versions, vec![2, 3, 4]);
        assert!(!dir.path().join("v1.json").exists());
    }

    #[test]
    fn unchanged_save_reuses_latest_version_without_evicting_history() {
        let dir = tempfile::tempdir().expect("tempdir");
        for round in 1..=3 {
            let entry = push(dir.path(), &format!("text {round}"), 3).unwrap();
            assert_eq!(entry.version, round);
        }
        // Repeated saves with identical model, title, and kind must not
        // allocate v4..v8 or evict v1 and v2 from the ring.
        for _ in 0..5 {
            let repeated = push(dir.path(), "text 3", 3).unwrap();
            assert_eq!(repeated.version, 3);
        }
        let versions: Vec<u32> = read_history_index(dir.path()).iter().map(|entry| entry.version).collect();
        assert_eq!(versions, vec![1, 2, 3]);
        assert_eq!(version_files(dir.path()), vec!["index.json", "v1.json", "v2.json", "v3.json"]);

        // Saving a changed title or model allocates v4 and prunes v1.
        let retitled = push_version(
            dir.path(),
            "writer".into(),
            "Renamed Doc".into(),
            &serde_json::json!({ "text": "text 3" }),
            3,
            officecore::io::write_atomic,
        )
        .unwrap();
        assert_eq!(retitled.version, 4);
        let versions_after: Vec<u32> = read_history_index(dir.path()).iter().map(|entry| entry.version).collect();
        assert_eq!(versions_after, vec![2, 3, 4]);
        assert_eq!(load_version(dir.path(), 4).unwrap(), serde_json::json!({ "text": "text 3" }));
    }
}
