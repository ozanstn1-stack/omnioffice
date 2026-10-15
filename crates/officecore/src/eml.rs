//! KEP `.eml` (RFC 822 / MIME) messages, read-only.
//!
//! Kayıtlı Elektronik Posta delivers `.eml` files: a message with headers, a
//! text and/or HTML body and attachments (usually a signed PDF). This module
//! parses the common MIME shapes — `multipart/mixed`, `multipart/alternative`
//! and nested combinations — decodes base64 and quoted-printable transfer
//! encodings and RFC 2047 encoded-words, and returns the headers, the body and
//! the attachments as bytes. Nothing is executed, no remote resource is
//! fetched, and the message is never modified.
//!
//! An S/MIME signature or an encrypted body (`application/pkcs7-mime`) is kept
//! as an attachment and reported: the module never tries to decrypt or verify,
//! so a KEP message whose payload is encrypted is shown with a clear warning.

use crate::error::{OfficeError, OfficeResult};
use crate::io::{read_bytes, write_atomic};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One attachment (or an unparsed body part kept for the user).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct EmlAttachment {
    pub name: String,
    pub content_type: String,
    pub size: usize,
    /// Where the attachment was extracted, when it was written to disk.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// The attachment bytes (kept for small parts so the UI can preview them).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data_base64: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct EmlMessage {
    pub subject: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub date: String,
    pub message_id: String,
    /// Every header, in order, for a "raw headers" view.
    pub headers: Vec<EmlHeader>,
    pub body_text: String,
    pub body_html: String,
    pub attachments: Vec<EmlAttachment>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct EmlHeader {
    pub name: String,
    pub value: String,
}

/// Parses a `.eml` message.
pub fn parse_eml(bytes: &[u8]) -> OfficeResult<EmlMessage> {
    parse_eml_with_depth(bytes, 0)
}

/// Parses a message, carrying the nesting depth across `message/rfc822`
/// boundaries. A forwarded message used to restart the depth at zero, so a
/// chain of nested messages recursed without bound and overflowed the stack.
fn parse_eml_with_depth(bytes: &[u8], depth: usize) -> OfficeResult<EmlMessage> {
    let (header_block, body) = split_headers(bytes);
    let headers = parse_headers(&header_block);
    if headers.is_empty() {
        return Err(OfficeError::corrupt("The file has no message headers."));
    }
    let mut message = EmlMessage::default();
    let get = |name: &str| {
        headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| decode_encoded_words(value))
            .unwrap_or_default()
    };
    message.subject = get("Subject");
    message.from = get("From");
    message.to = get("To");
    message.cc = get("Cc");
    message.date = get("Date");
    message.message_id = get("Message-ID");
    message.headers = headers
        .iter()
        .map(|(name, value)| EmlHeader { name: name.clone(), value: decode_encoded_words(value) })
        .collect();

    let (content_type, params) = content_type_of(&headers);
    walk_part(&content_type, &params, &headers, body, &mut message, depth);
    if message.body_text.trim().is_empty() && message.body_html.trim().is_empty() && message.attachments.is_empty() {
        message.warnings.push("The message has no readable body or attachment.".into());
    }
    message.warnings.sort();
    message.warnings.dedup();
    Ok(message)
}

/// Reads a `.eml` file and extracts its attachments next to `cache_dir`.
pub fn read_eml_file(path: &Path, cache_dir: &Path) -> OfficeResult<EmlMessage> {
    let bytes = read_bytes(path)?;
    let mut message = parse_eml(&bytes)?;
    let stem = crate::io::file_stem(path);
    let directory = cache_dir.join(format!("eml-{}", uuid::Uuid::new_v4().simple()));
    let mut wrote = false;
    for (index, attachment) in message.attachments.iter_mut().enumerate() {
        let data =
            base64::engine::general_purpose::STANDARD.decode(attachment.data_base64.as_bytes()).unwrap_or_default();
        if data.is_empty() {
            continue;
        }
        let name = safe_file_name(&attachment.name, index);
        if !wrote {
            std::fs::create_dir_all(&directory).map_err(|error| OfficeError::from_io(error, &directory))?;
            wrote = true;
        }
        let target = directory.join(&name);
        if write_atomic(&target, &data).is_ok() {
            attachment.path = target.to_string_lossy().to_string();
        } else {
            message.warnings.push(format!("{stem}: {name} could not be written."));
        }
    }
    Ok(message)
}

