//! Scores the compatibility corpus (`tests/corpus`, see `docs/corpus.md`).
//!
//! Two independent scores per sample:
//!
//! * **structural** - import -> export back to the same format -> re-import,
//!   then compare the flattened content (texts, cell values, slide texts).
//!   This is deterministic and catches data loss on the round trip.
//! * **visual** - import -> PDF (the `officecore::layout` engine) -> render
//!   every page with pdfium -> pixel comparison against a committed golden PDF
//!   (`tests/corpus/golden/`, the same pipeline's reviewed output). This
//!   catches layout regressions between releases.
//!
//! A third, informational score compares against **LibreOffice** reference
//! PDFs when a directory of them is provided (`--reference`): that is the only
//! external fidelity signal available, but it depends on the LibreOffice
//! version and is not calibrated yet, so it never drives the review list.
//!
//! Formats whose structural or visual score falls below the threshold (90 by
//! default, structural floor 99.5 because the round trip is deterministic)
//! land in `Report::review`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use officecore::model::{Block, Cell, CellValue, Deck, TextDocument, Workbook};
use pdfcore::compare::pixel_difference;
use pdfcore::render::{is_available, render_page, RenderOptions};

/// Default review threshold for the visual score, in percent.
pub const DEFAULT_THRESHOLD: f64 = 90.0;
/// The round trip is deterministic; anything below this is data loss.
pub const STRUCTURAL_FLOOR: f64 = 99.5;
/// Rendering DPI for the visual score. Low enough for speed, high enough that
/// text layout differences show up as changed pixels.
pub const DEFAULT_DPI: f32 = 96.0;
/// Pixels whose channel sum differs by more than this are "changed"; the same
/// idea as the PDF comparison's default tolerance (anti-aliased edges differ
/// by a few levels between renders).
const PIXEL_TOLERANCE: u8 = 12;

#[derive(Debug, Clone)]
pub struct Options {
    pub corpus: PathBuf,
    pub golden: PathBuf,
    pub reference: Option<PathBuf>,
    pub threshold: f64,
    pub dpi: f32,
}

