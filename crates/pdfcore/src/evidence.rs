//! Evidence bundles (C3, "delil klasörü").
//!
//! The exhibits are merged in the order given, behind a cover sheet and an
//! index. Every page gets a bundle-wide number; every exhibit page also gets
//! its exhibit label (`EK-2 · 1/3`). The index lists each exhibit's page range,
//! size and the SHA-256 of the file as it was submitted.
//!
//! A manifest with the same data is embedded in the bundle (an embedded JSON
//! file plus XMP, see [`crate::attachment`]). It records the digest of the
//! bundle with the manifest stripped, computed the way the redaction
//! certificate does it ("embed a probe, strip it, hash what remains"), so
//! reopening the bundle can tell whether it was changed, and an original file
//! can be matched to its exhibit by hash.

use std::collections::BTreeSet;

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::attachment::{self, RecordKind};
use crate::certificate::{digest, FileDigest};
use crate::docutil::{append_page_content, materialize_all_pages, page_mediabox, page_rotation, Matrix};
use crate::error::{PdfError, PdfResult};
use crate::merge::{merge_documents, MergeOptions};
use crate::textfont::{ensure_page_font, TextFont, RESOURCE_NAME};

/// The embedded manifest's file name inside the bundle.
pub const MANIFEST_FILE_NAME: &str = "omni-evidence-manifest.json";
/// XMP namespace that marks the manifest's metadata packet.
pub const MANIFEST_XMP_NAMESPACE: &str = "https://omnioffice.app/ns/evidence/1.0/";

const KIND: RecordKind = RecordKind {
    file_name: MANIFEST_FILE_NAME,
    description: "OmniOffice evidence bundle manifest",
    xmp_namespace: MANIFEST_XMP_NAMESPACE,
};

/// A4 portrait, in points.
const PAGE_W: f64 = 595.0;
const PAGE_H: f64 = 842.0;
const MARGIN: f64 = 56.0;
/// Index entries per index page (three lines each).
const ROWS_PER_INDEX_PAGE: usize = 15;

/// One exhibit as submitted: its label, file name and bytes.
pub struct EvidenceFile {
    pub label: String,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

/// What the cover sheet says about the bundle.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDetails {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub case_reference: String,
    #[serde(default)]
    pub court: String,
    #[serde(default)]
    pub submitted_by: String,
    /// "tr" or "en": the language of the cover, index and stamps.
    #[serde(default)]
    pub language: String,
}

/// One exhibit in the manifest.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceEntry {
    pub number: u32,
    pub label: String,
    pub file_name: String,
    pub sha256: String,
    pub size: u64,
    pub pages: u32,
    /// Bundle page numbers, 1-based and inclusive.
    pub first_page: u32,
    pub last_page: u32,
}

/// The manifest embedded in the bundle.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceManifest {
    pub version: u32,
    /// `EVD-` and the start of the digest over the exhibits' digests.
    pub id: String,
    /// RFC 3339 UTC.
    pub created_at: String,
    pub details: EvidenceDetails,
    pub entries: Vec<EvidenceEntry>,
    pub total_pages: u32,
    /// The bundle with the manifest stripped.
    pub bundle: FileDigest,
}

/// The result of checking a PDF for a manifest.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceVerification {
    pub present: bool,
    pub manifest: Option<EvidenceManifest>,
    /// True when the bundle, minus the manifest, still hashes to the recorded digest.
    pub content_matches: bool,
    pub actual_sha256: String,
}

/// The fixed words of the cover, index and stamps.
struct Words {
    default_title: &'static str,
    case_reference: &'static str,
    court: &'static str,
    submitted_by: &'static str,
    date: &'static str,
    exhibits: &'static str,
    pages: &'static str,
    bundle_id: &'static str,
    note: &'static str,
    index: &'static str,
    index_continued: &'static str,
    exhibit: &'static str,
    page_short: &'static str,
    bytes: &'static str,
    page_word: &'static str,
}

const TR: Words = Words {
    default_title: "Delil Klasörü",
    case_reference: "Dosya",
    court: "Mahkeme / merci",
    submitted_by: "Sunan",
    date: "Tarih",
    exhibits: "Ek sayısı",
    pages: "Toplam sayfa",
    bundle_id: "Klasör kimliği",
    note: "Her ekin SHA-256 özeti dizinde ve klasöre gömülü bildirimde yer alır. Klasörün değişip değişmediği ve bir dosyanın hangi eke karşılık geldiği OmniOffice > Delil klasörü > Doğrula ile denetlenebilir.",
    index: "Dizin",
    index_continued: "Dizin (devam)",
    exhibit: "EK",
    page_short: "s.",
    bytes: "bayt",
    page_word: "sayfa",
};

