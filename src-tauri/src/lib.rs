//! OmniOffice - Tauri application shell.
//!
//! The heavy lifting lives in `pdfcore`; this layer wires commands, events,
//! cancellation and engine discovery.

mod ai;
#[cfg(target_os = "android")]
mod android_background;
mod android_intent;
#[cfg(target_os = "android")]
mod android_keystore;
mod android_share;
// Android in-app update; host tests cover the URL/checksum helpers too.
#[cfg(any(target_os = "android", test))]
mod android_update;
#[cfg(any(target_os = "android", test))]
mod background_work;
pub mod cli;
mod commands;
mod concurrency;
mod crash_log;
mod diagnostics;
mod file_dirs;
mod files;
mod ipc;
mod jobs;
mod launch;
mod library;
mod links;
mod netpolicy;
mod oauth;
mod office;
mod office_tools;
mod paths;
mod paths_policy;
mod paths_read;
mod pdf_v3;
mod plugin;
mod recent;
mod recipes;
mod secret;
mod sign;
mod signatures;
mod sync;
mod update;
mod vault;
mod vault_index;
mod window_events;
#[cfg(test)]
mod wire_contract;
pub mod worker;

use jobs::{JobRegistry, JobStore};
use tauri::Manager;

/// Brings the main window to the front for a forwarded launch (it may be hidden).
#[cfg(desktop)]
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Single instance: a second launch never starts a second app; its arguments
/// are queued for the frontend (launch.rs) and the running window is focused.
#[cfg(desktop)]
fn single_instance_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Emitter;
    tauri_plugin_single_instance::init(|app, argv, cwd| {
        let files = launch::launch_files(argv, Some(std::path::Path::new(&cwd)));
        if !files.is_empty() {
            if let Some(queue) = app.try_state::<launch::LaunchQueue>() {
                queue.push(files);
            }
            let _ = app.emit("launch:files-queued", ());
        }
        focus_main_window(app);
    })
}

