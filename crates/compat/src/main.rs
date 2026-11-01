//! CLI for the corpus scorer.
//!
//!   compat score [--corpus DIR] [--golden DIR] [--reference DIR] [--out FILE] [--json]
//!   compat update-golden [--corpus DIR] [--golden DIR]
//!
//! `score` exits 1 when a format lands on the review list or a golden is
//! missing, 2 on a hard error, 0 otherwise.

// Refuse panicking shortcuts in production code (E1.7 ratchet), the same rule
// the library crates carry: a missing flag value is reported, not unwrapped.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "score".to_string());
    let mut options = compat::Options::default();
    let mut out: Option<PathBuf> = None;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--corpus" => options.corpus = PathBuf::from(flag_value(&mut args, "--corpus")),
            "--golden" => options.golden = PathBuf::from(flag_value(&mut args, "--golden")),
            "--reference" => options.reference = Some(PathBuf::from(flag_value(&mut args, "--reference"))),
            "--threshold" => options.threshold = flag_number(&mut args, "--threshold"),
            "--dpi" => options.dpi = flag_number(&mut args, "--dpi"),
            "--out" => out = Some(PathBuf::from(flag_value(&mut args, "--out"))),
            "--json" => json = true,
            other => fail(&format!("bilinmeyen argüman: {other}")),
        }
    }

    match command.as_str() {
        "update-golden" => match compat::update_golden(&options) {
            Ok(count) => println!("{count} golden PDF yazıldı: {}", options.golden.display()),
            Err(error) => {
                eprintln!("hata: {error}");
                std::process::exit(2);
            }
        },
        "score" => match compat::score(&options) {
            Ok(report) => {
                if let Some(path) = &out {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let text = match serde_json::to_string_pretty(&report) {
                        Ok(text) => text,
                        Err(error) => fail(&format!("rapor JSON'a çevrilemedi: {error}")),
                    };
                    if let Err(error) = std::fs::write(path, text) {
                        eprintln!("rapor yazılamadı: {error}");
                        std::process::exit(2);
                    }
                }
                if json {
                    match serde_json::to_string(&report) {
                        Ok(text) => println!("{text}"),
                        Err(error) => fail(&format!("rapor JSON'a çevrilemedi: {error}")),
                    }
                } else {
                    println!("{:<8} {:>10} {:>10} {:>10} {:>6}", "biçim", "yapısal", "görsel", "referans", "örnek");
                    for (format, entry) in &report.formats {
                        let display =
                            |value: Option<f64>| value.map(|v| format!("{v:.1}%")).unwrap_or_else(|| "-".into());
                        println!(
                            "{:<8} {:>10} {:>10} {:>10} {:>6}",
                            format,
                            display(entry.structural),
                            display(entry.visual),
                            display(entry.reference),
                            entry.samples
                        );
                    }
                    for line in report.review_lines() {
                        println!("inceleme: {line}");
                    }
                    for file in &report.missing_goldens {
                        println!("golden eksik: {file}");
                    }
                }
                if !report.review.is_empty() || !report.missing_goldens.is_empty() {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("hata: {error}");
                std::process::exit(2);
            }
        },
        other => {
            eprintln!("bilinmeyen komut: {other} (score | update-golden)");
            std::process::exit(2);
        }
    }
}

/// The value that follows a flag, or exit 2 naming the flag.
fn flag_value(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    match args.next() {
        Some(value) => value,
        None => fail(&format!("{flag} için değer yok")),
    }
}

/// The numeric value that follows a flag, or exit 2 explaining what was wrong.
fn flag_number<T: std::str::FromStr>(args: &mut impl Iterator<Item = String>, flag: &str) -> T {
    let value = flag_value(args, flag);
    match value.parse() {
        Ok(number) => number,
        Err(_) => fail(&format!("{flag} sayı olmalı: {value}")),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("hata: {message}");
    std::process::exit(2);
}
