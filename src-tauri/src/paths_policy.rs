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
    fn trusted_paths_cover_their_tree() {
        trust_path("/tmp/trust-test/out.pdf");
        let trusted = trusted_paths().lock().unwrap().clone();
        assert!(raw_path_decision(&path("/tmp/trust-test/out.pdf"), false, &[], &trusted));
        assert!(!raw_path_decision(&path("/tmp/trust-test/other.pdf"), false, &[], &trusted));
    }
}
