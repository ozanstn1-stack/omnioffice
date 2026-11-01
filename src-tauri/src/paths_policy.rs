//! Raw-path policy for commands that accept capability tokens.
//!
//! Desktop release builds require a capability token for a user file. Two
//! cases may still pass a raw path, and both are narrow:
//!
//! * **App-owned directories** (cache/data/config/temp): the app itself chooses
//!   these paths (staging a camera photo, an output default), and they are
//!   registered at startup. The webview cannot use them to reach user files.
//! * **Paths the backend produced**: a save target resolved from a token, or a
//!   file the backend wrote, is registered as trusted so the follow-up
//!   fingerprint/read commands keep working. A path the webview invents is
//!   never trusted.
//!
//! Everything else is refused at the validator (`paths::lexical` and
//! `paths::directory` included) instead of being accepted because a command
//! happens to exist.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use pdfcore::error::PdfError;

/// Where the persisted directory grants live, under the app config directory.
const GRANTS_FILE: &str = "path-grants.json";
/// How many configured folders the app remembers. A user configures a handful;
/// the bound keeps a tampered file from growing the trusted set without limit.
const MAX_GRANTS: usize = 32;

/// Whether any raw path from the webview is accepted (debug/E2E and Android's
/// SAF bridge return real cache paths).
pub(crate) fn raw_paths_allowed() -> bool {
    cfg!(any(debug_assertions, target_os = "android"))
}

pub(crate) const RAW_PATH_ERROR: &str = "the renderer must pass a file token instead of a path";

fn owned_roots() -> &'static Mutex<Vec<PathBuf>> {
    static ROOTS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    ROOTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn trusted_paths() -> &'static Mutex<Vec<PathBuf>> {
    static TRUSTED: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    TRUSTED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Registers a directory the app owns (cache, data, config, temp, log).
pub(crate) fn register_owned_root(path: impl AsRef<Path>) {
    let path = normalize_for_policy(path.as_ref());
    if path.as_os_str().is_empty() {
        return;
    }
    if let Ok(mut roots) = owned_roots().lock() {
        if !roots.iter().any(|root| root == &path) {
            roots.push(path);
        }
    }
}

/// Registers a path the backend itself produced (written output, save target).
/// Trust is additive and process-local: a token-expired path stays readable
/// only while the app runs, like a token.
pub(crate) fn trust_path(path: impl AsRef<Path>) {
    let path = normalize_for_policy(path.as_ref());
    if path.as_os_str().is_empty() {
        return;
    }
    if let Ok(mut trusted) = trusted_paths().lock() {
        if !trusted.iter().any(|known| known == &path) {
            trusted.push(path);
        }
    }
}

/// Loads the persisted directory grants and trusts them for this process.
///
/// A grant is created when the human picks a folder in the Rust dialog (see
/// `file_dirs::issue_dir`), so it is the durable form of "the user chose this
/// folder": the configured default output folder and the AI library keep working
/// after a restart, without a live token and without trusting whatever path the
/// webview sends. Returns how many grants were loaded.
pub(crate) fn load_grants(config_dir: &Path) -> usize {
    let path = config_dir.join(GRANTS_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else { return 0 };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { return 0 };
    let mut loaded = 0;
    if let Some(entries) = value.get("grants").and_then(serde_json::Value::as_array) {
        for entry in entries.iter().take(MAX_GRANTS) {
            let Some(text) = entry.as_str() else { continue };
            let candidate = PathBuf::from(text);
            if !candidate.is_absolute() {
                continue;
            }
            trust_path(&candidate);
            loaded += 1;
        }
    }
    loaded
}

/// Records a folder the human picked and trusts it now. Best effort: a folder
/// that cannot be persisted still works for this session.
pub(crate) fn persist_grant(config_dir: &Path, path: &Path) -> Result<(), PdfError> {
    let normalized = normalize_for_policy(path);
    if normalized.as_os_str().is_empty() || !normalized.is_absolute() {
        return Err(PdfError::InvalidInput("the output folder is not an absolute path".into()));
    }
    trust_path(&normalized);
    let file = config_dir.join(GRANTS_FILE);
    let mut grants: Vec<PathBuf> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("grants").and_then(serde_json::Value::as_array).cloned())
        .map(|entries| entries.iter().filter_map(|entry| entry.as_str()).map(PathBuf::from).collect())
        .unwrap_or_default();
    grants.retain(|known| known != &normalized);
    grants.push(normalized);
    if grants.len() > MAX_GRANTS {
        grants.drain(..grants.len() - MAX_GRANTS);
    }
    let payload = serde_json::json!({
        "grants": grants.iter().map(|grant| grant.display().to_string()).collect::<Vec<_>>(),
    });
    let text = payload.to_string();
    officecore::io::write_atomic(&file, text.as_bytes())
        .map_err(|error| PdfError::Internal(format!("the folder grant could not be saved: {error}")))?;
    Ok(())
}

