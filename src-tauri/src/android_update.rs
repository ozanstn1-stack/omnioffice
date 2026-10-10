//! Android in-app update: fetch the release APK and hand it to the system
//! package installer (Android builds only).
//!
//! `tauri-plugin-updater` does not support Android, so the phone runs its own
//! native flow. The GitHub-releases check in [`crate::update`] stays the
//! source of the release and its `OmniOffice-Android-<version>-<abi>.apk`
//! asset (URL plus, once the release publishes checksums, the SHA-256); the
//! webview then calls [`update_download_apk`] and [`update_install_apk`].
//!
//! The download is restricted to this project's GitHub release downloads, is
//! streamed into `<cache>/updates/` while its SHA-256 is computed, and is
//! deleted on any error, so the installer never sees a truncated or tampered
//! file. Installing goes through the Kotlin bridge (`ApkInstallerPlugin.kt`),
//! which builds the `content://` URI (FileProvider) and opens the system
//! installer - the same Tauri mobile plugin pattern as the Keystore and the
//! foreground service (`android_keystore.rs`, `android_background.rs`):
//! a Kotlin `@TauriPlugin` in the app module, registered with
//! `register_android_plugin` and called with `run_mobile_plugin`.
//!
//! The module is compiled for Android and for host tests; the pure helpers
//! below are separated from the command bodies so they can be tested on any
//! platform.

use std::path::Path;

#[cfg(target_os = "android")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "android")]
use std::io::Write;
#[cfg(target_os = "android")]
use std::time::Duration;

/// Only downloads under this prefix are accepted (mirrors `update.rs`).
const DOWNLOAD_PREFIX: &str = "https://github.com/ozanstn1-stack/omnioffice/releases/download/";
/// Hard cap for an APK; the largest release file is far below this.
#[cfg(target_os = "android")]
const MAX_APK_BYTES: u64 = 256 * 1024 * 1024;
/// Subdirectory of the app cache the APK is downloaded into.
const UPDATES_DIR: &str = "updates";
/// Progress event consumed by the update screen.
#[cfg(target_os = "android")]
const PROGRESS_EVENT: &str = "update:progress";
/// Minimum growth between two progress events, so one download does not flood
/// the webview with messages for every network chunk.
#[cfg(target_os = "android")]
const PROGRESS_STEP: u64 = 256 * 1024;

