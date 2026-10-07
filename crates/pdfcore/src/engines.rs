//! Locates the optional native engines (pdfium, qpdf, tesseract) that ship
//! with the installer. All resolution is local - nothing is downloaded at
//! runtime.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static ENGINE_BASE: OnceLock<PathBuf> = OnceLock::new();

/// Android: the app's private files directory (`<data dir>/files`). The APK
/// assets holding the OCR language models are copied here on first launch by
/// the Android shell, and the app tells pdfcore about it at startup.
#[cfg(target_os = "android")]
static ANDROID_FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_files_dir(dir: PathBuf) {
    let _ = ANDROID_FILES_DIR.set(dir);
}

/// Android: the directory that holds the extracted native libraries
/// (`/data/app/<pkg>/lib/<abi>`). It is the only place the system lets an app
/// execute a bundled binary from, so the Tesseract CLI lives there as
/// `libtesseract.so` and pdfium as `libpdfium.so`.
#[cfg(target_os = "android")]
pub fn native_library_dir() -> Option<PathBuf> {
    use std::sync::OnceLock;
    static NATIVE_LIB_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    NATIVE_LIB_DIR
        .get_or_init(|| {
            let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
            let mut fallback = None;
            for line in maps.lines() {
                let Some(path) = line.split_whitespace().last() else {
                    continue;
                };
                if !path.ends_with(".so") || !path.contains("/lib/") {
                    continue;
                }
                let candidate = Path::new(path).parent()?.to_path_buf();
                if path.contains("libpdf_sak_lib.so") {
                    return Some(candidate);
                }
                if fallback.is_none() {
                    fallback = Some(candidate);
                }
            }
            fallback
        })
        .clone()
}

/// Windows returns extended-length paths (`\\?\D:\...`) from some APIs
/// (including Tauri's resource dir). External tools like Tesseract cannot
/// parse that prefix, so strip it before handing paths to child processes.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

/// The application tells pdfcore where the bundled engines live
/// (e.g. `<install dir>/resources/engines`).
pub fn set_engine_base(dir: PathBuf) {
    let _ = ENGINE_BASE.set(strip_verbatim(dir));
}

/// All candidate directories that may contain an `engines` folder.
pub fn candidate_bases() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(base) = ENGINE_BASE.get() {
        dirs.push(base.clone());
    }
    if let Ok(env) = std::env::var("PDFSAK_ENGINES_DIR") {
        dirs.push(PathBuf::from(env));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("engines"));
            dirs.push(dir.join("resources").join("engines"));
            if let Some(parent) = dir.parent() {
                dirs.push(parent.join("resources").join("engines"));
                dirs.push(parent.join("engines"));
            }
        }
    }
    // Development fallback: the source tree (desktop only - the path only
    // exists on the machine that built the app).
    #[cfg(not(target_os = "android"))]
    {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src-tauri")
            .join("resources")
            .join("engines");
        dirs.push(dev);
    }
    dirs
}

#[cfg(not(target_os = "android"))]
fn find_in_bases(relative: &str) -> Option<PathBuf> {
    for base in candidate_bases() {
        let candidate = strip_verbatim(base.join(relative));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(not(target_os = "android"))]
fn find_in_path(exe_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = strip_verbatim(dir.join(exe_name));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn pdfium_path() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        // Packaged as a native library; the system loader finds it by name,
        // but an explicit path keeps the diagnostic output useful.
        return native_library_dir().map(|dir| dir.join("libpdfium.so")).filter(|path| path.exists());
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Some(p) = find_in_bases("pdfium/pdfium.dll") {
            return Some(p);
        }
        // Dev convenience: the fetch script also drops a copy next to the lib.
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("pdfium.dll");
        if dev.exists() {
            return Some(dev);
        }
        find_in_path("pdfium.dll")
    }
}

