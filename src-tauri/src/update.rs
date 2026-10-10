//! "A new version is available" check against the project's GitHub releases.
//!
//! Releases are published only on GitHub (Windows installer, Android APKs),
//! so the app asks the public releases API for the latest release, compares
//! its tag with the running version and hands the frontend the matching
//! download for this platform. It is an anonymous GET with no identifiers;
//! the frontend decides when to call it (weekly by default, switchable off).
//! Nothing is downloaded or installed here: the user opens the release page
//! or the installer link in the browser.

use serde::Serialize;
use std::time::Duration;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/ozanstn1-stack/omnioffice/releases/latest";
/// Only links under this prefix may be opened by [`update_open`].
const RELEASES_PREFIX: &str = "https://github.com/ozanstn1-stack/omnioffice/releases/";
const MAX_NOTES_CHARS: usize = 4000;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub release_url: String,
    /// Installer/APK for this platform and CPU, when the release has one.
    pub download_url: Option<String>,
    pub download_name: Option<String>,
    /// SHA-256 of the chosen asset when the GitHub API reports one
    /// (`digest: "sha256:..."`). The Android updater verifies the download
    /// against it before opening the installer.
    pub sha256: Option<String>,
    /// Release notes (Markdown), shortened.
    pub notes: String,
}

/// Parses `v1.2.3`, `1.2.3` or `1.2.3-beta` into a comparable triple. A
/// pre-release suffix is ignored; anything else is `None`.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Picks the release asset for `os`/`arch` (values of `std::env::consts`).
fn pick_asset<'a>(assets: &'a [serde_json::Value], os: &str, arch: &str) -> Option<&'a serde_json::Value> {
    let name_of =
        |asset: &serde_json::Value| asset.get("name").and_then(|name| name.as_str()).unwrap_or("").to_string();
    let wanted = |name: &str| -> bool {
        match os {
            "windows" => name.starts_with("OmniOffice-Setup-") && name.ends_with(".exe"),
            "android" => {
                let abi = if arch == "arm" { "-armeabi-v7a.apk" } else { "-arm64-v8a.apk" };
                name.starts_with("OmniOffice-Android-") && name.ends_with(abi)
            }
            _ => false,
        }
    };
    assets.iter().find(|asset| wanted(&name_of(asset)))
}

/// Builds the update answer from a GitHub "latest release" JSON document.
pub fn evaluate_release(
    release: &serde_json::Value,
    current: &str,
    os: &str,
    arch: &str,
) -> Result<UpdateInfo, String> {
    let tag = release.get("tag_name").and_then(|tag| tag.as_str()).ok_or("the release has no tag")?;
    let latest = parse_version(tag).ok_or_else(|| format!("unrecognised release tag {tag}"))?;
    let running = parse_version(current).ok_or_else(|| format!("unrecognised app version {current}"))?;
    let release_url = release
        .get("html_url")
        .and_then(|url| url.as_str())
        .filter(|url| url.starts_with(RELEASES_PREFIX))
        .unwrap_or("https://github.com/ozanstn1-stack/omnioffice/releases/latest")
        .to_string();
    let assets = release.get("assets").and_then(|assets| assets.as_array()).cloned().unwrap_or_default();
    let asset = pick_asset(&assets, os, arch);
    let download_url = asset
        .and_then(|asset| asset.get("browser_download_url"))
        .and_then(|url| url.as_str())
        .filter(|url| url.starts_with(RELEASES_PREFIX))
        .map(str::to_string);
    let download_name = download_url
        .as_ref()
        .and(asset)
        .and_then(|asset| asset.get("name"))
        .and_then(|name| name.as_str())
        .map(str::to_string);
    let sha256 = asset
        .and_then(|asset| asset.get("digest"))
        .and_then(|digest| digest.as_str())
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|hex| hex.to_ascii_lowercase());
    let notes: String =
        release.get("body").and_then(|body| body.as_str()).unwrap_or("").chars().take(MAX_NOTES_CHARS).collect();
    Ok(UpdateInfo {
        current: current.to_string(),
        latest: format!("{}.{}.{}", latest.0, latest.1, latest.2),
        newer: latest > running,
        release_url,
        download_url,
        download_name,
        sha256,
        notes,
    })
}

#[tauri::command]
pub async fn update_check() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("OmniOffice/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Could not create the HTTP client.".to_string())?;
    let mut response = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "Could not reach GitHub. Check the internet connection.".to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub answered {}.", response.status().as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "The update answer could not be read.".to_string())? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("The update answer is too large.".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    let release: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "The update answer is not valid JSON.".to_string())?;
    evaluate_release(&release, env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH)
}

