//! CLI for the corpus scorer.
//!
//!   compat score [--corpus DIR] [--golden DIR] [--reference DIR] [--out FILE] [--json]
//!   compat update-golden [--corpus DIR] [--golden DIR]
//!
//! `score` exits 1 when a format lands on the review list or a golden is
//! missing, 2 on a hard error, 0 otherwise.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "score".to_string());
    let mut options = compat::Options::default();
    let mut out: Option<PathBuf> = None;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--corpus" => options.corpus = PathBuf::from(args.next().expect("--corpus değeri")),
            "--golden" => options.golden = PathBuf::from(args.next().expect("--golden değeri")),
            "--reference" => options.reference = Some(PathBuf::from(args.next().expect("--reference değeri"))),
            "--threshold" => options.threshold = args.next().expect("--threshold değeri").parse().expect("sayı olmalı"),
            "--dpi" => options.dpi = args.next().expect("--dpi değeri").parse().expect("sayı olmalı"),
            "--out" => out = Some(PathBuf::from(args.next().expect("--out değeri"))),
            "--json" => json = true,
            other => {
                eprintln!("bilinmeyen argüman: {other}");
                std::process::exit(2);
            }
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
                    if let Err(error) = std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()) {
                        eprintln!("rapor yazılamadı: {error}");
                        std::process::exit(2);
                    }
                }
                if json {
                    println!("{}", serde_json::to_string(&report).unwrap());
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