/// Hands the log directory to the panic hook before the first window exists.
fn crash_log_dir_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("omnioffice-crash-log")
        .setup(|app, _api| {
            if let Ok(dir) = app.path().app_log_dir() {
                crash_log::set_log_dir(dir);
            }
            Ok(())
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Release builds abort on panic: the panic hook writes crash.log first.
    crash_log::install_panic_hook();
    // Persistent job history shared by the registry and the jobs_* commands.
    let job_store = JobStore::shared();
    let builder = tauri::Builder::default();
    // The single-instance plugin has to come first: a second launch must be
    // detected (and exit) before any other plugin or the setup hook runs.
    #[cfg(desktop)]
    let builder = builder.plugin(single_instance_plugin());
    let builder = builder
        .plugin(crash_log_dir_plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_android_fs::init())
        .manage(job_store.clone())
        .manage(JobRegistry::new(job_store))
        .manage(launch::LaunchQueue::default());
    // Android Keystore bridge for the secret store (API key, OAuth tokens,
    // WebDAV password); see android_keystore.rs.
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_keystore::init());
    // Foreground service that keeps the process alive while jobs run in the
    // background; see android_background.rs.
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_background::init());
    // Android in-app update: downloads the release APK and opens the system
    // package installer; see android_update.rs.
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_update::init());
    // Android share sheet for app-private documents; see android_share.rs.
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_share::init());
    // Desktop auto-update plus the `relaunch` used after installing one; both
    // are desktop only (Android updates run through android_update.rs).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build()).plugin(tauri_plugin_process::init());

    builder
        .setup(|app| {
            // Android: keep a foreground service up for as long as any job runs,
            // so the system does not freeze the process when the app leaves the
            // screen.
            #[cfg(target_os = "android")]
            if let Some(registry) = app.try_state::<JobRegistry>() {
                registry.set_activity_listener(Box::new(android_background::set_active));
            }
            // Restore the persistent job history from <config>/jobs.json and
            // flip every job that was still running/queued when the previous
            // process died to `interrupted` (Android activity recreation or an
            // app restart). This only restores state; it is the foreground
            // service above that tries to keep the process - and therefore the
            // work - alive.
            if let Ok(config_dir) = app.path().app_config_dir() {
                if let Some(store) = app.try_state::<std::sync::Arc<JobStore>>() {
                    store.attach_path(config_dir.join("jobs.json"));
                }
                // A scan cannot survive restart, so a persisted `scanning:
                // true` (crash / Android process death) is stale and must not
                // leave the vault UI permanently disabled.
                let _ = vault::repair_stored_status(&config_dir);
            }
            // Teach pdfcore where the bundled engines live. Both layouts are
            // covered: <install>/resources/engines (bundler default) and
            // <exe dir>/engines (portable layout).
            if let Ok(resource_dir) = app.path().resource_dir() {
                for candidate in [resource_dir.join("resources").join("engines"), resource_dir.join("engines")] {
                    if candidate.exists() {
                        pdfcore::engines::set_engine_base(candidate);
                        break;
                    }
                }
            }
            // Android: engines ship inside the APK. The native libraries live
            // in the app's lib directory (found by pdfcore) and the OCR models
            // are copied to the private files directory by the Android shell.
            #[cfg(target_os = "android")]
            {
                if let Ok(data_dir) = app.path().app_config_dir() {
                    pdfcore::engines::set_files_dir(data_dir.join("files"));
                }
                if let Ok(cache_dir) = app.path().app_cache_dir() {
                    let tmp = cache_dir.join("tmp");
                    let _ = std::fs::create_dir_all(&tmp);
                    // Android has no /tmp; tesseract and the temp-file helpers
                    // both honor TMPDIR, and child processes inherit it.
                    std::env::set_var("TMPDIR", &tmp);
                }
            }
            // Development/screenshot hook: keep the window above others so
            // automated captures are deterministic. No effect in normal use.
            #[cfg(desktop)]
            if std::env::var("PDFSAK_ALWAYS_ON_TOP").is_ok() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_always_on_top(true);
                }
            }
            // Fit the default window to the actual monitor so the layout is
            // never larger than the screen (laptops, high-DPI displays).
            #[cfg(desktop)]
            if let Some(window) = app.get_webview_window("main") {
                let monitor =
                    window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten());
                if let Some(monitor) = monitor {
                    // Work in physical pixels: WebView2 applies the Windows
                    // DPI scale to the CSS viewport, so sizing in logical units
                    // can still overflow the screen on scaled displays.
                    let screen = monitor.size();
                    let target_width = (screen.width as f64 * 0.96).clamp(1100.0, 1780.0);
                    let target_height = (screen.height as f64 * 0.93).clamp(720.0, 1080.0);
                    let _ = window.set_size(tauri::PhysicalSize::new(target_width, target_height));
                    let _ = window.center();
                }
            }
            Ok(())
        })
        .on_window_event(window_events::handle)
        .invoke_handler(ipc::invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running OmniOffice");
}

/// Writes the typed command bindings (tauri-specta, #316) to `path`. The
/// command groups move one at a time (docs/dependencies.md); the vault is the
/// first. Until a group is listed here, its hand-written `invoke` stays.
#[cfg(feature = "bindings")]
pub fn export_typed_bindings(path: &std::path::Path) -> Result<(), String> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            vault::vault_status,
            vault::vault_configure,
            vault::vault_scan,
            vault::vault_import_files,
            vault::vault_search,
            vault::vault_search_hybrid,
            vault::vault_index_build,
            vault::vault_document_text,
            vault::vault_clear,
            sync::sync_get_config,
            sync::sync_save_config,
            sync::sync_test_connection,
            sync::sync_status,
            sync::sync_upload,
            sync::sync_download,
            sync::sync_list,
            sync::sync_resolve,
            sync::sync_forget,
            sync::sync_delete_remote,
            sync::sync_capabilities,
            oauth::oauth_status,
            oauth::oauth_save_client,
            oauth::oauth_connect,
            oauth::oauth_disconnect,
            plugin::plugin_list,
            plugin::plugin_read_source,
            plugin::plugin_install,
            plugin::plugin_install_from_dialog,
            plugin::plugin_install_sample,
            plugin::plugin_delete,
            plugin::plugin_file_read,
            plugin::plugin_file_write,
            plugin::plugin_pdf_read,
            plugin::plugin_http_request,
        ])
        .export(specta_typescript::Typescript::default(), path)
        .map_err(|error| error.to_string())
}
