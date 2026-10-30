//! The IPC surface, split by domain (#338).
//!
//! Every `#[tauri::command]` is registered here, in exactly one domain list.
//! The `domain!` macro turns a list into the domain's command names and its
//! `generate_handler!`, so the two can never disagree, and `invoke_handler`
//! routes each call to the domain that owns its name. Command names are the
//! function names, unchanged: the frontend contract does not move.
//!
//! `src/lib/ipc-contract-coverage.test.ts` reads these lists and fails when a
//! declared command is not registered, or a registered name is not declared.

use std::collections::HashMap;

use crate::ai::edit as ai_edit;
#[cfg(target_os = "android")]
use crate::android_update;
use crate::{
    ai, android_intent, android_share, commands, diagnostics, evidence, file_dirs, files, jobs, launch, links, oauth,
    office, office_pdfa, office_tools, pdf_v3, person_cards, plugin, provenance, recent, recipes, sign, signatures,
    sync, update, vault,
};
use tauri::ipc::Invoke;
use tauri::Wry;

/// The handler of one domain: true when it answered the call.
type DomainHandler = Box<dyn Fn(Invoke<Wry>) -> bool + Send + Sync>;

macro_rules! domain {
    ($(#[$doc:meta])* $name:ident { $( $(#[$attr:meta])* $module:ident :: $command:ident ),* $(,)? }) => {
        $(#[$doc])*
        pub(crate) mod $name {
            #[allow(unused_imports)]
            use super::*;

            /// The command names this domain answers, as the frontend invokes them.
            // Pushes, not `vec![]`: an entry can carry a cfg attribute, which
            // an element of a `vec![]` literal cannot.
            #[allow(clippy::vec_init_then_push)]
            pub(crate) fn names() -> Vec<&'static str> {
                #[allow(unused_mut)]
                let mut names = Vec::new();
                $( $(#[$attr])* names.push(stringify!($command)); )*
                names
            }

            pub(crate) fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
                tauri::generate_handler![ $( $(#[$attr])* $module::$command ),* ]
            }
        }
    };
}

domain! {
    /// Shared app plumbing: settings, files and tokens, jobs, recent files, updates, diagnostics, platform bridges.
    app_commands {
        commands::app_info,
        update::update_check,
        update::update_open,
        #[cfg(target_os = "android")]
        android_update::update_download_apk,
        #[cfg(target_os = "android")]
        android_update::update_install_apk,
        links::open_external_link,
        diagnostics::diagnostics_report,
        commands::engine_status,
        commands::ocr_languages,
        files::file_token_pick,
        files::file_token_save,
        files::file_token_release,
        files::file_names,
        file_dirs::file_token_pick_directory,
        commands::cancel_job,
        jobs::jobs_list,
        jobs::jobs_clear_finished,
        jobs::jobs_register,
        jobs::jobs_progress,
        jobs::jobs_finish,
        jobs::jobs_retry,
        commands::load_settings,
        commands::save_settings,
        recent::load_recent,
        recent::add_recent,
        recent::clear_recent,
        commands::output_exists,
        commands::ensure_dir,
        commands::suggest_output,
        commands::file_sizes,
        commands::file_fingerprint,
        commands::open_document_file,
        commands::reveal_document_file,
        commands::dev_launch_context,
        android_intent::android_take_pending_open,
        android_intent::android_open_with_diagnostics,
        android_share::share_document_file,
        commands::log_operation,
        commands::load_operations,
        commands::clear_operations,
        commands::log_frontend,
        launch::office_take_launch_files,
        person_cards::person_card_list,
        person_cards::person_card_save,
        person_cards::person_card_delete,
    }
}

domain! {
    /// PDF tools: reading, page operations, conversion, security, redaction, the V3 studio, signing, recipes.
    pdf_commands {
        commands::pdf_info,
        commands::page_thumbnail,
        commands::page_preview,
        commands::page_tile,
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
        commands::pdf_list_annotations,
        commands::pdf_annotate_editable,
        commands::pdf_edit_annotations,
        commands::redact_pdf,
        commands::detect_sensitive_text,
        commands::redact_certificate_build,
        commands::redact_certificate_embed,
        commands::redact_certificate_verify,
        evidence::evidence_bundle_build,
        evidence::evidence_bundle_verify,
        evidence::evidence_match_originals,
        provenance::pdf_provenance_read,
        provenance::pdf_provenance_write,
        commands::detect_redaction_matches,
        commands::pdf_set_outline,
        commands::stamp_pdf,
        commands::nup_pdf,
        commands::compare_pdfs,
        commands::inspect_document,
        signatures::signature_save,
        signatures::signature_list,
        signatures::signature_delete,
        recipes::recipe_save,
        recipes::recipe_list,
        recipes::recipe_delete,
        recipes::recipe_run,
        recipes::watched_folder_get,
        recipes::watched_folder_set,
        recipes::watched_folder_scan,
        recipes::watched_folder_start,
        recipes::watched_folder_stop,
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
    }
}

domain! {
    /// The office suite: documents, workbooks, decks, invoices, and the office side tools.
    office_commands {
        office::office_startup_files,
        office::office_open_document,
        office::office_save_document,
        office::office_save_unit,
        office::office_export_pdf,
        office_pdfa::office_export_pdfa,
        office::office_convert,
        office::office_conversion_targets,
        office::office_open_invoice,
        office::office_open_invoice_package,
        office::office_invoice_summary,
        office::office_vat_rates,
        office::office_read_invoice_qr,
        office::office_open_eml,
        office::office_scan_pages,
        office::office_scan_preview,
        office::office_scan_detect,
        office::office_mail_merge,
        office::turkish_spellcheck,
        office::turkish_suggestions,
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
        office::recovery_save_blobs,
        office::recovery_gc_blobs,
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
    }
}

domain! {
    /// The optional AI assistant, its editor actions and the AI library.
    ai_commands {
        ai::ai_get_settings,
        ai::ai_save_settings,
        ai::ai_clear_key,
        ai::ai_test_connection,
        ai::ai_document_preview,
        ai::ai_summarize,
        ai::ai_translate,
        ai::ai_ask,
        ai::ai_chat,
        ai::ai_chat_save,
        ai::ai_chat_list,
        ai::ai_chat_load,
        ai::ai_chat_delete,
        ai::ai_clear_data,
        ai::ai_extract_table,
        ai::ai_batch_run,
        ai::ai_cleanup_text,
        ai::ai_suggest_metadata,
        ai::ai_cancel,
        ai_edit::ai_edit_text,
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
    }
}

domain! {
    /// The local document vault and its index.
    vault_commands {
        vault::vault_status,
        vault::vault_configure,
        vault::vault_scan,
        vault::vault_import_files,
        vault::vault_search,
        vault::vault_search_hybrid,
        vault::vault_index_build,
        vault::vault_document_text,
        vault::vault_clear,
    }
}

domain! {
    /// Folder sync with WebDAV and the OAuth providers.
    sync_commands {
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
    }
}

domain! {
    /// Third-party plugins and their sandboxed host calls.
    plugin_commands {
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
    }
}

/// Every domain's label and command names, in routing order (for the tests).
#[cfg(test)]
fn domain_names() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("app", app_commands::names()),
        ("pdf", pdf_commands::names()),
        ("office", office_commands::names()),
        ("ai", ai_commands::names()),
        ("vault", vault_commands::names()),
        ("sync", sync_commands::names()),
        ("plugin", plugin_commands::names()),
    ]
}

/// Every domain with its command names and handler. Only `invoke_handler`
/// builds these: a handler links every command of its domain.
fn domains() -> Vec<(&'static str, Vec<&'static str>, DomainHandler)> {
    vec![
        ("app", app_commands::names(), Box::new(app_commands::handler())),
        ("pdf", pdf_commands::names(), Box::new(pdf_commands::handler())),
        ("office", office_commands::names(), Box::new(office_commands::handler())),
        ("ai", ai_commands::names(), Box::new(ai_commands::handler())),
        ("vault", vault_commands::names(), Box::new(vault_commands::handler())),
        ("sync", sync_commands::names(), Box::new(sync_commands::handler())),
        ("plugin", plugin_commands::names(), Box::new(plugin_commands::handler())),
    ]
}

/// The app's invoke handler: the call goes to the domain that owns its name;
/// an unknown name answers false, as a single `generate_handler!` does.
pub(crate) fn invoke_handler() -> impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static {
    let mut handlers = Vec::new();
    let mut owner = HashMap::new();
    for (index, (_, names, handler)) in domains().into_iter().enumerate() {
        for name in names {
            owner.insert(name, index);
        }
        handlers.push(handler);
    }
    move |invoke| {
        let Some(&index) = owner.get(invoke.message.command()) else {
            return false;
        };
        handlers[index](invoke)
    }
}

// The tests read the names only. Building the handlers here would link every
// command into the lib test binary, and on Windows one of them needs
// comctl32 v6, which that binary has no manifest for: it would not start.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_order_matches_the_handlers() {
        // `domains()` must list the same domains in the same order as `domain_names()`.
        let source = include_str!("ipc.rs");
        let labels: Vec<&str> = domain_names().into_iter().map(|(label, _)| label).collect();
        for label in labels {
            let pattern = format!("(\"{label}\", {label}_commands::names(), Box::new({label}_commands::handler()))");
            assert!(source.contains(&pattern), "{label} is missing from domains()");
        }
    }

    #[test]
    fn every_command_name_belongs_to_exactly_one_domain() {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for (domain, names) in domain_names() {
            assert!(!names.is_empty(), "{domain} has no commands");
            for name in names {
                if let Some(other) = seen.insert(name, domain) {
                    panic!("{name} is registered in both {other} and {domain}");
                }
            }
        }
    }

    #[test]
    fn the_domains_cover_the_whole_surface() {
        let names: Vec<&str> = domain_names().into_iter().flat_map(|(_, names)| names).collect();
        // Desktop builds leave out the two Android-only update commands.
        let expected = if cfg!(target_os = "android") { TOTAL } else { TOTAL - 2 };
        assert_eq!(names.len(), expected);
    }

    /// The number of entries in the domain lists, Android-only ones included.
    const TOTAL: usize = 229;
}