/// A file name that cannot escape the extraction directory.
fn safe_file_name(name: &str, index: usize) -> String {
    let base = name.trim().rsplit(['/', '\\']).next().unwrap_or("").trim();
    let cleaned: String = base
        .chars()
        .map(|ch| {
            if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        format!("ek-{}", index + 1)
    } else {
        cleaned
    }
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

/// Splits the raw message at the first blank line.
fn split_headers(bytes: &[u8]) -> (String, Vec<u8>) {
    // Find \r\n\r\n or \n\n.
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            let after = index + 1;
            if after < bytes.len() && bytes[after] == b'\n' {
                return (decode_latin1(&bytes[..index]), bytes[after + 1..].to_vec());
            }
            if after + 1 < bytes.len() && bytes[after] == b'\r' && bytes[after + 1] == b'\n' {
                return (decode_latin1(&bytes[..index]), bytes[after + 2..].to_vec());
            }
        }
        index += 1;
    }
    (decode_latin1(bytes), Vec::new())
}

/// Parses headers, unfolding continuation lines.
fn parse_headers(block: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in block.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}

/// The top-level content type and its parameters.
fn content_type_of(headers: &[(String, String)]) -> (String, HashMap<String, String>) {
    let raw = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Type"))
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| "text/plain".to_string());
    parse_content_type(&raw)
}

fn parse_content_type(raw: &str) -> (String, HashMap<String, String>) {
    let mut parts = raw.split(';');
    let kind = parts.next().unwrap_or("text/plain").trim().to_ascii_lowercase();
    let mut params = HashMap::new();
    for part in parts {
        if let Some((name, value)) = part.split_once('=') {
            let value = value.trim().trim_matches('"').to_string();
            params.insert(name.trim().to_ascii_lowercase(), value);
        }
    }
    (kind, params)
}

