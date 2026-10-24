//! B2/2 scaffold: exports a typed TypeScript binding for one command so the
//! tauri-specta pipeline (and its RC API) is exercised without touching the
//! 213 command registrations yet.

#[tauri::command]
#[specta::specta]
fn scaffold_ping() -> String {
    "pong".to_string()
}

fn main() {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![scaffold_ping])
        .export(
            specta_typescript::Typescript::default(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/generated/specta.ts"),
        )
        .expect("export the scaffold bindings");
}