/// True when `url` is a download of this project's GitHub releases.
pub fn is_allowed_url(url: &str) -> bool {
    url.starts_with(DOWNLOAD_PREFIX) && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Storage name for the APK behind `url`: `OmniOffice-Android-4.6.1-arm64-v8a.apk`
/// becomes `OmniOffice-4.6.1.apk`, so a re-download of the same release
/// replaces the previous file instead of piling up per-ABI copies. Anything
/// that is not an `.apk` is rejected; a file name that does not follow the
/// release pattern is sanitized rather than trusted as a path.
pub fn apk_file_name(url: &str) -> Option<String> {
    let last = url.rsplit('/').next()?;
    let last = last.split(['?', '#']).next()?;
    let stem = last.strip_suffix(".apk")?;
    let core = stem.strip_prefix("OmniOffice-Android-").unwrap_or(stem);
    let core = core.strip_suffix("-arm64-v8a").or_else(|| core.strip_suffix("-armeabi-v7a")).unwrap_or(core);
    let safe: String =
        core.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    if safe.is_empty() {
        return None;
    }
    Some(format!("OmniOffice-{safe}.apk"))
}

/// Lower-case hex SHA-256, the same format as the release checksum files.
pub fn sha256_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Compares the computed digest with `expected` (when the release provided
/// one). A mismatch deletes the downloaded file - it must never reach the
/// installer - and reports a retryable error.
pub fn verify_digest_or_discard(path: &Path, computed: &str, expected: Option<&str>) -> Result<(), String> {
    match expected {
        Some(expected) if !computed.eq_ignore_ascii_case(expected.trim()) => {
            let _ = std::fs::remove_file(path);
            Err("The downloaded update failed its checksum and was deleted. Please try again.".to_string())
        }
        _ => Ok(()),
    }
}

/// Accepts only an existing `.apk` inside `<cache>/updates`, so a compromised
/// webview cannot point the installer at an arbitrary file.
pub fn validate_install_path(cache_dir: &Path, path: &Path) -> Result<(), String> {
    let updates =
        cache_dir.join(UPDATES_DIR).canonicalize().map_err(|_| "No update has been downloaded yet.".to_string())?;
    let candidate =
        path.canonicalize().map_err(|_| "The downloaded update is gone. Please download it again.".to_string())?;
    if !candidate.starts_with(&updates) || !candidate.is_file() {
        return Err("Only a downloaded update can be installed.".to_string());
    }
    let is_apk = candidate.extension().and_then(|ext| ext.to_str()).map(|ext| ext.eq_ignore_ascii_case("apk"));
    if is_apk != Some(true) {
        return Err("Only an APK can be installed.".to_string());
    }
    Ok(())
}

/// Payload of the `update:progress` event; `total` is absent when the server
/// does not announce a length (the download of a release normally does).
#[cfg(target_os = "android")]
#[derive(Clone, serde::Serialize)]
struct Progress {
    received: u64,
    total: Option<u64>,
}

/// Downloads the release APK into `<cache>/updates/OmniOffice-<version>.apk`
/// and returns its absolute path. Only links to this project's GitHub release
/// downloads are accepted; the body is capped, streamed and hashed, and the
/// file is removed again on any failure.
#[cfg(target_os = "android")]
#[tauri::command]
pub async fn update_download_apk(
    app: tauri::AppHandle,
    url: String,
    expected_sha256: Option<String>,
) -> Result<String, String> {
    use tauri::{Emitter, Manager};
    if !is_allowed_url(&url) {
        return Err("Only OmniOffice release downloads can be fetched.".to_string());
    }
    let file_name = apk_file_name(&url).ok_or_else(|| "The update link does not name an APK.".to_string())?;
    let cache_dir = app.path().app_cache_dir().map_err(|_| "The app cache directory is not available.".to_string())?;
    let updates_dir = cache_dir.join(UPDATES_DIR);
    std::fs::create_dir_all(&updates_dir).map_err(|_| "The update directory could not be created.".to_string())?;
    let destination = updates_dir.join(file_name);

    // Connect quickly, but let the body run: a release APK is far larger than
    // the release JSON, so a total timeout would cut it off. The read timeout
    // guards a stalled connection and MAX_APK_BYTES bounds the size.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .user_agent(concat!("OmniOffice/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Could not create the HTTP client.".to_string())?;
    let mut response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| "Could not reach GitHub. Check the internet connection.".to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub answered {}.", response.status().as_u16()));
    }
    let total = response.content_length();
    let mut file =
        std::fs::File::create(&destination).map_err(|_| "The update file could not be written.".to_string())?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut next_report: u64 = 0;
    let transfer = async {
        while let Some(chunk) = response.chunk().await.map_err(|_| "The download was interrupted.".to_string())? {
            received += chunk.len() as u64;
            if received > MAX_APK_BYTES {
                return Err("The update file is larger than expected. Download stopped.".to_string());
            }
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|_| "The update file could not be written.".to_string())?;
            if received >= next_report {
                let _ = app.emit(PROGRESS_EVENT, Progress { received, total });
                next_report = received + PROGRESS_STEP;
            }
        }
        file.flush().map_err(|_| "The update file could not be written.".to_string())
    }
    .await;
    if let Err(error) = transfer {
        drop(file);
        let _ = std::fs::remove_file(&destination);
        return Err(error);
    }
    drop(file);

    let digest = sha256_hex(&hasher.finalize());
    verify_digest_or_discard(&destination, &digest, expected_sha256.as_deref())?;
    let _ = app.emit(PROGRESS_EVENT, Progress { received, total });
    Ok(destination.to_string_lossy().into_owned())
}

/// Launches the system package installer for a previously downloaded APK.
/// The path is validated against the app cache first; the installer itself
/// runs in `ApkInstallerPlugin.kt` through the mobile plugin bridge.
#[cfg(target_os = "android")]
#[tauri::command]
pub fn update_install_apk(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri::Manager;
    let cache_dir = app.path().app_cache_dir().map_err(|_| "The app cache directory is not available.".to_string())?;
    let candidate = std::path::PathBuf::from(path);
    validate_install_path(&cache_dir, &candidate)?;
    call_install(&candidate.to_string_lossy())
}

#[cfg(target_os = "android")]
const PLUGIN_NAME: &str = "omnioffice-apk-installer";
#[cfg(target_os = "android")]
const KOTLIN_PACKAGE: &str = "io.github.ozanstn1.pdfswissarmyknife";
#[cfg(target_os = "android")]
const KOTLIN_CLASS: &str = "ApkInstallerPlugin";

#[cfg(target_os = "android")]
static HANDLE: std::sync::OnceLock<tauri::plugin::PluginHandle<tauri::Wry>> = std::sync::OnceLock::new();