const EN: Words = Words {
    default_title: "Evidence Bundle",
    case_reference: "Case",
    court: "Court / authority",
    submitted_by: "Submitted by",
    date: "Date",
    exhibits: "Exhibits",
    pages: "Total pages",
    bundle_id: "Bundle ID",
    note: "The SHA-256 digest of every exhibit is listed in the index and in the manifest embedded in this bundle. OmniOffice > Evidence bundle > Verify checks whether the bundle was changed and which exhibit a file corresponds to.",
    index: "Index",
    index_continued: "Index (continued)",
    exhibit: "EX",
    page_short: "p.",
    bytes: "bytes",
    page_word: "pages",
};

fn words(language: &str) -> &'static Words {
    if language.eq_ignore_ascii_case("en") {
        &EN
    } else {
        &TR
    }
}

/// One drawn line: text, size, position and grey level.
struct Line {
    text: String,
    size: f64,
    x: f64,
    y: f64,
    grey: f64,
    /// Right-align at `x` instead of starting there.
    right: bool,
}

impl Line {
    fn at(text: impl Into<String>, size: f64, x: f64, y: f64) -> Self {
        Self { text: text.into(), size, x, y, grey: 0.1, right: false }
    }
    fn grey(mut self, grey: f64) -> Self {
        self.grey = grey;
        self
    }
    fn right(mut self) -> Self {
        self.right = true;
        self
    }
}

