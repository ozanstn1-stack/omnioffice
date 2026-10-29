//! GİB karekod (e-Fatura / e-Arşiv QR) reading.
//!
//! Turkish e-invoices carry a QR code ("karekod") in the top-right corner whose
//! payload is a small JSON object with the sender's VKN/TCKN, the date, the
//! invoice number, the ETTN, the currency, the VAT bases per rate and the
//! payable amount. This module decodes the QR with `rxing` (pure Rust, QR only)
//! from a rendered PDF page and parses the payload; the deterministic
//! comparison against the invoice's own fields happens in the app layer.
//!
//! Nothing here contacts a server: the QR is read locally and the fields are
//! checked with the same VKN/TCKN checksums the rest of the suite uses.

use crate::error::PdfResult;
use crate::render::{render_page, RenderOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The parsed GİB karekod payload. Unknown keys are ignored; the rate-keyed
/// `kdvmatrah(N)` / `hesaplanankdv(N)` pairs are kept in maps.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GibQr {
    /// Sender VKN (10 digits) or TCKN (11 digits).
    pub vkn_tckn: String,
    /// Recipient VKN/TCKN.
    pub avkn_tckn: String,
    pub senaryo: String,
    pub tip: String,
    /// `YYYY-MM-DD`.
    pub tarih: String,
    /// Invoice number, e.g. `GIB2026000000001`.
    pub no: String,
    /// ETTN.
    pub ettn: String,
    pub para_birimi: String,
    pub mal_hizmet_toplam: String,
    pub vergi_dahil: String,
    pub odenecek: String,
    /// VAT base per rate, keyed by the rate string inside `kdvmatrah(N)`.
    pub kdv_matrah: BTreeMap<String, String>,
    /// VAT amount per rate, keyed by the rate string inside `hesaplanankdv(N)`.
    pub hesaplanan_kdv: BTreeMap<String, String>,
}

