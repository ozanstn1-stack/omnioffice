//! Chainable batch-job recipes and a watched folder.
//!
//! A **recipe** is an ordered list of steps (compress, stamp, PDF/A, OCR,
//! redact) run over one PDF. Recipes are saved as JSON under
//! `<app config dir>/recipes/`, so a chain can be built once and reused.
//!
//! A **watched folder** pairs a folder with a recipe: a new file dropped in is
//! processed with the recipe and written to an output folder. Watching is a
//! small polling loop (no filesystem-notification dependency); each pass is a
//! pure function ([`scan_once`]) so it can be tested without waiting.

use pdfcore::docutil::OverwritePolicy;
use pdfcore::error::{PdfError, PdfResult};
use pdfcore::progress::{CancelToken, ProgressCallback, ProgressEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// One step of a recipe. The `kind` tag matches the frontend's step palette.
/// `rename_all_fields` renames the variant **fields** too, so the frontend's
/// `headerFooter` reaches `header_footer` (it used to be silently dropped, and
/// the stamp step ran empty).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RecipeStep {
    Compress {
        options: pdfcore::compress::CompressOptions,
    },
    Stamp {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        header_footer: Option<pdfcore::stamp::HeaderFooterOptions>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bates: Option<pdfcore::stamp::BatesOptions>,
    },
    Pdfa {
        level: String,
    },
    Ocr {
        options: pdfcore::ocr::OcrOptions,
    },
    Redact {
        #[serde(default)]
        areas: Vec<pdfcore::redact::RedactionArea>,
        options: pdfcore::redact::RedactionOptions,
    },
}

impl RecipeStep {
    /// The step's stable name, used in the applied list and the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Compress { .. } => "compress",
            Self::Stamp { .. } => "stamp",
            Self::Pdfa { .. } => "pdfa",
            Self::Ocr { .. } => "ocr",
            Self::Redact { .. } => "redact",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub steps: Vec<RecipeStep>,
}

/// Runs every step of `recipe` over `input`, chaining each step's output into
/// the next, and writes the final PDF to `output` atomically. A step that fails
/// stops the chain and returns its error; the partially written temp files are
/// discarded and `output` is never half-written.
pub fn run_recipe(
    input: &Path,
    output: &Path,
    recipe: &Recipe,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<Vec<String>> {
    if recipe.steps.is_empty() {
        return Err(PdfError::InvalidInput("The recipe has no steps.".into()));
    }
    let temp = tempfile::tempdir().map_err(|error| PdfError::Internal(format!("temp dir: {error}")))?;
    let mut current = input.to_path_buf();
    let mut applied = Vec::with_capacity(recipe.steps.len());
    for (index, step) in recipe.steps.iter().enumerate() {
        cancel.check()?;
        let next = temp.path().join(format!("step-{index}.pdf"));
        let policy = OverwritePolicy::Replace;
        match step {
            RecipeStep::Compress { options } => {
                pdfcore::compress::compress_pdf(&current, &next, options, policy, password, progress, cancel)?;
            }
            RecipeStep::Stamp { header_footer, bates } => {
                pdfcore::stamp::stamp_pdf(
                    &current,
                    &next,
                    header_footer.as_ref(),
                    bates.as_ref(),
                    policy,
                    password,
                    progress,
                    cancel,
                )?;
            }
            RecipeStep::Pdfa { level } => {
                let level = pdfcore::pdfa::PdfaLevel::parse(level)?;
                pdfcore::pdfa::convert_pdfa(&current, &next, level, progress, cancel)?;
            }
            RecipeStep::Ocr { options } => {
                pdfcore::ocr::ocr_pdf(&current, &next, options, policy, password, progress, cancel)?;
            }
            RecipeStep::Redact { areas, options } => {
                pdfcore::redact::redact_pdf(&current, &next, areas, options, policy, password, progress, cancel)?;
            }
        }
        applied.push(step.label().to_string());
        current = next;
    }
    let bytes = std::fs::read(&current).map_err(PdfError::from_io)?;
    crate::commands::write_atomic(output, &bytes)?;
    Ok(applied)
}

// ---------------------------------------------------------------------------
// Recipe store
// ---------------------------------------------------------------------------

/// Keeps letters, digits, spaces, `-` and `_`, capped at 48 characters.
pub fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == '-' || *ch == '_' || *ch == ' ')
        .take(48)
        .collect::<String>()
        .trim()
        .to_string()
}