fn normalize_for_policy(path: &Path) -> PathBuf {
    // A lexical cleanup only: no canonicalize(), so a path that does not exist
    // yet (a save target) can still be registered.
    let mut out = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn is_under(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

/// The decision without side effects, so tests can cover every branch.
pub(crate) fn raw_path_decision(
    path: &Path,
    raw_allowed: bool,
    owned_roots: &[PathBuf],
    trusted_paths: &[PathBuf],
) -> bool {
    if raw_allowed {
        return true;
    }
    owned_roots.iter().any(|root| is_under(path, root)) || trusted_paths.iter().any(|known| is_under(path, known))
}

/// Refuses a raw path unless it is an app-owned path or one the backend made.
pub(crate) fn ensure_raw_path_allowed(path: &Path) -> Result<(), PdfError> {
    let path = normalize_for_policy(path);
    let owned = owned_roots().lock().map(|roots| roots.clone()).unwrap_or_default();
    let trusted = trusted_paths().lock().map(|paths| paths.clone()).unwrap_or_default();
    if raw_path_decision(&path, raw_paths_allowed(), &owned, &trusted) {
        Ok(())
    } else {
        Err(PdfError::InvalidInput(RAW_PATH_ERROR.to_string()))
    }
}

/// Refuses every raw path. Kept for commands whose raw branch is only ever
/// used by debug builds (input/output file validators) and for exact parity
/// with the previous policy.
pub(crate) fn ensure_raw_allowed() -> Result<(), PdfError> {
    if raw_paths_allowed() {
        Ok(())
    } else {
        Err(PdfError::InvalidInput(RAW_PATH_ERROR.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    #[test]
    fn debug_builds_accept_and_release_builds_need_trust() {
        let roots = vec![path("/app/cache")];
        let trusted = vec![path("/home/user/out.pdf")];
        assert!(raw_path_decision(&path("/etc/passwd"), true, &roots, &trusted));
        assert!(!raw_path_decision(&path("/etc/passwd"), false, &roots, &trusted));
        assert!(raw_path_decision(&path("/app/cache/scans/a.jpg"), false, &roots, &trusted));
        assert!(raw_path_decision(&path("/app/cache/scans"), false, &roots, &trusted));
        assert!(raw_path_decision(&path("/home/user/out.pdf"), false, &roots, &trusted));
        assert!(!raw_path_decision(&path("/app/cached-other/x"), false, &roots, &trusted));
    }

    #[test]
    fn registration_normalizes_and_deduplicates() {
        register_owned_root("/tmp/ownership-test/./cache/../cache");
        register_owned_root("/tmp/ownership-test/cache");
        let roots = owned_roots().lock().unwrap().clone();
        let matches: Vec<&PathBuf> = roots.iter().filter(|root| root.ends_with("ownership-test/cache")).collect();
        assert_eq!(matches.len(), 1, "duplicate roots are collapsed: {roots:?}");
    }

    #[test]
    fn a_grant_survives_a_restart_and_only_covers_its_own_folder() {
        // A folder the human picked is persisted as a grant; the next launch
        // trusts it, which is what keeps the configured output folder working
        // without a live token (E1).
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("config");
        std::fs::create_dir_all(&config).expect("config dir");
        let output = dir.path().join("Chosen Output");
        std::fs::create_dir_all(&output).expect("output dir");

        persist_grant(&config, &output).expect("persist the grant");
        let file = config.join(GRANTS_FILE);
        let saved = std::fs::read_to_string(&file).expect("grant file");
        assert!(saved.contains("Chosen Output"), "{saved}");

        // The next launch loads it into the trusted set. The decision is asked
        // with `raw_allowed = false` (what a release build does): the test
        // process is a debug build, where every raw path is accepted on purpose.
        assert!(load_grants(&config) >= 1);
        let trusted = trusted_paths().lock().unwrap().clone();
        let accepted = output.join("rapor.pdf");
        assert!(raw_path_decision(&accepted, false, &[], &trusted), "a granted folder is accepted");
        let refused = dir.path().join("Elsewhere/rapor.pdf");
        assert!(!raw_path_decision(&refused, false, &[], &trusted), "a folder the user never picked is not");
    }

    #[test]
    fn a_grant_must_be_an_absolute_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("config");
        std::fs::create_dir_all(&config).expect("config dir");
        let error = persist_grant(&config, Path::new("relative/output")).expect_err("relative paths are refused");
        assert!(error.to_string().contains("absolute"), "{error}");
        assert!(!config.join(GRANTS_FILE).exists(), "nothing is written for a refused grant");
    }

    #[test]
    fn trusted_paths_cover_their_tree() {
        trust_path("/tmp/trust-test/out.pdf");
        let trusted = trusted_paths().lock().unwrap().clone();
        assert!(raw_path_decision(&path("/tmp/trust-test/out.pdf"), false, &[], &trusted));
        assert!(!raw_path_decision(&path("/tmp/trust-test/other.pdf"), false, &[], &trusted));
    }
}
