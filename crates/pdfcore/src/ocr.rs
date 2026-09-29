//! OCR pipeline built on the bundled Tesseract engine.
//!
//! Per page: render with pdfium -> optional preprocessing (auto-orientation,
//! deskew, contrast, denoise, binarize) -> tesseract -> assemble.
//! Searchable PDFs are produced by merging per-page tesseract PDFs (which
//! carry a hidden text layer) with untouched original pages for any page the
//! user did not select.

use crate::docutil::*;
use crate::engines;
use crate::error::{PdfError, PdfResult};
use crate::organize;
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use crate::render::{self, RenderOptions};
use image::{DynamicImage, GrayImage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrPreprocess {
    #[serde(default)]
    pub auto_rotate: bool,
    #[serde(default)]
    pub deskew: bool,
    #[serde(default)]
    pub contrast: bool,
    #[serde(default)]
    pub denoise: bool,
    #[serde(default)]
    pub binarize: bool,
    #[serde(default)]
    pub grayscale: bool,
}

impl Default for OcrPreprocess {
    fn default() -> Self {
        Self {
            auto_rotate: false,
            deskew: false,
            contrast: true,
            denoise: false,
            binarize: false,
            grayscale: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrOptions {
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    #[serde(default = "default_psm")]
    pub psm: u32,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// searchable_pdf | text | markdown
    #[serde(default = "default_output_mode")]
    pub output_mode: String,
    /// Empty = all pages.
    #[serde(default)]
    pub pages: Vec<u32>,
    #[serde(default)]
    pub preprocess: OcrPreprocess,
    /// Skip OCR on pages that already contain a text layer.
    #[serde(default)]
    pub skip_text_pages: bool,
}

fn default_languages() -> Vec<String> {
    vec!["eng".into()]
}
fn default_psm() -> u32 {
    3
}
fn default_dpi() -> u32 {
    300
}
fn default_output_mode() -> String {
    "searchable_pdf".into()
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            languages: default_languages(),
            psm: default_psm(),
            dpi: default_dpi(),
            output_mode: default_output_mode(),
            pages: Vec::new(),
            preprocess: OcrPreprocess::default(),
            skip_text_pages: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OcrResult {
    pub path: String,
    pub pages_processed: u32,
    pub pages_skipped: u32,
    pub characters: u64,
    pub duration_ms: u64,
    pub engine: String,
    pub languages: Vec<String>,
    /// Preprocessing steps that were actually applied, e.g. `contrast`,
    /// `denoise`, `auto_rotate`, `rotate:90`.
    #[serde(rename = "preprocessApplied")]
    pub preprocess_applied: Vec<String>,
}

pub fn tesseract_available() -> bool {
    engines::tesseract_path().is_some() && engines::tessdata_dir().is_some()
}

fn validate_languages(languages: &[String]) -> PdfResult<Vec<String>> {
    let available = engines::ocr_languages();
    let codes: Vec<String> = available.iter().map(|l| l.code.clone()).collect();
    let mut chosen = Vec::new();
    for lang in languages {
        let normalized = lang.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if !codes.contains(&normalized) {
            return Err(PdfError::InvalidInput(format!(
                "OCR language '{normalized}' is not installed (available: {})",
                codes.join(", ")
            )));
        }
        chosen.push(normalized);
    }
    if chosen.is_empty() {
        return Err(PdfError::InvalidInput("select at least one OCR language".into()));
    }
    Ok(chosen)
}

/// Runs tesseract, respecting cancellation (the child process is killed).
/// Stderr is captured to a temp file so failures can be reported with the
/// engine's own diagnostic line (never document content).
fn run_tesseract(args: &[String], cancel: &CancelToken) -> PdfResult<()> {
    let mut cmd = engines::tesseract_command().ok_or(PdfError::OcrEngineUnavailable)?;
    let log_path = std::env::temp_dir().join(format!(
        "pdfsak-tesseract-{}-{:x}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(match std::fs::File::create(&log_path) {
            Ok(file) => Stdio::from(file),
            Err(_) => Stdio::null(),
        });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| PdfError::ProcessingFailed(format!("could not start OCR engine: {e}")))?;
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&log_path);
            return Err(PdfError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    let _ = std::fs::remove_file(&log_path);
                    return Ok(());
                }
                let diagnostic = std::fs::read_to_string(&log_path)
                    .ok()
                    .and_then(|text| {
                        text.lines()
                            .rev()
                            .find(|line| !line.trim().is_empty())
                            .map(|line| line.trim().chars().take(180).collect::<String>())
                    })
                    .unwrap_or_default();
                let _ = std::fs::remove_file(&log_path);
                let env_hint = std::env::var("TESSDATA_PREFIX").unwrap_or_else(|_| "<unset>".into());
                let arg_hint = args.join(" ");
                return Err(PdfError::ProcessingFailed(format!(
                    "OCR engine exited with code {}{} [TESSDATA_PREFIX={env_hint}] [args={arg_hint}]",
                    status.code().unwrap_or(-1),
                    if diagnostic.is_empty() {
                        String::new()
                    } else {
                        format!(": {diagnostic}")
                    }
                )));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(40)),
            Err(e) => {
                let _ = std::fs::remove_file(&log_path);
                return Err(PdfError::from_io(e));
            }
        }
    }
}