impl Default for Options {
    fn default() -> Self {
        let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tests").join("corpus");
        Self { golden: corpus.join("golden"), corpus, reference: None, threshold: DEFAULT_THRESHOLD, dpi: DEFAULT_DPI }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SampleScore {
    pub file: String,
    pub format: String,
    /// 0..100; `None` when the sample could not be scored.
    pub structural: Option<f64>,
    /// 0..100 against the committed golden PDF.
    pub visual: Option<f64>,
    /// 0..100 against the LibreOffice reference, informational only.
    pub reference: Option<f64>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FormatScore {
    pub samples: usize,
    pub structural: Option<f64>,
    pub visual: Option<f64>,
    pub reference: Option<f64>,
    pub review: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    pub threshold: f64,
    pub structural_floor: f64,
    pub formats: BTreeMap<String, FormatScore>,
    pub samples: Vec<SampleScore>,
    pub review: Vec<String>,
    pub missing_goldens: Vec<String>,
}

impl Report {
    /// Formats on the review list, with the reason that put them there.
    pub fn review_lines(&self) -> Vec<String> {
        self.review
            .iter()
            .filter_map(|format| {
                let score = self.formats.get(format)?;
                let structural = score.structural.map(|value| format!("{value:.1}")).unwrap_or_else(|| "-".into());
                let visual = score.visual.map(|value| format!("{value:.1}")).unwrap_or_else(|| "-".into());
                Some(format!("{format}: structural {structural}%, visual {visual}%"))
            })
            .collect()
    }
}

fn manifest(corpus: &Path) -> Result<Vec<serde_json::Value>, String> {
    let path = corpus.join("manifest.json");
    let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    value.as_array().cloned().ok_or_else(|| format!("{}: bir dizi olmalı", path.display()))
}

enum Imported {
    Writer(TextDocument),
    Calc(Workbook),
    Impress(Deck),
}

fn import(reader: &str, bytes: &[u8]) -> Result<Imported, String> {
    match reader {
        "docx::read_docx" => {
            Ok(Imported::Writer(officecore::docx::read_docx(bytes).map_err(|e| e.to_string())?.document))
        }
        "odf::read_odt" => Ok(Imported::Writer(officecore::odf::read_odt(bytes).map_err(|e| e.to_string())?.document)),
        "xlsx::read_workbook_bytes" => {
            Ok(Imported::Calc(officecore::xlsx::read_workbook_bytes(bytes).map_err(|e| e.to_string())?.workbook))
        }
        "pptx::read_pptx" => Ok(Imported::Impress(officecore::pptx::read_pptx(bytes).map_err(|e| e.to_string())?.deck)),
        "udf::read_udf" => Ok(Imported::Writer(officecore::udf::read_udf(bytes).map_err(|e| e.to_string())?.document)),
        other => Err(format!("bilinmeyen okuyucu: {other}")),
    }
}

fn export(reader: &str, imported: &Imported) -> Result<Vec<u8>, String> {
    match (reader, imported) {
        ("docx::read_docx", Imported::Writer(document)) => {
            officecore::docx::write_docx(document).map_err(|e| e.to_string())
        }
        ("odf::read_odt", Imported::Writer(document)) => {
            officecore::odf::write_odt(document).map_err(|e| e.to_string())
        }
        ("xlsx::read_workbook_bytes", Imported::Calc(workbook)) => {
            officecore::xlsx::write_xlsx(workbook).map_err(|e| e.to_string())
        }
        ("pptx::read_pptx", Imported::Impress(deck)) => officecore::pptx::write_pptx(deck).map_err(|e| e.to_string()),
        ("udf::read_udf", Imported::Writer(document)) => {
            officecore::udf::write_udf(document).map_err(|e| e.to_string())
        }
        (reader, _) => Err(format!("{reader}: model türü eşleşmedi")),
    }
}

fn layout_pdf(imported: &Imported) -> Vec<u8> {
    match imported {
        Imported::Writer(document) => officecore::layout::document_to_pdf(document),
        Imported::Calc(workbook) => officecore::layout::workbook_to_pdf(workbook, 20),
        Imported::Impress(deck) => officecore::layout::deck_to_pdf(deck),
    }
}

fn block_fragments(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Paragraph { runs, .. } => {
                let text: String = runs.iter().map(|run| run.text.as_str()).collect();
                if !text.trim().is_empty() {
                    out.push(text);
                }
            }
            Block::Table { table } => {
                for row in &table.rows {
                    for cell in &row.cells {
                        block_fragments(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn cell_value(value: &CellValue) -> Option<String> {
    match value {
        CellValue::Empty => None,
        CellValue::Number(number) => Some(format!("{number}")),
        CellValue::Text(text) => Some(text.clone()),
        CellValue::Bool(flag) => Some(format!("{flag}")),
        CellValue::Error(error) => Some(format!("error:{error}")),
    }
}

fn cell_fragments(cell: &Cell) -> Vec<String> {
    let mut fragments = Vec::new();
    if let Some(value) = cell_value(&cell.value) {
        fragments.push(format!("value:{value}"));
    }
    if let Some(formula) = &cell.formula {
        fragments.push(format!("formula:{}", formula.trim_start_matches('=')));
    }
    fragments
}

fn workbook_fragments(workbook: &Workbook) -> Vec<String> {
    let mut fragments = Vec::new();
    for sheet in &workbook.sheets {
        for (address, cell) in &sheet.cells {
            for fragment in cell_fragments(cell) {
                fragments.push(format!("{}!{address} {fragment}", sheet.name));
            }
        }
    }
    fragments
}

fn deck_fragments(deck: &Deck) -> (Vec<String>, usize) {
    let mut fragments = Vec::new();
    for slide in &deck.slides {
        for object in &slide.objects {
            if let Some(frame) = &object.text {
                for paragraph in &frame.paragraphs {
                    if !paragraph.text.trim().is_empty() {
                        fragments.push(paragraph.text.clone());
                    }
                }
            }
        }
        if !slide.notes.trim().is_empty() {
            fragments.push(format!("notes:{}", slide.notes));
        }
    }
    (fragments, deck.slides.len())
}

fn multiset_similarity(left: &[String], right: &[String]) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    for item in left {
        *counts.entry(item.as_str()).or_default() += 1;
    }
    let mut matched = 0usize;
    for item in right {
        if let Some(count) = counts.get_mut(item.as_str()) {
            if *count > 0 {
                *count -= 1;
                matched += 1;
            }
        }
    }
    matched as f64 * 2.0 / (left.len() + right.len()) as f64
}

fn structural_score(reader: &str, first: &Imported, second: &Imported) -> f64 {
    match (reader, first, second) {
        (_, Imported::Writer(left), Imported::Writer(right)) => {
            let mut left_fragments = Vec::new();
            let mut right_fragments = Vec::new();
            block_fragments(&left.blocks, &mut left_fragments);
            block_fragments(&right.blocks, &mut right_fragments);
            multiset_similarity(&left_fragments, &right_fragments)
        }
        ("xlsx::read_workbook_bytes", Imported::Calc(left), Imported::Calc(right)) => {
            multiset_similarity(&workbook_fragments(left), &workbook_fragments(right))
        }
        ("pptx::read_pptx", Imported::Impress(left), Imported::Impress(right)) => {
            let (left_fragments, left_slides) = deck_fragments(left);
            let (right_fragments, right_slides) = deck_fragments(right);
            let base = multiset_similarity(&left_fragments, &right_fragments);
            let counts = if left_slides.max(right_slides) == 0 {
                1.0
            } else {
                left_slides.min(right_slides) as f64 / left_slides.max(right_slides) as f64
            };
            base * counts
        }
        _ => 0.0,
    }
}

fn render_pages(pdf: &Path, dpi: f32) -> Result<Vec<(u32, u32, Vec<u8>)>, String> {
    let options = RenderOptions { dpi, max_width: Some(2000), max_height: Some(2000) };
    let mut pages = Vec::new();
    for page in 1..=40u32 {
        match render_page(pdf, None, page, &options) {
            Ok(rendered) => pages.push((rendered.width, rendered.height, rendered.rgba)),
            Err(error) => {
                if page == 1 {
                    return Err(format!("{}: {error}", pdf.display()));
                }
                break;
            }
        }
    }
    Ok(pages)
}

fn visual_score(ours: &Path, reference: &Path, dpi: f32, notes: &mut Vec<String>) -> Result<f64, String> {
    let ours_pages = render_pages(ours, dpi)?;
    let reference_pages = render_pages(reference, dpi)?;
    if ours_pages.is_empty() || reference_pages.is_empty() {
        return Err(format!("{}: boş PDF", ours.display()));
    }
    if ours_pages.len() != reference_pages.len() {
        notes.push(format!("sayfa sayısı farklı: biz {} / referans {}", ours_pages.len(), reference_pages.len()));
    }
    let pages = ours_pages.len().max(reference_pages.len());
    let mut total = 0.0;
    for index in 0..pages {
        let Some((width, height, rgba)) = ours_pages.get(index) else {
            continue;
        };
        let Some((reference_width, reference_height, reference_rgba)) = reference_pages.get(index) else {
            continue;
        };
        if width != reference_width || height != reference_height {
            notes.push(format!(
                "sayfa {} boyutu farklı: {width}x{height} / {reference_width}x{reference_height}",
                index + 1
            ));
            continue;
        }
        let (fraction, _) = pixel_difference(rgba, reference_rgba, PIXEL_TOLERANCE);
        total += 1.0 - fraction;
    }
    Ok(total / pages as f64)
}

fn percent(value: f64) -> f64 {
    (value * 1000.0).round() / 10.0
}

/// Averages percentages (already in 0..100) and rounds to one decimal.
fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some((values.iter().sum::<f64>() / values.len() as f64 * 10.0).round() / 10.0)
    }
}
/// Writes the golden PDFs (`import -> layout`) for every manifest entry.
pub fn update_golden(options: &Options) -> Result<usize, String> {
    let entries = manifest(&options.corpus)?;
    std::fs::create_dir_all(&options.golden).map_err(|error| format!("{}: {error}", options.golden.display()))?;
    let mut written = 0;
    for entry in &entries {
        let file = entry["file"].as_str().ok_or("file alanı yok")?;
        let reader = entry["reader"].as_str().ok_or("reader alanı yok")?;
        let path = options.corpus.join(file);
        let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let imported = import(reader, &bytes).map_err(|error| format!("{file}: {error}"))?;
        let target = options.golden.join(format!("{file}.pdf"));
        std::fs::write(&target, layout_pdf(&imported)).map_err(|error| format!("{}: {error}", target.display()))?;
        written += 1;
    }
    Ok(written)
}

/// Scores every manifest entry and aggregates per format.
pub fn score(options: &Options) -> Result<Report, String> {
    if !is_available() {
        return Err("pdfium yok: görsel puanlama için motorlar gerekli (npm run engines:fetch)".into());
    }
    let entries = manifest(&options.corpus)?;
    let mut samples = Vec::new();
    let mut missing_goldens = Vec::new();
    for entry in &entries {
        let file = entry["file"].as_str().ok_or("file alanı yok")?.to_string();
        let format = entry["format"].as_str().ok_or("format alanı yok")?.to_string();
        let reader = entry["reader"].as_str().ok_or("reader alanı yok")?.to_string();
        let mut notes = Vec::new();
        let path = options.corpus.join(&file);
        let bytes = std::fs::read(&path).map_err(|error| format!("{file}: {error}"))?;

        let mut structural = None;
        let mut visual = None;
        let mut reference = None;

        match import(&reader, &bytes) {
            Err(error) => notes.push(format!("içe aktarılamadı: {error}")),
            Ok(first) => {
                match export(&reader, &first) {
                    Err(error) => notes.push(format!("dışa aktarılamadı: {error}")),
                    Ok(exported) => match import(&reader, &exported) {
                        Err(error) => notes.push(format!("geri okunamadı: {error}")),
                        Ok(second) => structural = Some(percent(structural_score(&reader, &first, &second))),
                    },
                }

                let stem = Path::new(&file).file_stem().map(|value| value.to_string_lossy().to_string());
                let Some(stem) = stem else {
                    notes.push("dosya adı çözümlenemedi".into());
                    samples.push(SampleScore { file, format, structural, visual, reference, notes });
                    continue;
                };

                let ours_pdf = std::env::temp_dir().join(format!("compat-ours-{stem}.pdf"));
                if let Err(error) = std::fs::write(&ours_pdf, layout_pdf(&first)) {
                    notes.push(format!("geçici PDF yazılamadı: {error}"));
                } else {
                    let golden = options.golden.join(format!("{file}.pdf"));
                    if golden.is_file() {
                        match visual_score(&ours_pdf, &golden, options.dpi, &mut notes) {
                            Ok(value) => visual = Some(percent(value)),
                            Err(error) => notes.push(format!("görsel karşılaştırma hatası: {error}")),
                        }
                    } else {
                        missing_goldens.push(file.clone());
                        notes.push(format!("golden eksik: {}", golden.display()));
                    }
                    if let Some(reference_dir) = &options.reference {
                        // LibreOffice exports spreadsheets portrait while the
                        // layout engine pages them landscape, and the two
                        // paginate differently; a pixel comparison would only
                        // measure that policy difference, not fidelity.
                        if format == "xlsx" {
                            notes.push("referans puanı yok: çizelge yönü/sayfalaması LibreOffice ile farklı".into());
                        } else {
                            let reference_pdf = reference_dir.join(format!("{file}.pdf"));
                            if reference_pdf.is_file() {
                                match visual_score(&ours_pdf, &reference_pdf, options.dpi, &mut notes) {
                                    Ok(value) => reference = Some(percent(value)),
                                    Err(error) => notes.push(format!("referans karşılaştırma hatası: {error}")),
                                }
                            }
                        }
                    }
                    let _ = std::fs::remove_file(&ours_pdf);
                }
            }
        }

        samples.push(SampleScore { file, format, structural, visual, reference, notes });
    }

    let mut formats: BTreeMap<String, FormatScore> = BTreeMap::new();
    for sample in &samples {
        let entry = formats.entry(sample.format.clone()).or_insert(FormatScore {
            samples: 0,
            structural: None,
            visual: None,
            reference: None,
            review: false,
        });
        entry.samples += 1;
    }
    for (format, entry) in formats.iter_mut() {
        let of_format: Vec<&SampleScore> = samples.iter().filter(|sample| &sample.format == format).collect();
        entry.structural = mean(&of_format.iter().filter_map(|sample| sample.structural).collect::<Vec<_>>());
        entry.visual = mean(&of_format.iter().filter_map(|sample| sample.visual).collect::<Vec<_>>());
        entry.reference = mean(&of_format.iter().filter_map(|sample| sample.reference).collect::<Vec<_>>());
        let structural_ok = entry.structural.map(|value| value >= STRUCTURAL_FLOOR).unwrap_or(false);
        let visual_ok = entry.visual.map(|value| value >= options.threshold).unwrap_or(false);
        entry.review = !structural_ok || !visual_ok;
    }

    let review = formats.iter().filter(|(_, entry)| entry.review).map(|(format, _)| format.clone()).collect();
    Ok(Report {
        threshold: options.threshold,
        structural_floor: STRUCTURAL_FLOOR,
        formats,
        samples,
        review,
        missing_goldens,
    })
}