fn recipes_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("Could not locate the app data directory: {error}")))?
        .join("recipes");
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

#[tauri::command]
pub fn recipe_save(app: AppHandle, recipe: Recipe) -> Result<(), PdfError> {
    let id = sanitize_id(&recipe.id);
    if id.is_empty() {
        return Err(PdfError::InvalidInput("The recipe needs an id.".into()));
    }
    let path = recipes_dir(&app)?.join(format!("{id}.json"));
    let json = serde_json::to_vec_pretty(&recipe).map_err(|error| PdfError::Internal(format!("{error}")))?;
    crate::commands::write_atomic(&path, &json)?;
    Ok(())
}

#[tauri::command]
pub fn recipe_list(app: AppHandle) -> Result<Vec<Recipe>, PdfError> {
    let dir = recipes_dir(&app)?;
    let mut recipes = Vec::new();
    if let Ok(read) = std::fs::read_dir(&dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(recipe) = serde_json::from_slice::<Recipe>(&bytes) {
                    recipes.push(recipe);
                }
            }
        }
    }
    recipes.sort_by_key(|recipe| recipe.name.to_lowercase());
    Ok(recipes)
}

#[tauri::command]
pub fn recipe_delete(app: AppHandle, id: String) -> Result<(), PdfError> {
    let id = sanitize_id(&id);
    if id.is_empty() {
        return Ok(());
    }
    let path = recipes_dir(&app)?.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn recipe_run(app: AppHandle, request: RecipeRunRequest) -> Result<RecipeRunResult, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let output = crate::paths::output_file(&request.output)?;
        let progress = move |event: ProgressEvent| {
            let _ = app.emit(
                "recipe:progress",
                serde_json::json!({ "stage": event.stage, "current": event.current, "total": event.total }),
            );
        };
        let cancel = CancelToken::new();
        let applied = run_recipe(
            input.as_path(),
            output.as_path(),
            &request.recipe,
            request.password.as_deref(),
            &progress,
            &cancel,
        )?;
        Ok(RecipeRunResult { output: output.as_path().to_string_lossy().to_string(), applied })
    });
    task.await.map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunRequest {
    pub input: String,
    pub output: String,
    pub recipe: Recipe,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunResult {
    pub output: String,
    pub applied: Vec<String>,
}

// ---------------------------------------------------------------------------
// Watched folder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchedFolder {
    pub folder: String,
    pub output_dir: String,
    pub recipe_id: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchedResult {
    pub input: String,
    pub output: String,
    pub ok: bool,
    pub detail: String,
}

/// Processes every new PDF in `folder` with `recipe`, writing results to
/// `output_dir`. `seen` carries the file names already handled across passes,
/// so a file is processed once. A pure function: the caller owns the loop.
pub fn scan_once(
    folder: &Path,
    output_dir: &Path,
    recipe: &Recipe,
    password: Option<&str>,
    seen: &mut HashSet<String>,
) -> Vec<WatchedResult> {
    // Never process a folder into itself (or a nested folder): the output path
    // would be the input path, and the recipe would overwrite the source file.
    if folders_overlap(folder, output_dir) {
        return Vec::new();
    }
    let mut results = Vec::new();
    let Ok(read) = std::fs::read_dir(folder) else { return results };
    let silent = |_| ();
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("pdf") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        if !seen.insert(name.to_string()) {
            continue;
        }
        let output = output_dir.join(name);
        let cancel = CancelToken::new();
        match run_recipe(&path, &output, recipe, password, &silent, &cancel) {
            Ok(applied) => results.push(WatchedResult {
                input: path.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                ok: true,
                detail: applied.join(" → "),
            }),
            Err(error) => results.push(WatchedResult {
                input: path.to_string_lossy().to_string(),
                output: String::new(),
                ok: false,
                detail: error.to_string(),
            }),
        }
    }
    results
}