/// Detects the page orientation with tesseract OSD. Returns the clockwise
/// rotation in degrees that must be applied to make the page upright.
fn detect_orientation(path: &Path) -> Option<i32> {
    let mut cmd = engines::tesseract_command()?;
    cmd.args([
        path.to_string_lossy().to_string(),
        "stdout".into(),
        "--psm".into(),
        "0".into(),
    ])
    .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Rotate:") {
            if let Ok(degrees) = value.trim().parse::<i32>() {
                return Some(((degrees % 360) + 360) % 360);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Image preprocessing (pure Rust, no external dependencies)
// ---------------------------------------------------------------------------

fn to_gray(image: &DynamicImage) -> GrayImage {
    image.to_luma8()
}

/// Percentile contrast stretch (1%..99%). Skips the stretch when the
/// histogram is nearly uniform (pure white scans, noisy paper) because
/// amplifying a tiny range would turn sensor noise into fake ink.
fn stretch_contrast(gray: &GrayImage) -> GrayImage {
    let mut histogram = [0u64; 256];
    for px in gray.pixels() {
        histogram[px[0] as usize] += 1;
    }
    let total: u64 = gray.width() as u64 * gray.height() as u64;
    let low_threshold = total / 100;
    let high_threshold = total - total / 100;
    let mut acc = 0u64;
    let mut low = 0u8;
    let mut high = 255u8;
    for (value, count) in histogram.iter().enumerate() {
        acc += count;
        if acc >= low_threshold {
            low = value as u8;
            break;
        }
    }
    acc = 0;
    for (value, count) in histogram.iter().enumerate() {
        acc += count;
        if acc >= high_threshold {
            high = value as u8;
            break;
        }
    }
    if high <= low || (high as i32 - low as i32) < 24 {
        // Too flat to stretch safely: keep the image as-is.
        return gray.clone();
    }
    let scale = 255.0 / (high as f32 - low as f32);
    let mut out = GrayImage::new(gray.width(), gray.height());
    for (x, y, px) in gray.enumerate_pixels() {
        let value = ((px[0].saturating_sub(low)) as f32 * scale).clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, image::Luma([value]));
    }
    out
}

/// Otsu threshold binarization.
fn binarize_otsu(gray: &GrayImage) -> GrayImage {
    let mut histogram = [0u64; 256];
    for px in gray.pixels() {
        histogram[px[0] as usize] += 1;
    }
    let total: u64 = gray.width() as u64 * gray.height() as u64;
    let sum_all: f64 = histogram
        .iter()
        .enumerate()
        .map(|(i, c)| i as f64 * *c as f64)
        .sum();
    let mut sum_background = 0.0;
    let mut weight_background = 0u64;
    let mut best_threshold = 127u8;
    let mut best_variance = -1.0f64;
    for threshold in 0..256u32 {
        weight_background += histogram[threshold as usize];
        if weight_background == 0 {
            continue;
        }
        let weight_foreground = total - weight_background;
        if weight_foreground == 0 {
            break;
        }
        sum_background += threshold as f64 * histogram[threshold as usize] as f64;
        let mean_background = sum_background / weight_background as f64;
        let mean_foreground = (sum_all - sum_background) / weight_foreground as f64;
        let variance = weight_background as f64
            * weight_foreground as f64
            * (mean_background - mean_foreground).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = threshold as u8;
        }
    }
    let mut out = GrayImage::new(gray.width(), gray.height());
    for (x, y, px) in gray.enumerate_pixels() {
        let value = if px[0] > best_threshold { 255 } else { 0 };
        out.put_pixel(x, y, image::Luma([value]));
    }
    out
}

