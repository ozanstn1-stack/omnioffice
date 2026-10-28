//! Exports the typed command bindings (tauri-specta, #316) to
//! `src/lib/generated/specta.ts`. `npm run bindings:specta` runs it and
//! `npm run bindings:check` fails when the committed file drifts.

fn main() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/generated/specta.ts");
    pdf_sak_lib::export_typed_bindings(std::path::Path::new(path)).expect("export the typed bindings");
}