pub fn qpdf_path() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        None
    }
    #[cfg(windows)]
    {
        find_in_bases("qpdf/qpdf.exe").or_else(|| find_in_path("qpdf.exe"))
    }
    #[cfg(not(any(windows, target_os = "android")))]
    {
        // Linux and macOS builds bundle no qpdf; a system install is used.
        // Apps started from the macOS Finder do not inherit Homebrew's PATH,
        // so its prefixes are checked as well.
        let homebrew: &[&str] =
            if cfg!(target_os = "macos") { &["/opt/homebrew/bin/qpdf", "/usr/local/bin/qpdf"] } else { &[] };
        find_in_bases("qpdf/qpdf.exe")
            .or_else(|| find_in_path("qpdf.exe"))
            .or_else(|| find_in_path("qpdf"))
            .or_else(|| homebrew.iter().map(PathBuf::from).find(|path| path.is_file()))
    }
}

pub fn tesseract_path() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        if let Some(override_path) = std::env::var_os("PDFSAK_TESSERACT") {
            return Some(PathBuf::from(override_path));
        }
        let dir = native_library_dir()?;
        let candidate = dir.join("libtesseract.so");
        if candidate.exists() {
            return Some(candidate);
        }
        None
    }
    #[cfg(not(target_os = "android"))]
    {
        find_in_bases("tesseract/tesseract.exe").or_else(|| find_in_path("tesseract.exe"))
    }
}

pub fn tessdata_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        let dir = ANDROID_FILES_DIR.get()?.join("tessdata");
        if dir.exists() {
            return Some(dir);
        }
        None
    }
    #[cfg(not(target_os = "android"))]
    {
        let bundled = find_in_bases("tesseract/tessdata");
        if bundled.is_some() {
            return bundled;
        }
        if let Some(exe) = tesseract_path() {
            let local = exe.parent()?.join("tessdata");
            if local.exists() {
                return Some(local);
            }
        }
        std::env::var_os("TESSDATA_PREFIX").map(PathBuf::from)
    }
}

/// Builds a `Command` for tesseract with a hardened environment
/// (TESSDATA_PREFIX always points at a local directory, cwd is neutral).
pub fn tesseract_command() -> Option<Command> {
    let exe = tesseract_path()?;
    let mut cmd = Command::new(exe);
    if let Some(data) = tessdata_dir() {
        cmd.env("TESSDATA_PREFIX", data);
    }
    Some(cmd)
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub pdfium: bool,
    pub qpdf: bool,
    pub tesseract: bool,
    pub tesseract_version: Option<String>,
    pub ocr_languages: Vec<String>,
    /// Resolved locations, useful for support/diagnostics.
    pub pdfium_path: Option<String>,
    pub tesseract_path: Option<String>,
    pub tessdata_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OcrLanguage {
    pub code: String,
    pub name: String,
}

pub const SUPPORTED_OCR_LANGUAGES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("tur", "Turkish"),
    ("deu", "German"),
    ("nld", "Dutch"),
    ("fra", "French"),
    ("spa", "Spanish"),
    ("ita", "Italian"),
    ("bul", "Bulgarian"),
];

pub fn ocr_languages() -> Vec<OcrLanguage> {
    let mut out = Vec::new();
    if let Some(dir) = tessdata_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(code) = name.strip_suffix(".traineddata") {
                    let display = SUPPORTED_OCR_LANGUAGES
                        .iter()
                        .find(|(c, _)| *c == code)
                        .map(|(_, n)| n.to_string())
                        .unwrap_or_else(|| code.to_string());
                    out.push(OcrLanguage { code: code.to_string(), name: display });
                }
            }
        }
    }
    out.sort_by(|a, b| a.code.cmp(&b.code));
    out
}

pub fn engine_status() -> EngineStatus {
    let tesseract = tesseract_path();
    let version = tesseract.as_ref().and_then(|exe| {
        let mut cmd = Command::new(exe);
        if let Some(data) = tessdata_dir() {
            cmd.env("TESSDATA_PREFIX", data);
        }
        cmd.arg("--version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output = cmd.output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines().next().map(|l| l.trim().to_string())
    });
    EngineStatus {
        pdfium_path: pdfium_path().map(|p| p.display().to_string()),
        tesseract_path: tesseract.as_ref().map(|p| p.display().to_string()),
        tessdata_dir: tessdata_dir().map(|p| p.display().to_string()),
        pdfium: pdfium_path().is_some(),
        qpdf: qpdf_path().is_some(),
        tesseract: tesseract.is_some(),
        tesseract_version: version,
        ocr_languages: ocr_languages().into_iter().map(|l| l.code).collect(),
    }
}