/// 3x3 median denoise.
fn median_denoise(gray: &GrayImage) -> GrayImage {
    let (w, h) = (gray.width(), gray.height());
    let mut out = GrayImage::new(w, h);
    let mut window = [0u8; 9];
    for y in 0..h {
        for x in 0..w {
            let mut count = 0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let sx = x as i64 + dx;
                    let sy = y as i64 + dy;
                    if sx >= 0 && sy >= 0 && (sx as u32) < w && (sy as u32) < h {
                        window[count] = gray.get_pixel(sx as u32, sy as u32)[0];
                        count += 1;
                    }
                }
            }
            let slice = &mut window[..count];
            slice.sort_unstable();
            out.put_pixel(x, y, image::Luma([slice[count / 2]]));
        }
    }
    out
}

/// Estimates the skew angle in degrees (positive = rotate counter-clockwise
/// to correct) using a projection profile variance search.
fn estimate_skew_angle(gray: &GrayImage) -> Option<f64> {
    // Downscale for speed.
    let max_dim = 700u32;
    let scale = (max_dim as f64 / gray.width().max(1) as f64).min(1.0);
    if scale < 1.0 {
        let nw = (gray.width() as f64 * scale).max(8.0) as u32;
        let nh = (gray.height() as f64 * scale).max(8.0) as u32;
        let small = image::imageops::resize(gray, nw, nh, image::imageops::FilterType::Triangle);
        estimate_skew_angle_inner(&small)
    } else {
        estimate_skew_angle_inner(gray)
    }
}

fn estimate_skew_angle_inner(gray: &GrayImage) -> Option<f64> {
    let (w, h) = (gray.width(), gray.height());
    if w < 16 || h < 16 {
        return None;
    }
    // Sample dark pixels sparsely.
    let mut points: Vec<(f64, f64)> = Vec::new();
    let step = 2;
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            if gray.get_pixel(x, y)[0] < 140 {
                points.push((x as f64, y as f64));
            }
            x += step;
        }
        y += step;
    }
    if points.len() < 200 {
        return None;
    }
    let mut best_angle = 0.0f64;
    let mut best_score = -1.0f64;
    let mut angle = -4.0f64;
    while angle <= 4.0 {
        let radians = angle.to_radians();
        let (sin, cos) = radians.sin_cos();
        let mut rows = vec![0u32; h as usize];
        for (px, py) in &points {
            let rotated = py * cos - px * sin;
            let row = rotated.round() as i64;
            if row >= 0 && (row as usize) < rows.len() {
                rows[row as usize] += 1;
            }
        }
        let mean = rows.iter().map(|v| *v as f64).sum::<f64>() / rows.len() as f64;
        let variance = rows
            .iter()
            .map(|v| (*v as f64 - mean).powi(2))
            .sum::<f64>()
            / rows.len() as f64;
        if variance > best_score {
            best_score = variance;
            best_angle = angle;
        }
        angle += 0.25;
    }
    if best_angle.abs() < 0.25 {
        None
    } else {
        Some(best_angle)
    }
}