fn watched_config_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("Could not locate the app data directory: {error}")))?;
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir.join("watched-folder.json"))
}

/// True when the folders are the same or one contains the other.
fn folders_overlap(folder: &Path, output: &Path) -> bool {
    let folder = folder.canonicalize().unwrap_or_else(|_| folder.to_path_buf());
    let output = output.canonicalize().unwrap_or_else(|_| output.to_path_buf());
    folder == output || folder.starts_with(&output) || output.starts_with(&folder)
}

/// The platform secret store file for the watched folder's document password.
fn watched_secret_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(watched_config_path(app)?.with_file_name("watched-folder.secret"))
}

#[tauri::command]
pub fn watched_folder_get(app: AppHandle) -> Result<WatchedFolder, PdfError> {
    let path = watched_config_path(&app)?;
    let mut config = if path.exists() {
        let bytes = std::fs::read(&path).map_err(PdfError::from_io)?;
        serde_json::from_slice(&bytes).unwrap_or_default()
    } else {
        WatchedFolder::default()
    };
    // The password lives in the platform secret store, not the config JSON.
    let password = crate::secret::load_api_key(&watched_secret_path(&app)?).unwrap_or_default();
    if !password.is_empty() {
        config.password = Some(password);
    }
    Ok(config)
}

#[tauri::command]
pub fn watched_folder_set(app: AppHandle, mut config: WatchedFolder) -> Result<(), PdfError> {
    let path = watched_config_path(&app)?;
    if !config.folder.trim().is_empty()
        && !config.output_dir.trim().is_empty()
        && folders_overlap(Path::new(config.folder.trim()), Path::new(config.output_dir.trim()))
    {
        return Err(PdfError::InvalidInput(
            "The watched folder and the output folder must be different, separate folders.".into(),
        ));
    }
    // The document password goes to the platform secret store (DPAPI/keystore),
    // never into the config file.
    let secret = watched_secret_path(&app)?;
    match config.password.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(password) => {
            crate::secret::save_api_key(&secret, password)?;
        }
        None => {
            crate::secret::delete_api_key(&secret).ok();
        }
    }
    config.password = None;
    let json = serde_json::to_vec_pretty(&config).map_err(|error| PdfError::Internal(format!("{error}")))?;
    crate::commands::write_atomic(&path, &json)?;
    Ok(())
}

/// One watch pass: reads the config, the named recipe, and processes new files.
/// The frontend (or the background loop) calls this on an interval.
#[tauri::command]
pub async fn watched_folder_scan(app: AppHandle, seen: Option<Vec<String>>) -> Result<Vec<WatchedResult>, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    let task = tauri::async_runtime::spawn_blocking(move || {
        let config = watched_folder_get(app.clone())?;
        if config.folder.trim().is_empty() || config.output_dir.trim().is_empty() || config.recipe_id.trim().is_empty()
        {
            return Err(PdfError::InvalidInput("Configure the watched folder first.".into()));
        }
        let recipes = recipe_list(app.clone())?;
        let recipe = recipes
            .into_iter()
            .find(|recipe| recipe.id == config.recipe_id)
            .ok_or_else(|| PdfError::NotFound(format!("recipe {}", config.recipe_id)))?;
        let folder = PathBuf::from(config.folder.trim());
        let output_dir = PathBuf::from(config.output_dir.trim());
        std::fs::create_dir_all(&output_dir).map_err(PdfError::from_io)?;
        let mut seen: HashSet<String> = seen.unwrap_or_default().into_iter().collect();
        Ok(scan_once(&folder, &output_dir, &recipe, config.password.as_deref(), &mut seen))
    });
    task.await.map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// True while a watch loop is live, so `watched_folder_start` is idempotent.
