//! Android "Share" for documents the app holds.
//!
//! A document opened from another app (WhatsApp, a download manager, Files)
//! is copied into `cacheDir/incoming/`, and the office editors write a share
//! copy into `cacheDir/share/`. Sharing either one used to need a detour: the
//! file was first published to `Downloads/OmniOffice` and that copy was
//! shared, leaving a duplicate behind every time. `share_document_file` hands
//! the private file straight to the system share sheet through
//! `ShareFilePlugin.kt` and the app's FileProvider, which grants read access to
//! the receiving app only.
//!
//! Only regular files inside the app cache or data directory are accepted: the
//! FileProvider exposes exactly those roots, and the command must not become a
//! way to push an arbitrary path to another app. The Kotlin side is registered
//! with `register_android_plugin` and called with `run_mobile_plugin`.

#[cfg(any(target_os = "android", test))]
use std::path::{Path, PathBuf};

/// Resolves `candidate` and checks it is a regular file under one of `roots`.
///
/// Both sides are canonicalized, so `..` segments and symbolic links cannot
/// leave the allowed directories. Returns the canonical path to share.
#[cfg(any(target_os = "android", test))]
pub fn validate_share_path(roots: &[PathBuf], candidate: &Path) -> Result<PathBuf, String> {
    let resolved = candidate.canonicalize().map_err(|_| "The document to share no longer exists.".to_string())?;
    if !resolved.is_file() {
        return Err("Only documents can be shared.".to_string());
    }
    let inside = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| resolved.starts_with(&root));
    if !inside {
        return Err("This document is not in the app's storage; save or export it first.".to_string());
    }
    Ok(resolved)
}

/// Opens the system share sheet for an app-private document.
///
/// `async` keeps the work off the main thread: `app.path()` and
/// `run_mobile_plugin` both wait for the Android main thread (see
/// `android_intent::android_take_pending_open`).
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn share_document_file(
    app: tauri::AppHandle,
    path: String,
    mime_type: Option<String>,
    title: Option<String>,
) -> Result<(), String> {
    use tauri::Manager;
    let mut roots = Vec::new();
    if let Ok(cache) = app.path().app_cache_dir() {
        roots.push(cache);
    }
    if let Ok(data) = app.path().app_data_dir() {
        roots.push(data);
    }
    let resolved = validate_share_path(&roots, Path::new(&path))?;
    call_share(
        &resolved.to_string_lossy(),
        mime_type.as_deref().unwrap_or("application/octet-stream"),
        title.as_deref().unwrap_or(""),
    )
}

/// Desktop stub so `generate_handler!` stays platform independent; the
/// desktop UI reveals the file in its folder instead.
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn share_document_file(path: String, mime_type: Option<String>, title: Option<String>) -> Result<(), String> {
    let _ = (path, mime_type, title);
    Err("Sharing is only available on Android.".to_string())
}

#[cfg(target_os = "android")]
const PLUGIN_NAME: &str = "omnioffice-share";
#[cfg(target_os = "android")]
const KOTLIN_PACKAGE: &str = "io.github.ozanstn1.pdfswissarmyknife";
#[cfg(target_os = "android")]
const KOTLIN_CLASS: &str = "ShareFilePlugin";

#[cfg(target_os = "android")]
static HANDLE: std::sync::OnceLock<tauri::plugin::PluginHandle<tauri::Wry>> = std::sync::OnceLock::new();

/// The plugin to register on the app builder. A failed registration is logged
/// and swallowed: the frontend falls back to publishing and sharing a copy.
#[cfg(target_os = "android")]
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new(PLUGIN_NAME)
        .setup(|_app, api| {
            match api.register_android_plugin(KOTLIN_PACKAGE, KOTLIN_CLASS) {
                Ok(handle) => {
                    let _ = HANDLE.set(handle);
                }
                Err(error) => eprintln!("could not register the Android share plugin: {error}"),
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SharePayload<'a> {
    path: &'a str,
    mime_type: &'a str,
    title: &'a str,
}

#[cfg(target_os = "android")]
fn call_share(path: &str, mime_type: &str, title: &str) -> Result<(), String> {
    let handle = HANDLE.get().ok_or_else(|| "The Android share bridge is not ready.".to_string())?;
    handle
        .run_mobile_plugin::<()>("shareFile", SharePayload { path, mime_type, title })
        .map_err(|error| format!("The share sheet could not be opened: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfsak-share-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cache").join("incoming")).expect("create cache");
        std::fs::create_dir_all(dir.join("outside")).expect("create outside");
        dir
    }

    #[test]
    fn accepts_a_file_inside_the_cache() {
        let root = temp_root("inside");
        let file = root.join("cache").join("incoming").join("report.docx");
        std::fs::write(&file, b"PK").expect("write");
        let resolved = validate_share_path(&[root.join("cache")], &file).expect("accepted");
        assert!(resolved.ends_with("report.docx"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_files_outside_the_app_storage() {
        let root = temp_root("outside");
        let file = root.join("outside").join("secret.pdf");
        std::fs::write(&file, b"%PDF").expect("write");
        assert!(validate_share_path(&[root.join("cache")], &file).is_err());
        // A `..` walk out of the cache resolves to the same outside file.
        let walked = root.join("cache").join("..").join("outside").join("secret.pdf");
        assert!(validate_share_path(&[root.join("cache")], &walked).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_directories_and_missing_files() {
        let root = temp_root("kinds");
        assert!(validate_share_path(&[root.join("cache")], &root.join("cache").join("incoming")).is_err());
        assert!(validate_share_path(&[root.join("cache")], &root.join("cache").join("gone.pdf")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
