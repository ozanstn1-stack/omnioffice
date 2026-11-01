//! Plugin installation: the plan a human confirms, the SHA-256 recorded with it
//! and the integrity state `plugin_list` reports (E1/5).
//!
//! Until 7.2 `plugin_install` wrote whatever the webview sent: a compromised
//! renderer could install a plugin (and its permissions) without the user ever
//! seeing it. Two things changed:
//!
//! * **A human confirms the plan.** [`plan_install`] validates the pair and
//!   summarises what would be written - name, id, version, permissions,
//!   capabilities, size and hash - and [`install_confirmed`] writes nothing
//!   unless the confirmation accepts it. The command layer opens a native
//!   dialog with that summary; the sample and the user-picked folder are the
//!   only paths that skip it, because the bundled text and the folder choice are
//!   already the human's own action (each call site says which).
//! * **The install is recorded.** `install.json` next to the plugin's files
//!   holds the id, the bundle SHA-256, the time and the confirmation flag, so
//!   `plugin_list` can tell "these bytes are what the user approved" from "this
//!   plugin changed on disk after the fact".
//!
//! The hash covers the manifest and the source exactly as they are written,
//! under a domain separator, so it can never collide with a hash of some other
//! file. A signed manifest is deliberately not attempted here.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    plugin_dir, read_text, validate_plugin_id, write_text, PluginEntry, PluginErrorPayload, PluginResult, Value,
    MAX_MANIFEST_BYTES, MAX_SOURCE_BYTES,
};

/// Domain separator for the bundle hash.
const HASH_DOMAIN: &str = "omnioffice-plugin-v1";
/// The record written next to a plugin's `manifest.json` and `main.js`.
const RECORD_FILE: &str = "install.json";

/// What a plugin would add to the app, in the words the confirmation shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub id: String,
    pub name: String,
    pub version: String,
    pub permissions: Vec<String>,
    pub capabilities: Vec<String>,
    pub source_bytes: u64,
    pub sha256: String,
}

impl InstallPlan {
    /// The text the confirmation dialog shows. Pinned by a test: what the user
    /// reads before a plugin is installed is part of the decision.
    pub fn summary(&self) -> String {
        let list = |items: &[String]| {
            if items.is_empty() {
                "none".to_string()
            } else {
                items.join(", ")
            }
        };
        format!(
            "Install the plugin \"{}\" ({}) version {}?\n\n\
             It runs in a sandbox with no IPC and no DOM, but it asks for these permissions: {}.\n\
             Capabilities: {}.\n\
             Source: {} KB\nSHA-256: {}\n\n\
             Only install plugins you trust. The hash above is recorded so a later change to the \
             installed files is visible in the plugin list.",
            self.name,
            self.id,
            self.version,
            list(&self.permissions),
            list(&self.capabilities),
            self.source_bytes.div_ceil(1024),
            self.sha256,
        )
    }
}

/// The record written at install time; also what `plugin_list` compares against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallRecord {
    id: String,
    sha256: String,
    installed_at: String,
    confirmed: bool,
}