/// Decodes RFC 2047 encoded-words (`=?UTF-8?B?...?=` / `=?UTF-8?Q?...?=`).
fn decode_encoded_words(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("?=") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &after[..end];
        let fields: Vec<&str> = token.splitn(3, '?').collect();
        if fields.len() == 3 {
            let charset = fields[0];
            let encoding = fields[1].to_ascii_uppercase();
            let payload = fields[2];
            let bytes = match encoding.as_str() {
                "B" => base64::engine::general_purpose::STANDARD.decode(payload.as_bytes()).unwrap_or_default(),
                "Q" => decode_quoted_printable(payload.as_bytes(), true),
                _ => payload.as_bytes().to_vec(),
            };
            out.push_str(&decode_charset(&bytes, charset));
        } else {
            out.push_str(&rest[start..start + 2 + end + 2]);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Body walking
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn walk_part(
    content_type: &str,
    params: &HashMap<String, String>,
    headers: &[(String, String)],
    body: Vec<u8>,
    message: &mut EmlMessage,
    depth: usize,
) {
    if depth > 12 {
        message.warnings.push("Nested message parts are deeper than the safe limit.".into());
        return;
    }
    if content_type.starts_with("multipart/") {
        let Some(boundary) = params.get("boundary") else {
            message.warnings.push("A multipart message has no boundary and was read as text.".into());
            push_text(message, &decode_latin1(&body));
            return;
        };
        for part in split_parts(&body, boundary) {
            let (part_headers, part_body) = split_headers(&part);
            let headers = parse_headers(&part_headers);
            let (part_type, part_params) = content_type_of(&headers);
            walk_part(&part_type, &part_params, &headers, part_body, message, depth + 1);
        }
        return;
    }
    let encoding = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Transfer-Encoding"))
        .map(|(_, value)| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let decoded = match encoding.as_str() {
        "base64" => base64::engine::general_purpose::STANDARD.decode(strip_whitespace(&body)).unwrap_or_default(),
        "quoted-printable" => decode_quoted_printable(&body, false),
        _ => body.clone(),
    };
    let disposition = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Disposition"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let (disposition_kind, disposition_params) = parse_content_type(&disposition);
    let charset = params.get("charset").cloned().unwrap_or_default();
    let file_name = disposition_params
        .get("filename")
        .or_else(|| params.get("name"))
        .map(|value| decode_encoded_words(value))
        .unwrap_or_default();
    let is_attachment = disposition_kind.eq_ignore_ascii_case("attachment") || !file_name.is_empty();

    if is_attachment {
        push_attachment(message, file_name, content_type, decoded);
        return;
    }
    match content_type {
        "text/plain" => {
            let text = decode_charset(&decoded, &charset);
            if message.body_text.trim().is_empty() {
                message.body_text = text;
            } else {
                message.body_text.push('\n');
                message.body_text.push_str(&text);
            }
        }
        "text/html" => {
            let html = decode_charset(&decoded, &charset);
            if message.body_html.trim().is_empty() {
                message.body_html = html;
            }
        }
        "message/rfc822" => {
            // A forwarded message: recurse into it, carrying the depth so the
            // walk_part guard bounds the whole chain.
            if let Ok(nested) = parse_eml_with_depth(&decoded, depth + 1) {
                if message.body_text.trim().is_empty() {
                    message.body_text = nested.body_text;
                }
                if message.body_html.trim().is_empty() {
                    message.body_html = nested.body_html;
                }
                message.attachments.extend(nested.attachments);
                message.warnings.extend(nested.warnings);
            }
        }
        other => {
            // A part we cannot display (e.g. an encrypted S/MIME body).
            let name = if file_name.is_empty() { format!("govde-{}", other.replace('/', "-")) } else { file_name };
            push_attachment(message, name, content_type, decoded);
        }
    }
}

fn push_text(message: &mut EmlMessage, text: &str) {
    if message.body_text.trim().is_empty() {
        message.body_text = text.to_string();
    }
}

fn push_attachment(message: &mut EmlMessage, name: String, content_type: &str, data: Vec<u8>) {
    if content_type.eq_ignore_ascii_case("application/pkcs7-mime")
        || content_type.eq_ignore_ascii_case("application/x-pkcs7-mime")
    {
        message
            .warnings
            .push("An S/MIME part is kept as an attachment; the message is not decrypted or verified.".into());
    }
    let name = if name.trim().is_empty() {
        let extension = content_type.rsplit('/').next().unwrap_or("bin");
        format!("ek-{}.{extension}", message.attachments.len() + 1)
    } else {
        name
    };
    let size = data.len();
    message.attachments.push(EmlAttachment {
        name,
        content_type: content_type.to_string(),
        size,
        path: String::new(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(&data),
    });
}

/// Splits a multipart body on its boundary.
fn split_parts(body: &[u8], boundary: &str) -> Vec<Vec<u8>> {
    let delimiter = format!("--{boundary}");
    let mut parts: Vec<Vec<u8>> = Vec::new();
    let mut current: Option<Vec<u8>> = None;
    for line in split_keep_lines(body) {
        let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
        let trimmed = trimmed.strip_suffix(b"\r").unwrap_or(trimmed);
        if trimmed.starts_with(delimiter.as_bytes()) {
            if let Some(part) = current.take() {
                parts.push(trim_body(part));
            }
            if trimmed.starts_with(format!("{delimiter}--").as_bytes()) {
                break;
            }
            current = Some(Vec::new());
            continue;
        }
        if let Some(part) = current.as_mut() {
            part.extend_from_slice(&line);
        }
    }
    if let Some(part) = current.take() {
        parts.push(trim_body(part));
    }
    parts
}

/// The MIME preamble before the first boundary and the trailing CRLF are not
/// part of the part.
fn trim_body(part: Vec<u8>) -> Vec<u8> {
    let mut end = part.len();
    while end > 0 && (part[end - 1] == b'\n' || part[end - 1] == b'\r') {
        end -= 1;
    }
    part[..end].to_vec()
}

fn split_keep_lines(body: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, byte) in body.iter().enumerate() {
        if *byte == b'\n' {
            out.push(body[start..=index].to_vec());
            start = index + 1;
        }
    }
    if start < body.len() {
        out.push(body[start..].to_vec());
    }
    out
}

// ---------------------------------------------------------------------------
// Encodings
// ---------------------------------------------------------------------------

fn strip_whitespace(body: &[u8]) -> Vec<u8> {
    body.iter().copied().filter(|byte| !byte.is_ascii_whitespace()).collect()
}

/// Decodes quoted-printable. `header` mode does not treat `_` as a space and
/// does not join soft line breaks (the `Q` encoding uses `_` for space).
fn decode_quoted_printable(body: &[u8], header: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        let byte = body[index];
        if byte == b'=' {
            if header {
                // `=XX` only.
                if index + 2 < body.len() {
                    if let Ok(value) = u8::from_str_radix(&String::from_utf8_lossy(&body[index + 1..index + 3]), 16) {
                        out.push(value);
                        index += 3;
                        continue;
                    }
                }
                out.push(byte);
                index += 1;
                continue;
            }
            // A soft line break: `=` followed by CRLF/LF.
            if index + 1 < body.len() && body[index + 1] == b'\n' {
                index += 2;
                continue;
            }
            if index + 2 < body.len() && body[index + 1] == b'\r' && body[index + 2] == b'\n' {
                index += 3;
                continue;
            }
            if index + 2 < body.len() {
                if let Ok(value) = u8::from_str_radix(&String::from_utf8_lossy(&body[index + 1..index + 3]), 16) {
                    out.push(value);
                    index += 3;
                    continue;
                }
            }
            out.push(byte);
            index += 1;
        } else if header && byte == b'_' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(byte);
            index += 1;
        }
    }
    out
}

fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| *byte as char).collect()
}

