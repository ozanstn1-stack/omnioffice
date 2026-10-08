//! Opens a hyperlink from a spreadsheet cell in the system browser or mail
//! client. The webview has no opener permission, so this command is the only
//! way out, and it applies the same allow-list the file importers use: web and
//! mail links only, never a local path, a network share or a script.

use officecore::model::safe_link_target;

/// The target to open, or why it must not be opened.
fn external_link_target(url: &str) -> Result<String, String> {
    match safe_link_target(url) {
        // `#Sheet!A1` is a place inside the document, not something to launch.
        Some(target) if !target.starts_with('#') && !target.chars().any(char::is_whitespace) => Ok(target),
        _ => Err("Only web and e-mail links can be opened.".to_string()),
    }
}

#[tauri::command]
pub fn open_external_link(url: String) -> Result<(), String> {
    let target = external_link_target(&url)?;
    tauri_plugin_opener::open_url(target, None::<&str>).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::external_link_target;

    #[test]
    fn web_and_mail_links_are_allowed() {
        assert_eq!(external_link_target("https://example.org/docs").unwrap(), "https://example.org/docs");
        assert_eq!(external_link_target(" http://example.org ").unwrap(), "http://example.org");
        assert_eq!(external_link_target("mailto:me@example.org").unwrap(), "mailto:me@example.org");
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
