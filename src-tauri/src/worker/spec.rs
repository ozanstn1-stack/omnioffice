//! Worker job specification and CLI argument serialization.

use std::path::PathBuf;

/// One worker job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Parse an office document into the editor model (`office::open_path`).
    OfficeOpen { path: PathBuf },
    /// Parse a UBL-TR invoice (XML or ZIP).
    InvoiceOpen { path: PathBuf },
    /// Parse a ZIP of UBL-TR invoices.
    InvoicePackage { path: PathBuf },
    /// Parse an `.eml` message.
    EmlOpen { path: PathBuf },
    /// Inspect PDF metadata, page count, and encryption status.
    PdfInfo { path: PathBuf },
    /// Extract the text layer of one page (unauthenticated).
    PdfPageText { path: PathBuf, page: u32 },
    /// Rasterize a single unauthenticated PDF page as a PNG or JPEG data URL.
    PdfPageImage { path: PathBuf, page: u32, max_width: u32, dpi: u32, jpeg_quality: Option<u8> },
    /// List annotations in display space for an unauthenticated PDF.
    PdfListAnnotations { path: PathBuf },
    /// Run structural/accessibility inspection on an unauthenticated PDF.
    PdfInspect { path: PathBuf },
    /// List AcroForm fields in an unauthenticated PDF.
    PdfListFormFields { path: PathBuf },
    /// List page objects (annotations, widgets, images) in an unauthenticated PDF.
    PdfListObjects { path: PathBuf },
    /// List content-stream text runs in an unauthenticated PDF.
    PdfListTextRuns { path: PathBuf },
    /// Validate PDF/A compliance on an unauthenticated PDF.
    PdfaValidate { path: PathBuf, level: String },
    /// Rasterize a single tile of an unauthenticated PDF page.
    PdfPageTile {
        path: PathBuf,
        page: u32,
        scale_bits: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        jpeg_quality: Option<u8>,
    },
    /// Verify embedded PDF signatures offline.
    PdfVerifySignatures { path: PathBuf },
    /// Repair a damaged PDF (qpdf when available, built-in rebuild otherwise).
    Repair { input: PathBuf, output: PathBuf },
}

impl Job {
    /// The arguments after `--worker` on the command line.
    pub fn args(&self) -> Vec<String> {
        let path = |value: &PathBuf| value.to_string_lossy().to_string();
        match self {
            Job::OfficeOpen { path: file } => vec!["office-open".into(), path(file)],
            Job::InvoiceOpen { path: file } => vec!["invoice-open".into(), path(file)],
            Job::InvoicePackage { path: file } => vec!["invoice-package".into(), path(file)],
            Job::EmlOpen { path: file } => vec!["eml-open".into(), path(file)],
            Job::PdfInfo { path: file } => vec!["pdf-info".into(), path(file)],
            Job::PdfPageText { path: file, page } => vec!["pdf-page-text".into(), path(file), page.to_string()],
            Job::PdfPageImage { path: file, page, max_width, dpi, jpeg_quality } => vec![
                "pdf-page-image".into(),
                path(file),
                page.to_string(),
                max_width.to_string(),
                dpi.to_string(),
                jpeg_quality.map(|q| q.to_string()).unwrap_or_else(|| "png".into()),
            ],
            Job::PdfListAnnotations { path: file } => vec!["pdf-list-annotations".into(), path(file)],
            Job::PdfInspect { path: file } => vec!["pdf-inspect".into(), path(file)],
            Job::PdfListFormFields { path: file } => vec!["pdf-list-form-fields".into(), path(file)],
            Job::PdfListObjects { path: file } => vec!["pdf-list-objects".into(), path(file)],
            Job::PdfListTextRuns { path: file } => vec!["pdf-list-text-runs".into(), path(file)],
            Job::PdfaValidate { path: file, level } => vec!["pdfa-validate".into(), path(file), level.clone()],
            Job::PdfPageTile { path: file, page, scale_bits, x, y, width, height, jpeg_quality } => vec![
                "pdf-page-tile".into(),
                path(file),
                page.to_string(),
                scale_bits.to_string(),
                x.to_string(),
                y.to_string(),
                width.to_string(),
                height.to_string(),
                jpeg_quality.map(|q| q.to_string()).unwrap_or_else(|| "png".into()),
            ],
            Job::PdfVerifySignatures { path: file } => vec!["pdf-verify-signatures".into(), path(file)],
            Job::Repair { input, output } => vec!["repair".into(), path(input), path(output)],
        }
    }

