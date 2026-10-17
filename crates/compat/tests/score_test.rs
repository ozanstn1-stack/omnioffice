//! Runs the scorer over the committed corpus and golden PDFs.
//!
//! pdfium-backed: the test is skipped where the engine is unavailable (Linux
//! CI), and runs on Windows CI and locally after `npm run engines:fetch`.
//! A failure here means a sample no longer imports/round-trips or a page no
//! longer matches its reviewed golden render - review the golden with
//! `cargo run -p compat -- update-golden` before accepting the change.

#[test]
fn the_corpus_scores_above_the_threshold() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium yok; puanlama testi atlandı");
        return;
    }
    let report = compat::score(&compat::Options::default()).expect("puanlama çalışmalı");
    assert!(report.missing_goldens.is_empty(), "golden eksik: {:?}", report.missing_goldens);
    assert!(report.review.is_empty(), "inceleme listesi boş olmalı: {:?}", report.review_lines());
    for (format, score) in &report.formats {
        assert_eq!(score.structural, Some(100.0), "{format}: yapısal puan 100 olmalı (veri kaybı)");
        assert!(
            score.visual.map(|value| value >= 99.9).unwrap_or(false),
            "{format}: görsel puan {:.1} - golden'ı gözden geçir",
            score.visual.unwrap_or(0.0)
        );
    }
}