/// The SHA-256 of a plugin bundle: the manifest and the source exactly as they
/// are written to disk, under a domain separator.
pub fn bundle_sha256(manifest_json: &str, source: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(HASH_DOMAIN.as_bytes());
    hasher.update([0u8]);
    hasher.update(manifest_json.as_bytes());
    hasher.update([0u8]);
    hasher.update(source.as_bytes());
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Validates a manifest/source pair and describes what installing it would do.
/// Nothing is written: the caller decides whether a human confirmed it.
pub fn plan_install(manifest_json: &str, source: &str) -> PluginResult<InstallPlan> {
    if manifest_json.len() > MAX_MANIFEST_BYTES {
        return Err(PluginErrorPayload::too_large(format!(
            "The manifest is larger than {} KB.",
            MAX_MANIFEST_BYTES / 1024
        )));
    }
    if source.trim().is_empty() {
        return Err(PluginErrorPayload::invalid("main.js is empty."));
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(PluginErrorPayload::too_large(format!("main.js is larger than {} KB.", MAX_SOURCE_BYTES / 1024)));
    }
    let manifest: Value = serde_json::from_str(manifest_json)
        .map_err(|_| PluginErrorPayload::invalid("The manifest is not valid JSON."))?;
    let id = manifest
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginErrorPayload::invalid("The manifest has no id."))?;
    if !validate_plugin_id(id) {
        return Err(PluginErrorPayload::invalid("Invalid plugin id."));
    }
    let strings = |key: &str| {
        manifest
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    Ok(InstallPlan {
        id: id.to_string(),
        name: manifest
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(id)
            .to_string(),
        version: manifest.get("version").and_then(Value::as_str).unwrap_or("unknown").to_string(),
        permissions: strings("permissions"),
        capabilities: strings("capabilities"),
        source_bytes: source.len() as u64,
        sha256: bundle_sha256(manifest_json, source),
    })
}

/// Installs a plugin after `confirm` accepted its plan. This is the only path
/// that writes a plugin; a declined plan returns `forbidden` and leaves the disk
/// untouched.
pub fn install_confirmed(
    root: &Path,
    manifest_json: &str,
    source: &str,
    confirm: impl FnOnce(&InstallPlan) -> bool,
) -> PluginResult<PluginEntry> {
    let plan = plan_install(manifest_json, source)?;
    if !confirm(&plan) {
        return Err(PluginErrorPayload::forbidden("The plugin installation was not confirmed."));
    }
    let dir = root.join(&plan.id);
    // The plugin directory must not be a symlink: otherwise manifest.json and
    // main.js would be written through a link to an arbitrary location.
    if let Ok(meta) = std::fs::symlink_metadata(&dir) {
        if meta.file_type().is_symlink() {
            return Err(PluginErrorPayload::forbidden("Refusing to install into a symlinked plugin directory."));
        }
    }
    std::fs::create_dir_all(&dir)
        .map_err(|_| PluginErrorPayload::internal("Could not create the plugin directory."))?;
    // Re-check after creation: a directory created concurrently (or a link)
    // must not redirect the writes outside the plugins root.
    if let Ok(meta) = std::fs::symlink_metadata(&dir) {
        if meta.file_type().is_symlink() {
            return Err(PluginErrorPayload::forbidden("Refusing to install into a symlinked plugin directory."));
        }
    }
    write_text(&dir.join("manifest.json"), manifest_json, "manifest.json")?;
    write_text(&dir.join("main.js"), source, "main.js")?;
    write_record(&dir, &plan)?;
    let manifest = serde_json::from_str(manifest_json)
        .map_err(|_| PluginErrorPayload::invalid("manifest.json is not valid JSON."))?;
    Ok(PluginEntry {
        manifest,
        source_bytes: plan.source_bytes,
        sha256: plan.sha256,
        integrity: "verified".to_string(),
    })
}

/// Writes the install record. Atomic, so an interrupted install leaves either
/// the old record or the new one, never a truncated file.
fn write_record(dir: &Path, plan: &InstallPlan) -> PluginResult<()> {
    let record = InstallRecord {
        id: plan.id.clone(),
        sha256: plan.sha256.clone(),
        installed_at: synccore::metadata::now_rfc3339(),
        confirmed: true,
    };
    let json = serde_json::to_string_pretty(&record)
        .map_err(|_| PluginErrorPayload::internal("Could not encode the install record."))?;
    write_text(&dir.join(RECORD_FILE), &json, RECORD_FILE)
}

fn read_record(dir: &Path) -> Option<InstallRecord> {
    let text = std::fs::read_to_string(dir.join(RECORD_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Reads a plugin folder the *user* selected. The folder and its two manifest
/// files must be real files, not symlinks, so a plugin cannot smuggle host
/// files into its own directory. Desktop only: Android has no folder install.
/// The folder picker is the human's consent, so no second dialog is opened.
#[cfg(any(desktop, test))]
pub fn install_from_dir(root: &Path, source_dir: &Path) -> PluginResult<PluginEntry> {
    let link_meta = std::fs::symlink_metadata(source_dir)
        .map_err(|_| PluginErrorPayload::invalid("Choose a folder that contains manifest.json and main.js."))?;
    if link_meta.file_type().is_symlink() {
        return Err(PluginErrorPayload::forbidden("Refusing to install a plugin from a symlinked folder."));
    }
    if !link_meta.is_dir() {
        return Err(PluginErrorPayload::invalid("Choose a folder that contains manifest.json and main.js."));
    }
    let manifest_path = source_dir.join("manifest.json");
    let source_path = source_dir.join("main.js");
    for (path, label) in [(&manifest_path, "manifest.json"), (&source_path, "main.js")] {
        let meta = std::fs::symlink_metadata(path)
            .map_err(|_| PluginErrorPayload::invalid(format!("{label} was not found.")))?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(PluginErrorPayload::forbidden(format!(
                "Refusing to install a plugin whose {label} is a symlink."
            )));
        }
    }
    let manifest_json = read_text(&manifest_path, MAX_MANIFEST_BYTES, "manifest.json")?;
    let source = read_text(&source_path, MAX_SOURCE_BYTES, "main.js")?;
    install_confirmed(root, &manifest_json, &source, |_| true)
}

/// Every installed plugin, with the integrity state of its files.
pub fn list_plugins(root: &Path) -> Vec<PluginEntry> {
    let mut entries = Vec::new();
    let Ok(dir) = std::fs::read_dir(root) else {
        return entries;
    };
    for item in dir.flatten() {
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !validate_plugin_id(name) {
            continue;
        }
        let Ok(manifest_json) = read_text(&path.join("manifest.json"), MAX_MANIFEST_BYTES, "manifest.json") else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<Value>(&manifest_json) else {
            continue;
        };
        // The directory name and the manifest id must agree, otherwise two
        // plugins could claim the same identity.
        if manifest.get("id").and_then(Value::as_str) != Some(name) {
            continue;
        }
        let Ok(source) = read_text(&path.join("main.js"), MAX_SOURCE_BYTES, "main.js") else {
            continue;
        };
        let source_bytes = source.len() as u64;
        // The recorded hash is what the user approved; the files on disk are
        // what will run. A mismatch is reported instead of being hidden.
        let current = bundle_sha256(&manifest_json, &source);
        let (sha256, integrity) = match read_record(&path) {
            Some(record) if record.sha256 == current => (record.sha256, "verified"),
            Some(record) => (record.sha256, "modified"),
            None => (current, "unrecorded"),
        };
        entries.push(PluginEntry { manifest, source_bytes, sha256, integrity: integrity.to_string() });
    }
    entries.sort_by(|a, b| {
        let left = a.manifest.get("id").and_then(Value::as_str).unwrap_or_default();
        let right = b.manifest.get("id").and_then(Value::as_str).unwrap_or_default();
        left.cmp(right)
    });
    entries
}

pub fn delete_plugin(root: &Path, id: &str) -> PluginResult<()> {
    let dir = plugin_dir(root, id)?;
    if !dir.is_dir() {
        return Err(PluginErrorPayload::not_found("The plugin is not installed."));
    }
    std::fs::remove_dir_all(&dir).map_err(|_| PluginErrorPayload::internal("Could not remove the plugin directory."))
}

/// Installs without asking. Tests only: production callers go through
/// [`install_confirmed`] (or pass the bundled sample / the user's folder pick).
#[cfg(test)]
pub fn install_into(root: &Path, manifest_json: &str, source: &str) -> PluginResult<PluginEntry> {
    install_confirmed(root, manifest_json, source, |_| true)
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
