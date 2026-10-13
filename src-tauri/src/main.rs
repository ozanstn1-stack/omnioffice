#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--cli …` runs a batch operation and exits; otherwise the GUI starts.
    if let Some(code) = pdf_sak_lib::cli::run_if_present(std::env::args().collect()) {
        std::process::exit(code);
    }
    pdf_sak_lib::run();
}