/// Rotates an RGBA image by an arbitrary angle (bilinear sampling).
fn rotate_bilinear(image: &image::RgbaImage, degrees: f64) -> image::RgbaImage {
    if degrees.abs() < 0.01 {
        return image.clone();
    }
    let (w, h) = (image.width(), image.height());
    let radians = (-degrees).to_radians();
    let (sin, cos) = radians.sin_cos();
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let sx = cos * dx - sin * dy + cx;
            let sy = sin * dx + cos * dy + cy;
            let pixel = if sx >= 0.0 && sy >= 0.0 && sx < (w - 1) as f64 && sy < (h - 1) as f64 {
                let x0 = sx.floor() as u32;
                let y0 = sy.floor() as u32;
                let fx = sx - x0 as f64;
                let fy = sy - y0 as f64;
                let p00 = image.get_pixel(x0, y0);
                let p10 = image.get_pixel(x0 + 1, y0);
                let p01 = image.get_pixel(x0, y0 + 1);
                let p11 = image.get_pixel(x0 + 1, y0 + 1);
                let mut channels = [0u8; 4];
                for c in 0..4 {
                    let top = p00[c] as f64 * (1.0 - fx) + p10[c] as f64 * fx;
                    let bottom = p01[c] as f64 * (1.0 - fx) + p11[c] as f64 * fx;
                    channels[c] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
                }
                image::Rgba(channels)
            } else {
                image::Rgba([255, 255, 255, 255])
            };
            out.put_pixel(x, y, pixel);
        }
    }
    out
}

/// Applies rotation by multiples of 90 degrees, clockwise.
pub fn rotate_quarters(image: &image::RgbaImage, degrees: i32) -> image::RgbaImage {
    match degrees.rem_euclid(360) {
        90 => image::imageops::rotate90(image),
        180 => image::imageops::rotate180(image),
        270 => image::imageops::rotate270(image),
        _ => image.clone(),
    }
}

/// Applies the pixel filters, writes the result to `rendered_path`, and - when
/// `auto_rotate` is on - detects the orientation on that written raster and
/// re-writes it after applying the detected quarter turns.
///
/// Orientation detection must run against a raster file: tesseract cannot open
/// a PDF, so handing it the source path silently disabled auto-rotation. The
/// file is written first precisely so OSD sees what the OCR engine will read.
pub fn preprocess(
    image: DynamicImage,
    options: &OcrPreprocess,
    rendered_path: &Path,
    cancel: &CancelToken,
) -> PdfResult<(DynamicImage, i32)> {
    let filtered = apply_pixel_filters(image, options, cancel)?;
    filtered
        .save_with_format(rendered_path, image::ImageFormat::Png)
        .map_err(|e| PdfError::ConversionFailed(format!("page image write failed: {e}")))?;
    if !options.auto_rotate {
        return Ok((filtered, 0));
    }
    let Some(degrees) = detect_orientation(rendered_path) else {
        return Ok((filtered, 0));
    };
    if degrees == 0 {
        return Ok((filtered, 0));
    }
    let rotated = DynamicImage::ImageRgba8(rotate_quarters(&filtered.to_rgba8(), degrees));
    rotated
        .save_with_format(rendered_path, image::ImageFormat::Png)
        .map_err(|e| PdfError::ConversionFailed(format!("rotated page image write failed: {e}")))?;
    Ok((rotated, degrees))
}

/// Records the preprocessing steps that ran, for the OCR result.
pub fn describe_preprocessing(options: &OcrPreprocess, rotation: i32, out: &mut Vec<String>) {
    let steps = [
        (options.grayscale, "grayscale"),
        (options.denoise, "denoise"),
        (options.contrast, "contrast"),
        (options.binarize, "binarize"),
        (options.deskew, "deskew"),
        (options.auto_rotate, "auto_rotate"),
    ];
    for (enabled, name) in steps {
        if enabled && !out.iter().any(|entry| entry == name) {
            out.push(name.to_string());
        }
    }
    if rotation != 0 {
        let step = format!("rotate:{rotation}");
        if !out.iter().any(|entry| entry == &step) {
            out.push(step);
        }
    }
}