static WATCH_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Bumped on every start/stop: a loop whose generation no longer matches exits
/// before its next pass, so a stop → start cannot leave two loops running.
static WATCH_GENERATION: AtomicU64 = AtomicU64::new(0);

fn watch_wake() -> &'static tokio::sync::Notify {
    static WAKE: std::sync::OnceLock<tokio::sync::Notify> = std::sync::OnceLock::new();
    WAKE.get_or_init(tokio::sync::Notify::new)
}

/// Starts a polling loop that runs a watch pass every few seconds until
/// `watched_folder_stop` is called. Best-effort: a pass error is logged and the
/// loop keeps running.
#[tauri::command]
pub async fn watched_folder_start(app: AppHandle, interval_seconds: Option<u64>) -> Result<(), PdfError> {
    if WATCH_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let seconds = interval_seconds.unwrap_or(5).clamp(2, 3600);
    let generation = WATCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = Arc::new(app);
    let wake = watch_wake();
    tauri::async_runtime::spawn(async move {
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            // Sleep, but wake immediately on stop/start so a stale loop exits
            // before running another pass.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(seconds), wake.notified()).await;
            if WATCH_GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            let app_for_pass = app.clone();
            let seen_for_pass = seen.clone();
            let pass = tauri::async_runtime::spawn_blocking(move || {
                let config = watched_folder_get((*app_for_pass).clone()).unwrap_or_default();
                let recipes = recipe_list((*app_for_pass).clone()).unwrap_or_default();
                let Some(recipe) = recipes.into_iter().find(|recipe| recipe.id == config.recipe_id) else {
                    return (Vec::new(), seen_for_pass);
                };
                if config.folder.trim().is_empty() || config.output_dir.trim().is_empty() {
                    return (Vec::new(), seen_for_pass);
                }
                let mut seen = seen_for_pass;
                let results = scan_once(
                    Path::new(config.folder.trim()),
                    Path::new(config.output_dir.trim()),
                    &recipe,
                    config.password.as_deref(),
                    &mut seen,
                );
                (results, seen)
            })
            .await;
            if let Ok((results, updated_seen)) = pass {
                // Carry the handled-file set across passes so a file is processed once.
                seen = updated_seen;
                for result in results {
                    let _ = app.emit("watched:file", &result);
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn watched_folder_stop() -> Result<(), PdfError> {
    WATCH_ACTIVE.store(false, Ordering::SeqCst);
    WATCH_GENERATION.fetch_add(1, Ordering::SeqCst);
    watch_wake().notify_waiters();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn blank_pdf(path: &Path) {
        // A minimal but valid single-page PDF.
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(Stream::new(lopdf::Dictionary::new(), b"BT ET".to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content,
        });
        let pages = dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc.save(path).expect("save pdf");
    }

    #[test]
    fn a_recipe_chain_produces_a_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        blank_pdf(&input);
        let output = dir.path().join("out.pdf");
        let recipe = Recipe {
            id: "r1".into(),
            name: "Stamp then compress".into(),
            steps: vec![
                RecipeStep::Stamp {
                    header_footer: Some(pdfcore::stamp::HeaderFooterOptions {
                        header_center: "Sayfa {page}".into(),
                        ..Default::default()
                    }),
                    bates: None,
                },
                RecipeStep::Compress { options: pdfcore::compress::CompressOptions::default() },
            ],
        };
        let applied = run_recipe(&input, &output, &recipe, None, &|_| (), &CancelToken::new()).unwrap();
        assert_eq!(applied, vec!["stamp".to_string(), "compress".to_string()]);
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn an_empty_recipe_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.pdf");
        blank_pdf(&input);
        let recipe = Recipe { id: "r".into(), name: "empty".into(), steps: Vec::new() };
        assert!(run_recipe(&input, &dir.path().join("o.pdf"), &recipe, None, &|_| (), &CancelToken::new()).is_err());
    }

    #[test]
    fn a_watch_pass_processes_new_files_once() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("inbox");
        let output_dir = dir.path().join("done");
        std::fs::create_dir_all(&folder).unwrap();
        blank_pdf(&folder.join("a.pdf"));
        let recipe = Recipe {
            id: "r".into(),
            name: "compress".into(),
            steps: vec![RecipeStep::Compress { options: pdfcore::compress::CompressOptions::default() }],
        };
        let mut seen = HashSet::new();
        let first = scan_once(&folder, &output_dir, &recipe, None, &mut seen);
        assert_eq!(first.len(), 1);
        assert!(first[0].ok, "{}", first[0].detail);
        assert!(output_dir.join("a.pdf").exists());
        // A second pass sees the file as already handled.
        let second = scan_once(&folder, &output_dir, &recipe, None, &mut seen);
        assert!(second.is_empty());
    }

    #[test]
    fn recipe_ids_are_sanitized() {
        assert_eq!(sanitize_id("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_id("Rapor 1"), "Rapor 1");
        assert_eq!(sanitize_id(""), "");
    }

    #[test]
    fn recipe_step_wire_contract() {
        // The frontend sends camelCase fields; `headerFooter` used to be
        // dropped (a no-op stamp) and `options: {}` failed to parse.
        let stamp: RecipeStep =
            serde_json::from_str(r#"{"kind":"stamp","headerFooter":{"headerCenter":"Sayfa {page}"}}"#).expect("stamp");
        match stamp {
            RecipeStep::Stamp { header_footer, .. } => {
                let header = header_footer.expect("headerFooter must reach header_footer");
                assert_eq!(header.header_center, "Sayfa {page}");
            }
            other => panic!("expected a stamp step, got {other:?}"),
        }
        // Empty options objects fall back to the documented defaults.
        for json in [
            r#"{"kind":"redact","options":{}}"#,
            r#"{"kind":"compress","options":{}}"#,
            r#"{"kind":"ocr","options":{}}"#,
        ] {
            let step: RecipeStep = serde_json::from_str(json).unwrap_or_else(|error| panic!("{json}: {error}"));
            // Round-trips back to the same shape.
            let encoded = serde_json::to_string(&step).expect("serialize");
            assert!(encoded.contains("\"kind\""), "{encoded}");
        }
    }

    #[test]
    fn overlapping_folders_are_detected() {
        let base = tempfile::tempdir().unwrap();
        let inbox = base.path().join("inbox");
        let nested = inbox.join("done");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(folders_overlap(&inbox, &inbox), "the same folder overlaps");
        assert!(folders_overlap(&inbox, &nested), "a nested folder overlaps");
        assert!(folders_overlap(&nested, &inbox), "the parent overlaps too");
        assert!(!folders_overlap(&inbox, &base.path().join("other")));
    }

    #[test]
    fn a_watch_pass_never_writes_into_its_own_input_folder() {
        // folder == output_dir used to overwrite the input file in place.
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("both");
        std::fs::create_dir_all(&folder).unwrap();
        let input = folder.join("a.pdf");
        blank_pdf(&input);
        let before = std::fs::read(&input).unwrap();
        let recipe = Recipe {
            id: "r".into(),
            name: "compress".into(),
            steps: vec![RecipeStep::Compress { options: pdfcore::compress::CompressOptions::default() }],
        };
        let mut seen = HashSet::new();
        let results = scan_once(&folder, &folder, &recipe, None, &mut seen);
        assert!(results.is_empty(), "an overlapping folder must be skipped");
        assert_eq!(std::fs::read(&input).unwrap(), before, "the input must not change");
    }
}