/// Decodes a body with the named charset. UTF-8 is used when the charset is
/// unknown; the Turkish ISO-8859-9 / Windows-1254 positions differ from
/// Latin-1, so they get their own mapping.
fn decode_charset(bytes: &[u8], charset: &str) -> String {
    match charset.trim().to_ascii_lowercase().as_str() {
        "iso-8859-9" | "latin5" | "windows-1254" | "cp1254" => {
            bytes.iter().map(|byte| iso_8859_9_char(*byte)).collect()
        }
        "iso-8859-1" | "latin1" | "windows-1252" | "us-ascii" | "ascii" => decode_latin1(bytes),
        _ => String::from_utf8_lossy(bytes).to_string(),
    }
}

/// ISO-8859-9 differs from Latin-1 at the six Turkish letters.
fn iso_8859_9_char(byte: u8) -> char {
    match byte {
        0xD0 => 'Ğ',
        0xDD => 'İ',
        0xDE => 'Ş',
        0xF0 => 'ğ',
        0xFD => 'ı',
        0xFE => 'ş',
        other => other as char,
    }
}

/// A stable directory for extracted attachments.
pub fn default_cache_dir() -> PathBuf {
    std::env::temp_dir().join("omnioffice-eml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        // multipart/mixed: a multipart/alternative body (plain + html) and a
        // base64 attachment, with an RFC 2047 encoded-word subject.
        let mut eml = String::new();
        eml.push_str("From: =?UTF-8?B?S0VQIEfDtm5kZXJpY2k=?= <kep@example.org>\r\n");
        eml.push_str("To: avukat@example.org\r\n");
        eml.push_str("Subject: =?UTF-8?Q?Tebligat_=C3=B6rne=C4=9Fi?=\r\n");
        eml.push_str("Date: Mon, 12 Oct 2026 09:30:00 +0300\r\n");
        eml.push_str("Message-ID: <abc123@kep.example.org>\r\n");
        eml.push_str("MIME-Version: 1.0\r\n");
        eml.push_str("Content-Type: multipart/mixed; boundary=\"BOUND\"\r\n");
        eml.push_str("\r\n");
        eml.push_str("--BOUND\r\n");
        eml.push_str("Content-Type: multipart/alternative; boundary=\"ALT\"\r\n");
        eml.push_str("\r\n");
        eml.push_str("--ALT\r\n");
        eml.push_str("Content-Type: text/plain; charset=utf-8\r\n");
        eml.push_str("Content-Transfer-Encoding: quoted-printable\r\n");
        eml.push_str("\r\n");
        eml.push_str("Say=C4=B1n Avukat, tebligat ekte g=C3=B6nderilmi=C5=9Ftir.\r\n");
        eml.push_str("--ALT\r\n");
        eml.push_str("Content-Type: text/html; charset=utf-8\r\n");
        eml.push_str("\r\n");
        eml.push_str("<html><body><p>Sayın Avukat, tebligat ekte gönderilmiştir.</p></body></html>\r\n");
        eml.push_str("--ALT--\r\n");
        eml.push_str("--BOUND\r\n");
        eml.push_str("Content-Type: application/pdf; name=\"tebligat.pdf\"\r\n");
        eml.push_str("Content-Transfer-Encoding: base64\r\n");
        eml.push_str("Content-Disposition: attachment; filename=\"tebligat.pdf\"\r\n");
        eml.push_str("\r\n");
        // "%PDF-1.4" in base64.
        eml.push_str("JVBERi0xLjQK\r\n");
        eml.push_str("--BOUND--\r\n");
        eml
    }

    #[test]
    fn parses_headers_and_decodes_encoded_words() {
        let message = parse_eml(fixture().as_bytes()).unwrap();
        assert_eq!(message.subject, "Tebligat örneği");
        assert!(message.from.contains("KEP Gönderici"), "from: {}", message.from);
        assert_eq!(message.to, "avukat@example.org");
        assert_eq!(message.message_id, "<abc123@kep.example.org>");
        assert!(message.headers.iter().any(|header| header.name == "Date"));
    }

    #[test]
    fn decodes_the_plain_body_and_keeps_the_html() {
        let message = parse_eml(fixture().as_bytes()).unwrap();
        assert_eq!(message.body_text.trim(), "Sayın Avukat, tebligat ekte gönderilmiştir.");
        assert!(message.body_html.contains("Sayın Avukat"));
    }

    #[test]
    fn extracts_the_attachment() {
        let message = parse_eml(fixture().as_bytes()).unwrap();
        assert_eq!(message.attachments.len(), 1);
        let attachment = &message.attachments[0];
        assert_eq!(attachment.name, "tebligat.pdf");
        assert_eq!(attachment.content_type, "application/pdf");
        let data = base64::engine::general_purpose::STANDARD.decode(attachment.data_base64.as_bytes()).unwrap();
        assert!(data.starts_with(b"%PDF"));
    }

    #[test]
    fn reads_a_turkish_iso_8859_9_body() {
        let mut eml = String::new();
        eml.push_str("Subject: test\r\nContent-Type: text/plain; charset=iso-8859-9\r\n\r\n");
        let mut bytes = eml.into_bytes();
        // "Şğİı" in ISO-8859-9.
        bytes.extend_from_slice(&[0xDE, 0xF0, 0xDD, 0xFD]);
        let message = parse_eml(&bytes).unwrap();
        assert_eq!(message.body_text, "Şğİı");
    }

    #[test]
    fn reports_an_encrypted_smime_body() {
        let mut eml = String::new();
        eml.push_str("Subject: gizli\r\nContent-Type: application/pkcs7-mime; name=\"smime.p7m\"\r\n");
        eml.push_str("Content-Transfer-Encoding: base64\r\n\r\nAAAA\r\n");
        let message = parse_eml(eml.as_bytes()).unwrap();
        assert_eq!(message.attachments.len(), 1);
        assert!(message.warnings.iter().any(|warning| warning.contains("S/MIME")), "{:?}", message.warnings);
    }

    #[test]
    fn rejects_a_file_without_headers() {
        assert!(parse_eml(b"\r\n\r\nbody only").is_err());
    }

    #[test]
    fn sanitizes_attachment_names() {
        assert_eq!(safe_file_name("../../etc/passwd", 0), "passwd");
        assert_eq!(safe_file_name("C:\\Windows\\evil.exe", 0), "evil.exe");
        assert_eq!(safe_file_name("", 2), "ek-3");
    }

    fn nested_eml(depth: usize) -> String {
        let mut out = String::new();
        for _ in 0..depth {
            out.push_str("From: a@example.com\r\nContent-Type: message/rfc822\r\n\r\n");
        }
        out.push_str("From: a@example.com\r\nContent-Type: text/plain\r\n\r\nhello\r\n");
        out
    }

    #[test]
    fn nested_messages_are_depth_limited_not_stack_overflowing() {
        // A chain of message/rfc822 parts used to restart the depth counter and
        // overflow the stack; the whole chain must now stay bounded.
        for depth in [1usize, 12, 13, 50, 20_000] {
            let message = parse_eml(nested_eml(depth).as_bytes())
                .unwrap_or_else(|error| panic!("depth {depth} must parse: {error}"));
            if depth > 12 {
                assert!(
                    message.warnings.iter().any(|warning| warning.contains("deeper than the safe limit")),
                    "depth {depth} must warn: {:?}",
                    message.warnings
                );
            }
        }
    }
}