fn apply_pixel_filters(
    image: DynamicImage,
    options: &OcrPreprocess,
    cancel: &CancelToken,
) -> PdfResult<DynamicImage> {
    let mut current = image;
    if options.grayscale || options.contrast || options.denoise || options.binarize || options.deskew {
        let mut gray = to_gray(&current);
        cancel.check()?;
        if options.denoise {
            gray = median_denoise(&gray);
        }
        cancel.check()?;
        if options.contrast {
            gray = stretch_contrast(&gray);
        }
        cancel.check()?;
        if options.binarize {
            gray = binarize_otsu(&gray);
        }
        cancel.check()?;
        if options.deskew {
            if let Some(angle) = estimate_skew_angle(&gray) {
                let rgba = DynamicImage::ImageLuma8(gray).to_rgba8();
                let rotated = rotate_bilinear(&rgba, angle);
                gray = DynamicImage::ImageRgba8(rotated).to_luma8();
            }
        }
        cancel.check()?;
        current = DynamicImage::ImageLuma8(gray);
    }
    Ok(current)
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub fn ocr_pdf(
    input: &Path,
    output: &Path,
    options: &OcrOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<OcrResult> {
    let started = Instant::now();
    if !tesseract_available() {
        return Err(PdfError::OcrEngineUnavailable);
    }
    let languages = validate_languages(&options.languages)?;
    let lang_arg = languages.join("+");
    let output_mode = options.output_mode.to_lowercase();

    let geometries = render::page_geometries(input, password)?;
    let total = geometries.len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    let target: Vec<u32> = if options.pages.is_empty() {
        (1..=total).collect()
    } else {
        for p in &options.pages {
            if *p == 0 || *p > total {
                return Err(PdfError::RangeOutOfBounds);
            }
        }
        options.pages.clone()
    };

    let job = JobDir::new("ocr")?;
    let dpi = options.dpi.clamp(100, 600);
    let render_options = RenderOptions {
        dpi: dpi as f32,
        max_width: Some(6000),
        max_height: Some(6000),
    };

    let mut page_pdfs: Vec<(u32, PathBuf)> = Vec::new();
    let mut text_for_page: Vec<(u32, String)> = Vec::new();
    let mut pages_processed = 0u32;
    let mut pages_skipped = 0u32;
    let mut preprocess_applied: Vec<String> = Vec::new();

    for (index, page_number) in target.iter().enumerate() {
        cancel.check()?;
        progress(
            ProgressEvent::new("ocr.page", index as u64, target.len() as u64)
                .with_message(format!("{page_number} / {total}")),
        );

        if options.skip_text_pages {
            let has_text = render::extract_page_text(input, password, *page_number)
                .map(|t| t.trim().chars().count() > 12)
                .unwrap_or(false);
            if has_text {
                pages_skipped += 1;
                continue;
            }
        }

        let rendered = render::render_page(input, password, *page_number, &render_options)?;
        let image = rendered.to_dynamic_image()?;
        let png_path = job.file(&format!("page-{page_number:05}.png"));
        // `preprocess` writes the raster itself so orientation detection runs
        // on the exact file tesseract will open.
        let (_processed, rotation) = preprocess(image, &options.preprocess, &png_path, cancel)?;
        describe_preprocessing(&options.preprocess, rotation, &mut preprocess_applied);

        let out_base = job.file(&format!("page-{page_number:05}"));
        let mut args = vec![
            png_path.to_string_lossy().to_string(),
            out_base.to_string_lossy().to_string(),
            "-l".into(),
            lang_arg.clone(),
            "--psm".into(),
            options.psm.to_string(),
        ];
        match output_mode.as_str() {
            "text" | "markdown" => args.push("txt".into()),
            _ => {
                args.push("pdf".into());
                args.push("txt".into());
            }
        }
        run_tesseract(&args, cancel)?;

        if output_mode == "searchable_pdf" {
            let page_pdf = out_base.with_extension("pdf");
            if !page_pdf.exists() {
                return Err(PdfError::ProcessingFailed(format!(
                    "OCR produced no output for page {page_number}"
                )));
            }
            match fit_ocr_page(&page_pdf, &geometries[(page_number - 1) as usize]) {
                Ok(()) => {}
                Err(err) => return Err(err),
            }
            let txt_path = out_base.with_extension("txt");
            let text = std::fs::read_to_string(&txt_path).unwrap_or_default();
            text_for_page.push((*page_number, text));
            page_pdfs.push((*page_number, page_pdf));
        } else {
            let txt_path = out_base.with_extension("txt");
            let text = std::fs::read_to_string(&txt_path).unwrap_or_default();
            text_for_page.push((*page_number, text));
        }
        pages_processed += 1;
    }

    progress(ProgressEvent::new("ocr.assemble", 0, 1));

    let result = match output_mode.as_str() {
        "text" | "markdown" => {
            let extension = if output_mode == "text" { "txt" } else { "md" };
            let final_path = resolve_output_path(&with_extension(output, extension), policy)?;
            let mut body = String::new();
            let mut characters = 0u64;
            for (page, text) in &text_for_page {
                characters += text.chars().count() as u64;
                if output_mode == "markdown" {
                    body.push_str(&format!("## Page {page}\n\n{}\n\n", text.trim()));
                } else {
                    body.push_str(&format!("--- Page {page} ---\n{}\n", text.trim()));
                }
            }
            std::fs::write(&final_path, body).map_err(PdfError::from_io)?;
            OcrResult {
                path: final_path.display().to_string(),
                pages_processed,
                pages_skipped,
                characters,
                duration_ms: started.elapsed().as_millis() as u64,
                engine: engines::engine_status().tesseract_version.unwrap_or_else(|| "tesseract".into()),
                languages,
                preprocess_applied,
            }
        }
        _ => {
            // Assemble in page order: OCR pdfs for processed pages, original
            // pages otherwise (extracted in consecutive runs).
            let mut merge_list: Vec<PathBuf> = Vec::new();
            let processed_pages: std::collections::HashMap<u32, PathBuf> =
                page_pdfs.into_iter().collect();
            let mut page = 1u32;
            let mut run_start: Option<u32> = None;
            let flush_run = |run_start: &mut Option<u32>, page: u32, list: &mut Vec<PathBuf>| -> PdfResult<()> {
                if let Some(start) = run_start.take() {
                    let pages: Vec<u32> = (start..page).collect();
                    let part = job.file(&format!("orig-{start:05}-{}.pdf", page - 1));
                    organize::extract_pages(input, &pages, &part, OverwritePolicy::Replace, password)?;
                    list.push(part);
                }
                Ok(())
            };
            while page <= total {
                if let Some(ocred) = processed_pages.get(&page) {
                    flush_run(&mut run_start, page, &mut merge_list)?;
                    merge_list.push(ocred.clone());
                } else {
                    if run_start.is_none() {
                        run_start = Some(page);
                    }
                }
                page += 1;
            }
            flush_run(&mut run_start, total + 1, &mut merge_list)?;

            let final_path = resolve_output_path(&with_extension(output, "pdf"), policy)?;
            // A single-page document (or a one-page selection) has nothing to
            // merge: copy it straight to the destination instead.
            let merged_path = if merge_list.len() == 1 {
                std::fs::copy(&merge_list[0], &final_path).map_err(PdfError::from_io)?;
                final_path.clone()
            } else {
                crate::merge::merge_files(
                    &merge_list,
                    &final_path,
                    &crate::merge::MergeOptions { preserve_metadata: false },
                    OverwritePolicy::Replace,
                    progress,
                    cancel,
                )?
                .0
            };
            let characters = text_for_page.iter().map(|(_, t)| t.chars().count() as u64).sum();
            OcrResult {
                path: merged_path.display().to_string(),
                pages_processed,
                pages_skipped,
                characters,
                duration_ms: started.elapsed().as_millis() as u64,
                engine: engines::engine_status().tesseract_version.unwrap_or_else(|| "tesseract".into()),
                languages,
                preprocess_applied,
            }
        }
    };
    // Temp files are removed when `job` goes out of scope.
    progress(ProgressEvent::new("ocr.assemble", 1, 1));
    Ok(result)
}

/// Ensures a tesseract output page keeps the exact dimensions of the original
/// page by scaling its content into the original MediaBox.
fn fit_ocr_page(page_pdf: &Path, geometry: &render::PageGeometry) -> PdfResult<()> {
    let mut doc = lopdf::Document::load(page_pdf).map_err(|e| PdfError::from_lopdf(e, Some(page_pdf)))?;
    let pages = doc.get_pages();
    let Some((_, page_id)) = pages.iter().next() else {
        return Err(PdfError::ProcessingFailed("OCR page has no content".into()));
    };
    let page_id = *page_id;
    let media = page_mediabox(&doc, page_id)?;
    let (cur_w, cur_h) = ((media[2] - media[0]).abs(), (media[3] - media[1]).abs());
    let (target_w, target_h) = (geometry.display_width_pt.max(1.0), geometry.display_height_pt.max(1.0));
    if cur_w > 0.5 && cur_h > 0.5 && ((cur_w - target_w).abs() > 1.0 || (cur_h - target_h).abs() > 1.0) {
        let sx = target_w / cur_w;
        let sy = target_h / cur_h;
        wrap_page_content_transform(&mut doc, page_id, Matrix::scale(sx, sy))?;
    }
    let page = doc.get_object_mut(page_id)?.as_dict_mut()?;
    page.set(
        "MediaBox",
        vec![
            lopdf::Object::Real(0.0),
            lopdf::Object::Real(0.0),
            lopdf::Object::Real(target_w as f32),
            lopdf::Object::Real(target_h as f32),
        ],
    );
    page.remove(b"Rotate");
    let mut fresh = doc;
    save_document(&mut fresh, page_pdf, true)?;
    Ok(())
}

fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut out = path.to_path_buf();
    out.set_extension(extension);
    out
}

/// Compares page counts before/after for tests.
pub fn count_pages(path: &Path) -> PdfResult<u32> {
    let doc = lopdf::Document::load(path).map_err(|e| PdfError::from_lopdf(e, Some(path)))?;
    Ok(doc.get_pages().len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otsu_and_contrast_run() {
        let mut img = GrayImage::new(20, 20);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let value = if x < 10 { 40 } else { 220 };
            px[0] = value + ((x + y) % 5) as u8;
        }
        let binarized = binarize_otsu(&img);
        let dark_count = binarized.pixels().filter(|p| p[0] < 128).count();
        assert!((190..=210).contains(&dark_count), "dark count was {dark_count}");
        let stretched = stretch_contrast(&img);
        assert!(stretched.pixels().any(|p| p[0] == 0) || stretched.pixels().any(|p| p[0] == 255));
    }

    #[test]
    fn rotation_quarters_are_consistent() {
        let mut img = image::RgbaImage::new(4, 2);
        for (x, y, px) in img.enumerate_pixels_mut() {
            px.0 = if x == 0 && y == 0 { [255, 0, 0, 255] } else { [0, 0, 0, 255] };
        }
        let rotated = rotate_quarters(&img, 90);
        assert_eq!((rotated.width(), rotated.height()), (2, 4));
    }

    #[test]
    fn rotation_quarters_moves_the_marked_corner_clockwise() {
        let mut img = image::RgbaImage::new(3, 2);
        for pixel in img.pixels_mut() {
            pixel.0 = [0, 0, 0, 255];
        }
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let rotated = rotate_quarters(&img, 90);
        assert_eq!((rotated.width(), rotated.height()), (2, 3));
        // imageops::rotate90 turns clockwise, so (0, 0) moves to (h - 1, 0).
        assert_eq!(rotated.get_pixel(1, 0).0, [255, 0, 0, 255]);
        assert_eq!(rotate_quarters(&img, 180).get_pixel(2, 1).0, [255, 0, 0, 255]);
        assert_eq!(rotate_quarters(&img, 270).get_pixel(0, 2).0, [255, 0, 0, 255]);
        assert_eq!((rotate_quarters(&img, 0).width(), rotate_quarters(&img, 0).height()), (3, 2));
    }
}