/// Parses the JSON payload of a GİB karekod. Returns `None` when the text is
/// not a JSON object (some older codes wrap the JSON in a URL).
pub fn parse_payload(text: &str) -> Option<GibQr> {
    let trimmed = text.trim();
    let json = if trimmed.starts_with('{') {
        trimmed
    } else {
        // A URL like `...?data={...}` or a query string: take the outermost braces.
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        if end <= start {
            return None;
        }
        &trimmed[start..=end]
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let object = value.as_object()?;
    let get = |key: &str| object.get(key).and_then(|value| value.as_str()).unwrap_or("").trim().to_string();
    let mut qr = GibQr {
        vkn_tckn: get("vkntckn"),
        avkn_tckn: get("avkntckn"),
        senaryo: get("senaryo"),
        tip: get("tip"),
        tarih: get("tarih"),
        no: get("no"),
        ettn: get("ettn"),
        para_birimi: get("parabirimi"),
        mal_hizmet_toplam: get("malhizmettoplam"),
        vergi_dahil: get("vergidahil"),
        odenecek: get("odenecek"),
        ..Default::default()
    };
    for (key, value) in object {
        let Some(text) = value.as_str() else { continue };
        if let Some(rate) = key.strip_prefix("kdvmatrah(").and_then(|rest| rest.strip_suffix(')')) {
            qr.kdv_matrah.insert(rate.to_string(), text.trim().to_string());
        } else if let Some(rate) = key.strip_prefix("hesaplanankdv(").and_then(|rest| rest.strip_suffix(')')) {
            qr.hesaplanan_kdv.insert(rate.to_string(), text.trim().to_string());
        }
    }
    if qr.vkn_tckn.is_empty() && qr.no.is_empty() && qr.ettn.is_empty() {
        // A JSON object that is not a GİB payload.
        return None;
    }
    Some(qr)
}

/// Decodes a QR from an 8-bit grayscale buffer.
pub fn decode_luma(luma: &[u8], width: u32, height: u32) -> Option<String> {
    if width == 0 || height == 0 || luma.len() < (width * height) as usize {
        return None;
    }
    let mut hints = rxing::DecodeHints { TryHarder: Some(true), ..Default::default() };
    rxing::helpers::detect_in_luma_slice_with_hints(
        luma,
        width,
        height,
        Some(rxing::BarcodeFormat::QR_CODE),
        &mut hints,
    )
    .ok()
    .map(|result| result.getText().to_string())
}

/// Decodes a QR from an RGBA buffer (the renderer's pixel format).
pub fn decode_rgba(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    if rgba.len() < (width as usize * height as usize * 4) {
        return None;
    }
    let mut luma = Vec::with_capacity((width as usize) * (height as usize));
    for index in 0..(width as usize * height as usize) {
        let base = index * 4;
        let value = 0.299 * rgba[base] as f32 + 0.587 * rgba[base + 1] as f32 + 0.114 * rgba[base + 2] as f32;
        luma.push(value.round().clamp(0.0, 255.0) as u8);
    }
    decode_luma(&luma, width, height)
}

/// Renders `page` of a PDF at a few resolutions and returns the first QR that
/// decodes, as `(payload, raw_text)`. A low-resolution scan that cannot be read
/// yields `Ok(None)` so the caller can show a clear message.
pub fn decode_page(path: &Path, password: Option<&str>, page: u32) -> PdfResult<Option<(GibQr, String)>> {
    // 150 dpi is the default render; 220/300 catch a small or dense code.
    for dpi in [150.0_f32, 220.0, 300.0] {
        let options = RenderOptions { dpi, max_width: Some(4000), max_height: Some(4000) };
        // A rendering failure (no pdfium) is reported, not retried per dpi.
        let rendered = render_page(path, password, page, &options)?;
        if let Some(text) = decode_rgba(&rendered.rgba, rendered.width, rendered.height) {
            // A QR that is not a GİB payload is returned with an empty struct so
            // the caller can still show the raw text.
            return Ok(Some((parse_payload(&text).unwrap_or_default(), text)));
        }
    }
    Ok(None)
}

/// Reads the first page that carries a GİB karekod, up to `max_pages`.
pub fn decode_document(path: &Path, password: Option<&str>, max_pages: u32) -> PdfResult<Option<(GibQr, String, u32)>> {
    let pages = crate::render::page_geometries(path, password)?;
    let limit = pages.len().min(max_pages.max(1) as usize);
    for index in 1..=limit as u32 {
        if let Some((qr, raw)) = decode_page(path, password, index)? {
            return Ok(Some((qr, raw, index)));
        }
    }
    Ok(None)
}

/// The 10-digit VKN weighted modulo-9 check. The all-same number
/// `4444444444` passes the arithmetic but is not a real VKN, so it is rejected
/// (the same guard `redact::looks_like_vkn` uses).
pub fn vkn_valid(value: &str) -> bool {
    let digits: Vec<u32> = value.trim().chars().filter_map(|ch| ch.to_digit(10)).collect();
    if value.trim().len() != 10 || digits.len() != 10 {
        return false;
    }
    if digits.iter().all(|digit| *digit == digits[0]) {
        return false;
    }
    let mut sum = 0u32;
    for (index, digit) in digits.iter().take(9).enumerate() {
        let tmp = (digit + 9 - index as u32) % 10;
        let mut value = (tmp * 2u32.pow(9 - index as u32)) % 9;
        if tmp != 0 && value == 0 {
            value = 9;
        }
        sum += value;
    }
    let check = (10 - (sum % 10)) % 10;
    check == digits[9]
}

/// The 11-digit TCKN check.
pub fn tckn_valid(value: &str) -> bool {
    let digits: Vec<u32> = value.trim().chars().filter_map(|ch| ch.to_digit(10)).collect();
    if value.trim().len() != 11 || digits.len() != 11 || digits[0] == 0 {
        return false;
    }
    let odd = digits[0] + digits[2] + digits[4] + digits[6] + digits[8];
    let even = digits[1] + digits[3] + digits[5] + digits[7];
    // `odd * 7 - even` can be negative (e.g. 35 - 36), which underflows u32;
    // adding ten before the modulo keeps the result identical and safe.
    let tenth = (odd * 7 + 10 - (even % 10)) % 10;
    let eleventh = digits.iter().take(10).sum::<u32>() % 10;
    digits[9] == tenth && digits[10] == eleventh
}

/// True when the value is a valid VKN or TCKN.
pub fn tax_id_valid(value: &str) -> bool {
    vkn_valid(value) || tckn_valid(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rxing::{BarcodeFormat, Writer};

    fn payload() -> String {
        r#"{"vkntckn":"1111111111","avkntckn":"2222222222","senaryo":"TEMELFATURA","tip":"SATIS","tarih":"2026-03-14","no":"GIB2026000000001","ettn":"04e26a62-7c00-46d0-878c-6f7c60834525","parabirimi":"TRY","malhizmettoplam":"1000.00","kdvmatrah(20)":"1000.00","hesaplanankdv(20)":"200.00","vergidahil":"1200.00","odenecek":"1200.00"}"#
            .to_string()
    }

    #[test]
    fn parses_the_documented_payload() {
        let qr = parse_payload(&payload()).unwrap();
        assert_eq!(qr.vkn_tckn, "1111111111");
        assert_eq!(qr.no, "GIB2026000000001");
        assert_eq!(qr.ettn, "04e26a62-7c00-46d0-878c-6f7c60834525");
        assert_eq!(qr.tarih, "2026-03-14");
        assert_eq!(qr.odenecek, "1200.00");
        assert_eq!(qr.kdv_matrah.get("20").map(String::as_str), Some("1000.00"));
        assert_eq!(qr.hesaplanan_kdv.get("20").map(String::as_str), Some("200.00"));
    }

    #[test]
    fn parses_a_payload_wrapped_in_a_url() {
        let wrapped = format!("https://example.invalid/verify?data={}", payload());
        let qr = parse_payload(&wrapped).unwrap();
        assert_eq!(qr.no, "GIB2026000000001");
    }

    #[test]
    fn rejects_a_json_object_that_is_not_a_payload() {
        assert!(parse_payload(r#"{"hello":"world"}"#).is_none());
        assert!(parse_payload("not json at all").is_none());
    }

    /// Renders a QR with rxing's own encoder, so the decoder is exercised end
    /// to end without committing a binary fixture.
    fn encode(text: &str, size: i32) -> (Vec<u8>, u32, u32) {
        let matrix = rxing::qrcode::QRCodeWriter.encode(text, &BarcodeFormat::QR_CODE, size, size).expect("encode");
        let (width, height) = (matrix.getWidth(), matrix.getHeight());
        let mut luma = vec![255u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                if matrix.get(x, y) {
                    luma[(y * width + x) as usize] = 0;
                }
            }
        }
        (luma, width, height)
    }

    #[test]
    fn decodes_a_generated_qr_at_several_resolutions() {
        for size in [128, 200, 320] {
            let (luma, width, height) = encode(&payload(), size);
            let text = decode_luma(&luma, width, height).expect("decode");
            let qr = parse_payload(&text).expect("parse");
            assert_eq!(qr.no, "GIB2026000000001", "size {size}");
        }
    }

    #[test]
    fn decodes_through_the_rgba_path() {
        let (luma, width, height) = encode(&payload(), 200);
        // Grayscale -> RGBA with an opaque alpha channel.
        let mut rgba = Vec::with_capacity(luma.len() * 4);
        for value in &luma {
            rgba.extend_from_slice(&[*value, *value, *value, 255]);
        }
        let text = decode_rgba(&rgba, width, height).expect("decode rgba");
        assert_eq!(parse_payload(&text).unwrap().no, "GIB2026000000001");
    }

    #[test]
    fn a_blank_image_yields_no_qr() {
        let luma = vec![255u8; 200 * 200];
        assert!(decode_luma(&luma, 200, 200).is_none());
    }

    #[test]
    fn validates_vkn_and_tckn_checksums() {
        assert!(vkn_valid("1000036109"));
        assert!(vkn_valid("1234567890"));
        assert!(!vkn_valid("4444444444"));
        assert!(!vkn_valid("123456789"));
        assert!(!vkn_valid("12345678901"));
        // A TCKN with a valid check digit.
        assert!(tckn_valid("10000000146"));
        assert!(!tckn_valid("10000000147"));
        assert!(!tckn_valid("00000000000"));
        // `odd * 7 - even` is negative here (5 * 7 - 36), which used to panic in
        // debug and wrap in release.
        assert!(tckn_valid("19191919190"));
        assert!(!tckn_valid("19191919191"));
        assert!(tax_id_valid("1000036109"));
    }
}
