//! OmniOffice - Tauri application shell.
//!
//! The heavy lifting lives in the `pdfcore` crate; this layer only wires
//! commands, progress events, cancellation and engine discovery.

mod ai;
mod android_intent;
mod commands;
mod concurrency;
mod diagnostics;
mod jobs;
mod library;
mod oauth;
mod office;
mod office_tools;
mod paths;
mod pdf_v3;
mod plugin;
mod secret;
mod sign;
mod sync;
mod update;
mod vault;

use jobs::{JobRegistry, JobStore};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Persistent job history, shared by the registry and the jobs_* commands.
    // Managed as Arc<JobStore> so both sides see the same records.
    let job_store = JobStore::shared();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_android_fs::init())
        .manage(job_store.clone())
        .manage(JobRegistry::new(job_store));

    builder
        .setup(|app| {
            // Restore the persistent job history from <config>/jobs.json and
            // flip every job that was still running/queued when the previous
            // process died to `interrupted` (Android activity recreation or an
            // app restart). This only preserves state; it does not keep the
            // process - and therefore the work - alive.
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
        .on_window_event(|window, event| {
            // Cancelling every running job when the window closes keeps the
            // process from lingering on long operations.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(registry) = window.app_handle().try_state::<JobRegistry>() {
                    registry.cancel_all();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            update::update_check,
            update::update_open,
            diagnostics::diagnostics_report,
            commands::engine_status,
            commands::ocr_languages,
            commands::cancel_job,
            jobs::jobs_list,
            jobs::jobs_clear_finished,
            jobs::jobs_register,
            jobs::jobs_progress,
            jobs::jobs_finish,
            jobs::jobs_retry,
            commands::pdf_info,
            commands::page_thumbnail,
            commands::page_preview,
            commands::page_text,
            commands::search_document,
            commands::check_password,
            commands::merge_pdfs,
            commands::extract_pages,
            commands::delete_pages,
            commands::rotate_pages,
            commands::apply_page_plan,
            commands::split_pdf,
            commands::estimate_compression,
            commands::compress_pdf,
            commands::ocr_pdf,
            commands::protect_pdf,
            commands::unlock_pdf,
            commands::pdf_to_images,
            commands::images_to_pdf,
            commands::resize_pages,
            commands::crop_pages,
            commands::edit_metadata,
            commands::add_page_numbers,
            commands::watermark_pdf,
            commands::annotate_pdf,
            commands::redact_pdf,
            commands::detect_sensitive_text,
            commands::compare_pdfs,
            commands::inspect_document,
            commands::load_settings,
            commands::save_settings,
            commands::load_recent,
            commands::add_recent,
            commands::clear_recent,
            commands::output_exists,
            commands::ensure_dir,
            commands::suggest_output,
            commands::file_sizes,
            commands::file_fingerprint,
            commands::open_document_file,
            commands::reveal_document_file,
            commands::dev_launch_context,
            android_intent::android_take_pending_open,
            ai::ai_get_settings,
            ai::ai_save_settings,
            ai::ai_clear_key,
            ai::ai_test_connection,
            ai::ai_document_preview,
            ai::ai_summarize,
            ai::ai_translate,
            ai::ai_ask,
            ai::ai_cleanup_text,
            ai::ai_suggest_metadata,
            ai::ai_cancel,
            ai::ai_save_output,
            ai::ai_example_prompts,
            ai::ai_models,
            ai::ai_discover_models,
            commands::ai_library_save,
            commands::ai_library_list,
            commands::ai_library_text,
            commands::ai_library_delete,
            commands::ai_library_clear,
            commands::ai_library_export,
            commands::ai_library_default_dir,
            commands::log_operation,
            commands::load_operations,
            commands::clear_operations,
            commands::log_frontend,
            office::office_startup_files,
            office::office_open_document,
            office::office_save_document,
            office::office_save_unit,
            office::office_export_pdf,
            office::office_convert,
            office::office_conversion_targets,
            office::office_clean,
            office::office_image_footprint,
            office::store_load,
            office::store_save,
            office::store_clear,
            office::history_push,
            office::history_list,
            office::history_load,
            office::history_clear,
            office::recovery_save,
            office::recovery_list,
            office::recovery_load,
            office::recovery_discard,
            office::recovery_discard_all,
            office_tools::office_images_to_pdf,
            office_tools::office_pdf_to_images,
            office_tools::office_pdf_to_text,
            office_tools::office_pdf_add_form,
            office_tools::office_pdf_list_form,
            office::office_capabilities,
            office::office_model_capabilities,
            office::office_supported_extensions,
            office::office_compatibility,
            pdf_v3::sanitize_pdf,
            pdf_v3::pdf_repair,
            pdf_v3::pdf_linearize,
            pdf_v3::flatten_pdf,
            pdf_v3::pdfa_validate,
            pdf_v3::pdfa_convert,
            pdf_v3::pdf_list_form_fields,
            pdf_v3::pdf_fill_form,
            pdf_v3::pdf_validate_form,
            pdf_v3::pdf_list_objects,
            pdf_v3::pdf_edit_objects,
            pdf_v3::pdf_list_text_runs,
            pdf_v3::pdf_edit_text_runs,
            sign::pdf_archive_validation_data,
            sign::pdf_sign,
            sign::pdf_verify_signatures,
            sign::pdf_verify_signatures_online,
            sign::pdf_list_signing_certificates,
            vault::vault_status,
            vault::vault_configure,
            vault::vault_scan,
            vault::vault_import_files,
            vault::vault_search,
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
            plugin::plugin_http_request,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OmniOffice");
}