/// The plugin to register on the app builder. A failed registration is logged
/// and swallowed: the download still works and the browser-based fallback of
/// `update.rs` stays available.
#[cfg(target_os = "android")]
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new(PLUGIN_NAME)
        .setup(|_app, api| {
            match api.register_android_plugin(KOTLIN_PACKAGE, KOTLIN_CLASS) {
                Ok(handle) => {
                    let _ = HANDLE.set(handle);
                }
                Err(error) => eprintln!("could not register the Android APK installer plugin: {error}"),
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
#[derive(serde::Serialize)]
struct InstallPayload<'a> {
    path: &'a str,
}

/// Hands the validated path to the Kotlin plugin, which opens the system
/// installer for it.
#[cfg(target_os = "android")]
fn call_install(path: &str) -> Result<(), String> {
    let handle = HANDLE.get().ok_or_else(|| "The Android installer bridge is not ready.".to_string())?;
    handle
        .run_mobile_plugin::<()>("installApk", InstallPayload { path })
        .map_err(|error| format!("The system installer could not be opened: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset_name(base: &str) -> String {
        format!("{base}/v4.6.1/OmniOffice-Android-4.6.1-arm64-v8a.apk")
    }

    #[test]
    fn only_release_downloads_are_accepted() {
        let base = "https://github.com/ozanstn1-stack/omnioffice/releases/download";
        assert!(is_allowed_url(&asset_name(base)));
        assert!(is_allowed_url(&format!("{base}/v4.6.1/OmniOffice-Android-4.6.1-armeabi-v7a.apk")));
        assert!(!is_allowed_url("https://evil.example/v4.6.1/OmniOffice-Android-4.6.1-arm64-v8a.apk"));
        assert!(!is_allowed_url("https://github.com/other/repo/releases/download/v4.6.1/app.apk"));
        assert!(!is_allowed_url("http://github.com/ozanstn1-stack/omnioffice/releases/download/v4.6.1/app.apk"));
        assert!(!is_allowed_url(&format!("{base}/v4.6.1/app.apk\n")));
    }

    #[test]
    fn download_names_follow_the_omni_office_pattern() {
        let base = "https://github.com/ozanstn1-stack/omnioffice/releases/download";
        assert_eq!(apk_file_name(&asset_name(base)).as_deref(), Some("OmniOffice-4.6.1.apk"));
        assert_eq!(
            apk_file_name(&format!("{base}/v4.6.1/OmniOffice-Android-4.6.1-armeabi-v7a.apk")).as_deref(),
            Some("OmniOffice-4.6.1.apk")
        );
        // Query strings and generic names are handled without trusting them.
        assert_eq!(
            apk_file_name(&format!("{base}/v4.6.1/download.apk?token=x")).as_deref(),
            Some("OmniOffice-download.apk")
        );
        assert_eq!(apk_file_name(&format!("{base}/v4.6.1/OmniOffice-Android-4.6.1-arm64-v8a.zip")), None);
        assert_eq!(apk_file_name(&format!("{base}/v4.6.1/../.apk")), None);
    }

    #[test]
    fn a_bad_checksum_deletes_the_download() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("OmniOffice-4.6.1.apk");
        std::fs::write(&file, b"apk bytes").expect("write apk");
        let digest = sha256_hex(b"apk bytes");

        // A mismatch (and a tampered digest) removes the file.
        assert!(verify_digest_or_discard(&file, &digest, Some("00ff")).is_err());
        assert!(!file.exists());

        // The expected digest is compared case-insensitively and trimmed.
        std::fs::write(&file, b"apk bytes").expect("rewrite apk");
        assert!(verify_digest_or_discard(&file, &digest, Some(&format!("  {}  ", digest.to_uppercase()))).is_ok());
        assert!(file.exists());
        assert!(verify_digest_or_discard(&file, &digest, None).is_ok());
        assert!(file.exists());
    }

    #[test]
    fn only_cache_apks_can_be_installed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let updates = dir.path().join(UPDATES_DIR);
        std::fs::create_dir_all(&updates).expect("create updates dir");
        let apk = updates.join("OmniOffice-4.6.1.apk");
        std::fs::write(&apk, b"apk bytes").expect("write apk");
        assert!(validate_install_path(dir.path(), &apk).is_ok());

        let text = updates.join("notes.txt");
        std::fs::write(&text, b"no").expect("write text");
        assert!(validate_install_path(dir.path(), &text).is_err());

        let outside = dir.path().join("evil.apk");
        std::fs::write(&outside, b"apk").expect("write outside");
        assert!(validate_install_path(dir.path(), &outside).is_err());
        assert!(validate_install_path(dir.path(), &updates.join("missing.apk")).is_err());
    }
}
