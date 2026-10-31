//! Build script.
//!
//! Besides the usual Tauri setup (icons, the app manifest), this declares the
//! **application command ACL** (E1/4b): `AppManifest::new()` picks up the
//! permissions in `src-tauri/permissions/`, which `npm run acl:sync` generates
//! from the IPC domain lists. Tauri only enforces the ACL once an app manifest
//! exists, and from that moment a command the capability does not allow is
//! refused before it runs. `src-tauri/src/command_acl.rs` fails the test suite
//! when a registered command is not in the compiled allow list.

fn main() {
    let attributes = tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new());
    tauri_build::try_build(attributes).expect("tauri-build failed");
}
