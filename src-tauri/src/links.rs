//! Opens a hyperlink from a spreadsheet cell in the system browser or mail
//! client. The webview has no opener permission, so this command is the only
//! way out, and it applies the same allow-list the file importers use: web and
//! mail links only, never a local path, a network share or a script.

use officecore::model::safe_link_target;
use pdfcore::error::PdfError;

/// The target to open, or why it must not be opened.
fn external_link_target(url: &str) -> Result<String, String> {
    match safe_link_target(url) {
        // `#Sheet!A1` is a place inside the document, not something to launch.
        Some(target) if !target.starts_with('#') && !target.chars().any(char::is_whitespace) => Ok(target),
        _ => Err("Only web and e-mail links can be opened.".to_string()),
    }
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub fn open_external_link(url: String) -> Result<(), PdfError> {
    let target = external_link_target(&url).map_err(PdfError::InvalidInput)?;
    tauri_plugin_opener::open_url(target, None::<&str>).map_err(|error| PdfError::Internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{external_link_target, open_external_link};

    #[test]
    fn a_rejected_link_is_a_typed_error_on_the_wire() {
        // The UI maps `code` to a localized message, so the shape matters: a
        // rejected link is `invalid_input`, not a bare string.
        let error = open_external_link("javascript:alert(1)".into()).expect_err("javascript must be refused");
        let wire = serde_json::to_value(&error).expect("serializes");
        assert_eq!(wire["code"], "invalid_input", "{wire}");
        assert!(wire["message"].as_str().unwrap_or_default().contains("web and e-mail"), "{wire}");
    }

    #[test]
    fn web_and_mail_links_are_allowed() {
        assert_eq!(external_link_target("https://example.org/docs").unwrap(), "https://example.org/docs");
        assert_eq!(external_link_target(" http://example.org ").unwrap(), "http://example.org");
        assert_eq!(external_link_target("mailto:me@example.org").unwrap(), "mailto:me@example.org");
    }

    #[test]
    fn the_edevlet_verification_page_is_allowed() {
        // The Reader's "Verify document" button opens the official page; it is
        // https, so the allow-list accepts it (the host itself is not restricted,
        // only the scheme: web and mail links, never a local path or script).
        assert_eq!(
            external_link_target("https://www.turkiye.gov.tr/belge-dogrulama").unwrap(),
            "https://www.turkiye.gov.tr/belge-dogrulama"
        );
        assert!(external_link_target("https://dogrulama.turkiye.gov.tr/x").is_ok());
    }

    #[test]
    fn everything_else_is_refused() {
        for bad in [
            "",
            "#Sheet2!A1",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "ftp://example.org/file",
            r"\\server\share\file.exe",
            "//server/share",
            "C:\\Windows\\notepad.exe",
            "notepad.exe",
            "https://example.org/a b",
            "https://example.org/\u{7}",
            "https://",
        ] {
            assert!(external_link_target(bad).is_err(), "{bad:?} must be refused");
        }
    }
}
