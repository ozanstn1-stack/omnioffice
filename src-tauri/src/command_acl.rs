//! The application command ACL contract (E1/4b).
//!
//! `build.rs` declares an ACL manifest from `src-tauri/permissions/`, and Tauri
//! then refuses any application command that no capability allows. The build
//! script always writes the two artifacts these tests read from its output
//! directory, so the check runs against the same data the binary compiles in:
//!
//! * `acl-manifests.json` - the parsed manifests, including the app manifest
//!   (`__app-acl__`) with one permission per IPC domain and the commands each
//!   one allows;
//! * `capabilities.json` - the capabilities parsed from
//!   `src-tauri/capabilities/`, with the permissions each grants and the windows
//!   they apply to.
//!
//! What this pins: the manifest is active, and every command the IPC registry
//! routes is granted to the `main` window by an unconditional capability. A
//! command that lands in a domain list without a permission - or a permission
//! no capability grants - would otherwise only fail at runtime, as a refused
//! invoke. The generated files themselves are checked by `npm run acl:check`,
//! and `e2e/smoke.mjs --acl` proves the runtime refusal.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

/// The app manifest key Tauri uses for application permissions.
const APP_MANIFEST: &str = "__app-acl__";
/// The window every capability in this app targets.
const MAIN_WINDOW: &str = "main";

fn artifact(name: &str) -> Value {
    let path = PathBuf::from(env!("OUT_DIR")).join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the build script wrote no {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display()))
}

/// Application permission identifier -> the command names it allows.
fn app_permissions() -> HashMap<String, Vec<String>> {
    let manifests = artifact("acl-manifests.json");
    let app = manifests
        .get(APP_MANIFEST)
        .unwrap_or_else(|| panic!("the app ACL manifest ({APP_MANIFEST}) is missing: no permission file was read"));
    let mut permissions = HashMap::new();
    if let Some(entries) = app.get("permissions").and_then(Value::as_object) {
        for (identifier, permission) in entries {
            let allowed = permission
                .get("commands")
                .and_then(|commands| commands.get("allow"))
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            permissions.insert(identifier.clone(), allowed);
        }
    }
    permissions
}

/// One capability that applies to every platform.
struct Capability {
    windows: Vec<String>,
    permissions: Vec<String>,
}

fn unconditional_capabilities() -> Vec<Capability> {
    let parsed = artifact("capabilities.json");
    let mut capabilities = Vec::new();
    for (_, capability) in parsed.as_object().into_iter().flatten() {
        // A capability with a `platforms` filter only covers those targets; the
        // ACL has to hold everywhere, so only unconditional ones count here.
        if capability.get("platforms").is_some_and(|platforms| !platforms.is_null()) {
            continue;
        }
        let windows = capability
            .get("windows")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_else(Vec::new);
        let mut permissions = Vec::new();
        if let Some(entries) = capability.get("permissions").and_then(Value::as_array) {
            for entry in entries {
                if let Some(identifier) = entry.as_str() {
                    permissions.push(identifier.to_string());
                } else if let Some(identifier) = entry.get("identifier").and_then(Value::as_str) {
                    permissions.push(identifier.to_string());
                }
            }
        }
        capabilities.push(Capability { windows, permissions });
    }
    capabilities
}

/// Every command the IPC registry routes on this target.
fn registered_commands() -> Vec<&'static str> {
    let mut names = Vec::new();
    names.extend(crate::ipc::app_commands::names());
    names.extend(crate::ipc::pdf_commands::names());
    names.extend(crate::ipc::office_commands::names());
    names.extend(crate::ipc::ai_commands::names());
    names.extend(crate::ipc::vault_commands::names());
    names.extend(crate::ipc::sync_commands::names());
    names.extend(crate::ipc::lan_commands::names());
    names.extend(crate::ipc::plugin_commands::names());
    names
}

#[test]
fn the_application_acl_is_active() {
    let permissions = app_permissions();
    assert_eq!(
        permissions.len(),
        8,
        "expected one app permission per IPC domain, found {:?}",
        permissions.keys().collect::<Vec<_>>()
    );
    for (identifier, commands) in &permissions {
        assert!(!commands.is_empty(), "{identifier} allows no command");
    }
}

#[test]
fn every_registered_command_is_granted_to_the_main_window() {
    let permissions = app_permissions();
    let capabilities = unconditional_capabilities();
    let granted: Vec<&String> = capabilities
        .iter()
        .filter(|capability| capability.windows.is_empty() || capability.windows.iter().any(|w| w == MAIN_WINDOW))
        .flat_map(|capability| capability.permissions.iter())
        .filter(|identifier| permissions.contains_key(*identifier))
        .collect();
    let allowed: std::collections::HashSet<&str> =
        granted.iter().flat_map(|identifier| permissions[*identifier].iter().map(String::as_str)).collect();
    assert!(!allowed.is_empty(), "no capability grants an application permission to {MAIN_WINDOW}");

    let mut missing: Vec<&str> = registered_commands().into_iter().filter(|name| !allowed.contains(name)).collect();
    missing.sort_unstable();
    assert!(
        missing.is_empty(),
        "these registered commands are not granted to the {MAIN_WINDOW} window (run npm run acl:sync): {missing:?}"
    );
}

#[test]
fn the_capability_grants_exactly_the_generated_permissions() {
    let permissions = app_permissions();
    let capabilities = unconditional_capabilities();
    let granted: std::collections::HashSet<&str> =
        capabilities.iter().flat_map(|capability| capability.permissions.iter()).map(String::as_str).collect();
    let mut orphaned: Vec<&str> = permissions.keys().map(String::as_str).filter(|id| !granted.contains(id)).collect();
    orphaned.sort_unstable();
    assert!(orphaned.is_empty(), "these app permissions are granted by no capability: {orphaned:?}");
}