/// Opens a release page or download in the system browser. Only links to
/// this project's GitHub releases are accepted.
#[tauri::command]
pub fn update_open(url: String) -> Result<(), String> {
    if !url.starts_with(RELEASES_PREFIX) || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Only OmniOffice release links can be opened.".to_string());
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str) -> serde_json::Value {
        let base = "https://github.com/ozanstn1-stack/omnioffice/releases/download";
        let digest = format!("sha256:{}", "ab".repeat(32));
        json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/ozanstn1-stack/omnioffice/releases/tag/{tag}"),
            "body": "notes",
            "assets": [
                { "name": "OmniOffice-Portable-3.9.0.zip", "browser_download_url": format!("{base}/{tag}/OmniOffice-Portable-3.9.0.zip") },
                { "name": "OmniOffice_3.9.0_x64-setup.exe", "browser_download_url": format!("{base}/{tag}/OmniOffice_3.9.0_x64-setup.exe"), "digest": digest },
                { "name": "OmniOffice-Setup-3.9.0.exe", "browser_download_url": format!("{base}/{tag}/OmniOffice-Setup-3.9.0.exe"), "digest": digest },
                { "name": "OmniOffice-Android-3.9.0-arm64-v8a.apk", "browser_download_url": format!("{base}/{tag}/OmniOffice-Android-3.9.0-arm64-v8a.apk"), "digest": digest },
                { "name": "OmniOffice-Android-3.9.0-armeabi-v7a.apk", "browser_download_url": format!("{base}/{tag}/OmniOffice-Android-3.9.0-armeabi-v7a.apk"), "digest": digest },
                { "name": "OmniOffice-Android-3.9.0-arm64-v8a.aab", "browser_download_url": format!("{base}/{tag}/OmniOffice-Android-3.9.0-arm64-v8a.aab") }
            ]
        })
    }

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(parse_version("v3.8.2"), Some((3, 8, 2)));
        assert_eq!(parse_version("3.10"), Some((3, 10, 0)));
        assert_eq!(parse_version("4.0.0-beta.1"), Some((4, 0, 0)));
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(parse_version("3.10.0") > parse_version("3.9.9"));
    }

    #[test]
    fn picks_the_installer_and_the_matching_apk() {
        let expected_digest = "ab".repeat(32);
        let windows = evaluate_release(&release("v3.9.0"), "3.8.3", "windows", "x86_64").unwrap();
        assert!(windows.newer);
        assert_eq!(windows.latest, "3.9.0");
        assert_eq!(windows.download_name.as_deref(), Some("OmniOffice-Setup-3.9.0.exe"));
        assert_eq!(windows.sha256.as_deref(), Some(expected_digest.as_str()));

        let phone = evaluate_release(&release("v3.9.0"), "3.8.3", "android", "aarch64").unwrap();
        assert_eq!(phone.download_name.as_deref(), Some("OmniOffice-Android-3.9.0-arm64-v8a.apk"));
        assert!(phone.sha256.is_some(), "the APK digest must reach the updater");
        let old_phone = evaluate_release(&release("v3.9.0"), "3.8.3", "android", "arm").unwrap();
        assert_eq!(old_phone.download_name.as_deref(), Some("OmniOffice-Android-3.9.0-armeabi-v7a.apk"));

        let linux = evaluate_release(&release("v3.9.0"), "3.8.3", "linux", "x86_64").unwrap();
        assert_eq!(linux.download_url, None);
        assert_eq!(linux.sha256, None);
        assert!(linux.release_url.ends_with("/releases/tag/v3.9.0"));
    }

    #[test]
    fn malformed_digests_are_ignored() {
        let expected_digest = "ab".repeat(32);
        let mut doc = release("v3.9.0");
        doc["assets"][2]["digest"] = json!("sha256:not-a-hash");
        let info = evaluate_release(&doc, "3.8.3", "windows", "x86_64").unwrap();
        assert_eq!(info.sha256, None);

        doc["assets"][2]["digest"] = json!("md5:abcd");
        assert_eq!(evaluate_release(&doc, "3.8.3", "windows", "x86_64").unwrap().sha256, None);

        doc["assets"][2]["digest"] = json!(format!("sha256:{}", "AB".repeat(32)));
        assert_eq!(
            evaluate_release(&doc, "3.8.3", "windows", "x86_64").unwrap().sha256.as_deref(),
            Some(expected_digest.as_str()),
            "hex digests are normalised to lower case"
        );
    }

    #[test]
    fn same_or_older_release_is_not_newer() {
        assert!(!evaluate_release(&release("v3.8.3"), "3.8.3", "windows", "x86_64").unwrap().newer);
        assert!(!evaluate_release(&release("v3.8.0"), "3.8.3", "windows", "x86_64").unwrap().newer);
    }

    #[test]
    fn foreign_links_are_dropped() {
        let mut doc = release("v9.0.0");
        doc["html_url"] = json!("https://evil.example/release");
        doc["assets"][2]["browser_download_url"] = json!("https://evil.example/OmniOffice-Setup-9.0.0.exe");
        let info = evaluate_release(&doc, "3.8.3", "windows", "x86_64").unwrap();
        assert_eq!(info.release_url, "https://github.com/ozanstn1-stack/omnioffice/releases/latest");
        assert_eq!(info.download_url, None);
        assert_eq!(info.download_name, None);
    }

    #[test]
    fn bad_tags_are_errors() {
        assert!(evaluate_release(&json!({ "tag_name": "nightly" }), "3.8.3", "windows", "x86_64").is_err());
        assert!(evaluate_release(&json!({}), "3.8.3", "windows", "x86_64").is_err());
    }

    #[test]
    fn only_release_links_may_be_opened() {
        assert!(update_open("https://evil.example/x".into()).is_err());
        assert!(update_open("https://github.com/ozanstn1-stack/omnioffice/releases/ x".into()).is_err());
    }
}