fn rfc3339_utc(now: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn cover_date(now: time::OffsetDateTime, language: &str) -> String {
    let (y, m, d) = (now.year(), u8::from(now.month()), now.day());
    if language.eq_ignore_ascii_case("en") {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{d:02}.{m:02}.{y:04}")
    }
}

fn page_range(words: &Words, entry: &EvidenceEntry) -> String {
    if entry.first_page == entry.last_page {
        format!("{} {}", words.page_short, entry.first_page)
    } else {
        format!("{} {}–{}", words.page_short, entry.first_page, entry.last_page)
    }
}

/// The cover sheet's lines.
fn cover_lines(manifest: &EvidenceManifest, date: &str) -> Vec<Line> {
    let details = &manifest.details;
    let words = words(&details.language);
    let title = if details.title.trim().is_empty() { words.default_title } else { details.title.trim() };
    let mut lines = vec![Line::at(title, 22.0, MARGIN, 760.0)];
    let rows = [
        (words.case_reference, details.case_reference.trim().to_string()),
        (words.court, details.court.trim().to_string()),
        (words.submitted_by, details.submitted_by.trim().to_string()),
        (words.date, date.to_string()),
        (words.exhibits, manifest.entries.len().to_string()),
        (words.pages, manifest.total_pages.to_string()),
        (words.bundle_id, manifest.id.clone()),
    ];
    let mut y = 700.0;
    for (label, value) in rows.into_iter().filter(|(_, value)| !value.is_empty()) {
        lines.push(Line::at(label, 11.0, MARGIN, y).grey(0.4));
        lines.push(Line::at(value, 11.0, MARGIN + 130.0, y));
        y -= 22.0;
    }
    lines
}

/// The index lines for entries `[start, start + ROWS_PER_INDEX_PAGE)`.
fn index_lines(manifest: &EvidenceManifest, page: usize) -> Vec<Line> {
    let words = words(&manifest.details.language);
    let heading = if page == 0 { words.index } else { words.index_continued };
    let mut lines = vec![Line::at(heading, 16.0, MARGIN, 780.0)];
    let mut y = 744.0;
    for entry in manifest.entries.iter().skip(page * ROWS_PER_INDEX_PAGE).take(ROWS_PER_INDEX_PAGE) {
        lines.push(Line::at(format!("{}-{}  {}", words.exhibit, entry.number, entry.label), 11.0, MARGIN, y));
        lines.push(Line::at(page_range(words, entry), 11.0, PAGE_W - MARGIN, y).right());
        lines.push(
            Line::at(
                format!("{} · {} {} · {} {}", entry.file_name, entry.size, words.bytes, entry.pages, words.page_word),
                8.5,
                MARGIN + 12.0,
                y - 13.0,
            )
            .grey(0.35),
        );
        lines.push(Line::at(format!("SHA-256 {}", entry.sha256), 7.5, MARGIN + 12.0, y - 24.0).grey(0.35));
        y -= 43.0;
    }
    lines
}

/// Greedy word wrap of `text` into lines no wider than `width`.
fn wrap(font: &TextFont, text: &str, size: f64, width: f64) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if font.measure(&candidate, size) > width && !current.is_empty() {
            out.push(std::mem::replace(&mut current, word.to_string()));
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `text` cut with an ellipsis so it fits `width`.
fn fit(font: &TextFont, text: &str, size: f64, width: f64) -> String {
    if font.measure(text, size) <= width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if font.measure(&candidate, size) <= width {
            return candidate;
        }
    }
    "…".into()
}

fn draw(font: &TextFont, lines: &[Line]) -> String {
    let mut out = String::new();
    for line in lines {
        let x = if line.right { line.x - font.measure(&line.text, line.size) } else { line.x };
        out.push_str(&format!(
            "BT\n{g:.2} {g:.2} {g:.2} rg\n/{RESOURCE_NAME} {size:.2} Tf\n{x:.2} {y:.2} Td\n{text} Tj\nET\n",
            g = line.grey,
            size = line.size,
            y = line.y,
            text = font.encode(&line.text),
        ));
    }
    out
}

/// A document of `count` blank A4 pages for the cover and the index.
fn blank_pages(count: u32) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for _ in 0..count {
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
            "Contents" => Object::Reference(content_id),
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => count as i64, "Kids" => kids }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

fn load_exhibit(file: &EvidenceFile) -> PdfResult<Document> {
    let doc = Document::load_mem(&file.bytes)
        .map_err(|error| PdfError::InvalidPdf(format!("{}: {error}", file.file_name)))?;
    if doc.is_encrypted() {
        return Err(PdfError::InvalidInput(format!("{}: the file is password protected", file.file_name)));
    }
    if doc.get_pages().is_empty() {
        return Err(PdfError::InvalidPdf(format!("{}: the file has no pages", file.file_name)));
    }
    Ok(doc)
}

/// Draws `lines` on a bundle page (a cover or index page, never rotated).
fn draw_front_page(doc: &mut Document, page_id: ObjectId, font: &TextFont, lines: &[Line]) -> PdfResult<()> {
    ensure_page_font(doc, page_id, font)?;
    append_page_content(doc, page_id, format!("q\n{}Q\n", draw(font, lines)).into_bytes())
}

/// Draws `lines` in the displayed (rotation-corrected) frame of an exhibit page.
fn stamp_page(
    doc: &mut Document,
    page_id: ObjectId,
    font: &TextFont,
    stamp: &dyn Fn(f64, f64) -> Vec<Line>,
) -> PdfResult<()> {
    let rotation = page_rotation(doc, page_id)?;
    let media = page_mediabox(doc, page_id)?;
    let (page_w, page_h) = (media[2] - media[0], media[3] - media[1]);
    let (display_w, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
    let to_page = Matrix::display_to_page(rotation, page_w, page_h);
    let lines = stamp(display_w, display_h);
    ensure_page_font(doc, page_id, font)?;
    // Draw relative to the MediaBox origin, which need not be (0, 0).
    let origin = Matrix::translate(media[0], media[1]);
    let content = format!("q\n{}\n{}\n{}Q\n", origin.to_cm(), to_page.to_cm(), draw(font, &lines));
    append_page_content(doc, page_id, content.into_bytes())
}

/// Builds the bundle and its manifest from the exhibits, in order.
pub fn build_evidence_bundle(
    files: Vec<EvidenceFile>,
    details: &EvidenceDetails,
) -> PdfResult<(Vec<u8>, EvidenceManifest)> {
    if files.is_empty() {
        return Err(PdfError::InvalidInput("add at least one exhibit".into()));
    }
    let mut exhibits = Vec::with_capacity(files.len());
    for file in &files {
        exhibits.push(load_exhibit(file)?);
    }

    let index_pages = files.len().div_ceil(ROWS_PER_INDEX_PAGE) as u32;
    let front_pages = 1 + index_pages;
    let mut next_page = front_pages + 1;
    let mut entries = Vec::with_capacity(files.len());
    for (index, (file, doc)) in files.iter().zip(&exhibits).enumerate() {
        let pages = doc.get_pages().len() as u32;
        let file_digest = digest(&file.bytes);
        entries.push(EvidenceEntry {
            number: index as u32 + 1,
            label: file.label.trim().to_string(),
            file_name: file.file_name.clone(),
            sha256: file_digest.sha256,
            size: file_digest.size,
            pages,
            first_page: next_page,
            last_page: next_page + pages - 1,
        });
        next_page += pages;
    }
    let total_pages = next_page - 1;

    let mut hasher = Sha256::new();
    for entry in &entries {
        hasher.update(entry.sha256.as_bytes());
        hasher.update(b"\n");
    }
    let id = format!("EVD-{}", format!("{:x}", hasher.finalize())[..12].to_uppercase());
    let now = time::OffsetDateTime::now_utc();
    let mut manifest = EvidenceManifest {
        version: 1,
        id,
        created_at: rfc3339_utc(now),
        details: details.clone(),
        entries,
        total_pages,
        bundle: digest(&[]),
    };

    // Lay the text out first: the subset font needs every character drawn.
    let words = words(&details.language);
    let width = PAGE_W - 2.0 * MARGIN;
    let mut front: Vec<Vec<Line>> = vec![cover_lines(&manifest, &cover_date(now, &details.language))];
    for page in 0..index_pages as usize {
        front.push(index_lines(&manifest, page));
    }
    let mut chars: BTreeSet<char> = "0123456789/ ·-–…".chars().collect();
    chars.extend(words.exhibit.chars());
    chars.extend(words.note.chars());
    for line in front.iter().flatten() {
        chars.extend(line.text.chars());
    }

    let mut sources = vec![("cover".to_string(), blank_pages(front_pages))];
    sources.extend(files.iter().map(|file| file.file_name.clone()).zip(exhibits));
    let mut doc = merge_documents(sources, &MergeOptions { preserve_metadata: false })?;
    materialize_all_pages(&mut doc)?;
    let font = TextFont::create(&mut doc, &chars)?;

    // Cut what does not fit; the note wraps at the foot of the cover.
    for lines in front.iter_mut() {
        for line in lines.iter_mut() {
            let available = if line.right { width } else { PAGE_W - MARGIN - line.x };
            line.text = fit(&font, &line.text, line.size, available);
        }
    }
    let mut y = 150.0;
    for text in wrap(&font, words.note, 9.0, width) {
        front[0].push(Line::at(text, 9.0, MARGIN, y).grey(0.4));
        y -= 13.0;
    }

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for (index, page_id) in page_ids.iter().enumerate() {
        let number = index as u32 + 1;
        let footer = format!("{number} / {total_pages}");
        if let Some(lines) = front.get_mut(index) {
            lines.push(
                Line::at(footer.clone(), 9.0, PAGE_W / 2.0 + font.measure(&footer, 9.0) / 2.0, 28.0).right().grey(0.4),
            );
            draw_front_page(&mut doc, *page_id, &font, lines)?;
            continue;
        }
        let entry = manifest
            .entries
            .iter()
            .find(|entry| (entry.first_page..=entry.last_page).contains(&number))
            .ok_or_else(|| PdfError::Internal("bundle page outside every exhibit".into()))?;
        let label = format!("{}-{} · {}/{}", words.exhibit, entry.number, number - entry.first_page + 1, entry.pages);
        let footer_width = font.measure(&footer, 9.0);
        stamp_page(&mut doc, *page_id, &font, &|w, h| {
            vec![
                Line::at(label.clone(), 9.0, w - 24.0, h - 22.0).right().grey(0.25),
                Line::at(footer.clone(), 9.0, (w + footer_width) / 2.0, 14.0).right().grey(0.25),
            ]
        })?;
    }

    let info = dictionary! {
        "Title" => crate::docutil::pdf_text_object(if details.title.trim().is_empty() { words.default_title } else { details.title.trim() }),
        "Subject" => crate::docutil::pdf_text_object(&manifest.id),
        "Producer" => crate::docutil::pdf_text_object("OmniOffice"),
    };
    let info_id = doc.add_object(info);
    doc.trailer.set("Info", Object::Reference(info_id));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;

    // The recorded digest is the bundle with the manifest stripped, which is
    // what verification recomputes (see the module docs).
    let probe = embed_manifest(&bytes, &manifest)?;
    manifest.bundle = digest(&attachment::strip_record(&probe, &KIND)?);
    let bundle = embed_manifest(&bytes, &manifest)?;
    Ok((bundle, manifest))
}

fn embed_manifest(pdf: &[u8], manifest: &EvidenceManifest) -> PdfResult<Vec<u8>> {
    let json =
        serde_json::to_vec_pretty(manifest).map_err(|error| PdfError::Internal(format!("manifest json: {error}")))?;
    let xmp = attachment::xmp_packet(
        MANIFEST_XMP_NAMESPACE,
        &[
            ("BundleId", manifest.id.clone()),
            ("BundleSha256", manifest.bundle.sha256.clone()),
            ("CreatedAt", manifest.created_at.clone()),
            ("ExhibitCount", manifest.entries.len().to_string()),
        ],
    );
    attachment::embed_record(pdf, &KIND, json, xmp)
}

/// Extracts and verifies the manifest, if the PDF carries one.
pub fn verify_evidence_bundle(pdf: &[u8]) -> PdfResult<EvidenceVerification> {
    let doc = Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    let Some(json) = attachment::read_record(&doc, &KIND) else {
        return Ok(EvidenceVerification {
            present: false,
            manifest: None,
            content_matches: false,
            actual_sha256: String::new(),
        });
    };
    drop(doc);
    let manifest: EvidenceManifest =
        serde_json::from_slice(&json).map_err(|error| PdfError::InvalidPdf(format!("manifest json: {error}")))?;
    let actual = digest(&attachment::strip_record(pdf, &KIND)?);
    Ok(EvidenceVerification {
        present: true,
        content_matches: actual.sha256 == manifest.bundle.sha256,
        manifest: Some(manifest),
        actual_sha256: actual.sha256,
    })
}

/// The exhibit number whose recorded digest matches `bytes`, if any.
pub fn match_original(manifest: &EvidenceManifest, bytes: &[u8]) -> Option<u32> {
    let sha256 = digest(bytes).sha256;
    manifest.entries.iter().find(|entry| entry.sha256 == sha256).map(|entry| entry.number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(language: &str, exhibits: u32) -> EvidenceManifest {
        EvidenceManifest {
            version: 1,
            id: "EVD-ABCDEF012345".into(),
            created_at: "2026-10-29T08:00:00Z".into(),
            details: EvidenceDetails {
                title: String::new(),
                case_reference: "2026/123 E.".into(),
                court: "İstanbul 3. Asliye Ticaret Mahkemesi".into(),
                submitted_by: String::new(),
                language: language.into(),
            },
            entries: (1..=exhibits)
                .map(|number| EvidenceEntry {
                    number,
                    label: format!("Şikâyet dilekçesi {number}"),
                    file_name: format!("ek-{number}.pdf"),
                    sha256: "a".repeat(64),
                    size: 1024,
                    pages: 2,
                    first_page: 2 * number + 1,
                    last_page: 2 * number + 2,
                })
                .collect(),
            total_pages: 2 + 2 * exhibits,
            bundle: digest(&[]),
        }
    }

    #[test]
    fn the_cover_skips_empty_fields_and_uses_the_default_title() {
        let texts: Vec<String> = cover_lines(&manifest("tr", 2), "29.10.2026").into_iter().map(|l| l.text).collect();
        assert_eq!(texts[0], "Delil Klasörü");
        assert!(texts.contains(&"İstanbul 3. Asliye Ticaret Mahkemesi".to_string()));
        assert!(texts.contains(&"EVD-ABCDEF012345".to_string()));
        assert!(!texts.contains(&"Sunan".to_string()), "an empty field has no row");
    }

    #[test]
    fn index_rows_carry_the_label_range_and_digest() {
        let texts: Vec<String> = index_lines(&manifest("tr", 2), 0).into_iter().map(|l| l.text).collect();
        assert_eq!(texts[0], "Dizin");
        assert!(texts.contains(&"EK-1  Şikâyet dilekçesi 1".to_string()));
        assert!(texts.contains(&"s. 3–4".to_string()));
        assert!(texts.contains(&format!("SHA-256 {}", "a".repeat(64))));
        assert!(texts.contains(&"ek-2.pdf · 1024 bayt · 2 sayfa".to_string()));
    }

    #[test]
    fn the_english_index_uses_english_words() {
        let texts: Vec<String> = index_lines(&manifest("en", 1), 0).into_iter().map(|l| l.text).collect();
        assert_eq!(texts[0], "Index");
        assert!(texts.contains(&"EX-1  Şikâyet dilekçesi 1".to_string()));
        assert!(texts.contains(&"p. 3–4".to_string()));
    }

    #[test]
    fn a_continued_index_page_starts_where_the_last_one_stopped() {
        let texts: Vec<String> = index_lines(&manifest("tr", 20), 1).into_iter().map(|l| l.text).collect();
        assert_eq!(texts[0], "Dizin (devam)");
        assert!(texts[1].starts_with("EK-16 "));
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis_and_the_note_wraps() {
        let mut doc = Document::with_version("1.7");
        let chars: BTreeSet<char> = (' '..='~').chain("…şğıİ".chars()).collect();
        let font = TextFont::create(&mut doc, &chars).unwrap();
        let cut = fit(&font, &"uzun ".repeat(60), 11.0, 200.0);
        assert!(cut.ends_with('…'));
        assert!(font.measure(&cut, 11.0) <= 200.0);
        let lines = wrap(&font, &"kelime ".repeat(80), 9.0, 300.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|line| font.measure(line, 9.0) <= 300.0));
    }
}