    /// Parses the arguments that follow `--worker`.
    pub fn parse(rest: &[String]) -> Result<Job, String> {
        let operation = rest.first().ok_or("--worker needs an operation")?;
        let path = |index: usize| -> Result<PathBuf, String> {
            rest.get(index).map(PathBuf::from).ok_or_else(|| format!("{operation} needs a path"))
        };
        let num = |index: usize, label: &str| -> Result<u32, String> {
            rest.get(index)
                .ok_or_else(|| format!("{operation} needs {label}"))?
                .parse::<u32>()
                .map_err(|_| format!("{operation}: invalid {label}"))
        };
        match operation.as_str() {
            "office-open" => Ok(Job::OfficeOpen { path: path(1)? }),
            "invoice-open" => Ok(Job::InvoiceOpen { path: path(1)? }),
            "invoice-package" => Ok(Job::InvoicePackage { path: path(1)? }),
            "eml-open" => Ok(Job::EmlOpen { path: path(1)? }),
            "pdf-info" => Ok(Job::PdfInfo { path: path(1)? }),
            "pdf-page-text" => Ok(Job::PdfPageText { path: path(1)?, page: num(2, "a page number")? }),
            "pdf-page-image" => {
                let quality = match rest.get(5).map(String::as_str) {
                    None | Some("png") => None,
                    Some(raw) => Some(raw.parse::<u8>().map_err(|_| format!("{operation}: invalid jpeg quality"))?),
                };
                Ok(Job::PdfPageImage {
                    path: path(1)?,
                    page: num(2, "a page number")?,
                    max_width: num(3, "max_width")?,
                    dpi: num(4, "dpi")?,
                    jpeg_quality: quality,
                })
            }
            "pdf-list-annotations" => Ok(Job::PdfListAnnotations { path: path(1)? }),
            "pdf-inspect" => Ok(Job::PdfInspect { path: path(1)? }),
            "pdf-list-form-fields" => Ok(Job::PdfListFormFields { path: path(1)? }),
            "pdf-list-objects" => Ok(Job::PdfListObjects { path: path(1)? }),
            "pdf-list-text-runs" => Ok(Job::PdfListTextRuns { path: path(1)? }),
            "pdfa-validate" => {
                let level = rest.get(2).cloned().ok_or_else(|| format!("{operation} needs a level"))?;
                Ok(Job::PdfaValidate { path: path(1)?, level })
            }
            "pdf-page-tile" => {
                let quality = match rest.get(8).map(String::as_str) {
                    None | Some("png") => None,
                    Some(raw) => Some(raw.parse::<u8>().map_err(|_| format!("{operation}: invalid jpeg quality"))?),
                };
                Ok(Job::PdfPageTile {
                    path: path(1)?,
                    page: num(2, "a page number")?,
                    scale_bits: num(3, "scale")?,
                    x: num(4, "x")?,
                    y: num(5, "y")?,
                    width: num(6, "width")?,
                    height: num(7, "height")?,
                    jpeg_quality: quality,
                })
            }
            "pdf-verify-signatures" => Ok(Job::PdfVerifySignatures { path: path(1)? }),
            "repair" => Ok(Job::Repair { input: path(1)?, output: path(2)? }),
            other => Err(format!("unknown worker operation: {other}")),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Job::OfficeOpen { .. } => "office-open",
            Job::InvoiceOpen { .. } => "invoice-open",
            Job::InvoicePackage { .. } => "invoice-package",
            Job::EmlOpen { .. } => "eml-open",
            Job::PdfInfo { .. } => "pdf-info",
            Job::PdfPageText { .. } => "pdf-page-text",
            Job::PdfPageImage { .. } => "pdf-page-image",
            Job::PdfListAnnotations { .. } => "pdf-list-annotations",
            Job::PdfInspect { .. } => "pdf-inspect",
            Job::PdfListFormFields { .. } => "pdf-list-form-fields",
            Job::PdfListObjects { .. } => "pdf-list-objects",
            Job::PdfListTextRuns { .. } => "pdf-list-text-runs",
            Job::PdfaValidate { .. } => "pdfa-validate",
            Job::PdfPageTile { .. } => "pdf-page-tile",
            Job::PdfVerifySignatures { .. } => "pdf-verify-signatures",
            Job::Repair { .. } => "repair",
        }
    }
}
