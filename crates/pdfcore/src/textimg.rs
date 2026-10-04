//! Renders text into RGBA bitmaps with a bundled OFL font. This is how text
//! watermarks and text stamps support full Unicode (Turkish, accents, CJK
//! fall back through the font) without embedding font programs in the PDF.

use crate::docutil::RawImage;
use crate::error::{PdfError, PdfResult};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

const FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/PT_Sans-Web-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/PT_Sans-Web-Bold.ttf");

#[derive(Debug, Clone)]
pub struct TextRenderRequest {
    pub text: String,
    /// Font size in pixels (the caller chooses a pixel scale tied to output
    /// resolution so strokes stay crisp).
    pub size_px: f32,
    pub color: [u8; 4],
    pub bold: bool,
    /// Multiplier applied to the natural line height.
    pub line_spacing: f32,
    pub padding_px: u32,
    /// When set, lines longer than this wrap at word boundaries.
    pub wrap_width_px: Option<f32>,
}

impl Default for TextRenderRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            size_px: 48.0,
            color: [0, 0, 0, 255],
            bold: false,
            line_spacing: 1.2,
            padding_px: 4,
            wrap_width_px: None,
        }
    }
}

fn font_bytes(bold: bool) -> &'static [u8] {
    if bold {
        FONT_BOLD
    } else {
        FONT_REGULAR
    }
}

fn wrap_text(font: &FontRef<'_>, size: f32, text: &str, wrap_width: Option<f32>) -> Vec<String> {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        if wrap_width.is_none() {
            lines.push(raw_line.to_string());
            continue;
        }
        let max_w = wrap_width.unwrap();
        let mut current = String::new();
        let mut current_w = 0.0f32;
        for word in raw_line.split_whitespace() {
            let word_w: f32 = word.chars().map(|c| scaled.h_advance(font.glyph_id(c))).sum::<f32>()
                + scaled.h_advance(font.glyph_id(' '));
            if current_w > 0.0 && current_w + word_w > max_w {
                lines.push(current.trim_end().to_string());
                current = String::new();
                current_w = 0.0;
            }
            current.push_str(word);
            current.push(' ');
            current_w += word_w;
        }
        lines.push(current.trim_end().to_string());
    }
    lines
}

fn measure_line(font: &FontRef<'_>, size: f32, line: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in line.chars() {
        let id = font.glyph_id(c);
        if let Some(prev_id) = prev {
            width += scaled.kern(prev_id, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Renders text into an RGBA buffer with a transparent background.
pub fn render_text(request: &TextRenderRequest) -> PdfResult<RawImage> {
    if request.text.is_empty() {
        return Err(PdfError::InvalidInput("text is empty".into()));
    }
    let font = FontRef::try_from_slice(font_bytes(request.bold))
        .map_err(|e| PdfError::Internal(format!("bundled font error: {e}")))?;
    let size = request.size_px.max(1.0);
    let scaled = font.as_scaled(PxScale::from(size));
    let lines = wrap_text(&font, size, &request.text, request.wrap_width_px);
    let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap()) * request.line_spacing;
    let text_width = lines.iter().map(|l| measure_line(&font, size, l)).fold(0.0f32, f32::max);
    let pad = request.padding_px as f32;
    let width = (text_width.ceil() + pad * 2.0).max(1.0) as u32;
    let ascender = scaled.ascent();
    let height = ((line_height * lines.len() as f32).ceil() + pad * 2.0).max(1.0) as u32;

    let mut buffer = vec![0u8; (width * height * 4) as usize];
    let color = request.color;

    for (line_index, line) in lines.iter().enumerate() {
        let baseline = pad + ascender + line_index as f32 * line_height;
        let mut caret_x = pad;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for ch in line.chars() {
            let glyph_id = font.glyph_id(ch);
            if let Some(prev_id) = prev {
                caret_x += scaled.kern(prev_id, glyph_id);
            }
            let glyph: ab_glyph::Glyph = glyph_id.with_scale_and_position(size, ab_glyph::point(caret_x, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                        return;
                    }
                    let index = ((py as u32 * width + px as u32) * 4) as usize;
                    blend_pixel(&mut buffer[index..index + 4], color, coverage);
                });
            }
            caret_x += scaled.h_advance(glyph_id);
            prev = Some(glyph_id);
        }
    }

    Ok(RawImage { width, height, rgba: buffer })
}

fn blend_pixel(dst: &mut [u8], color: [u8; 4], coverage: f32) {
    let src_a = coverage * (color[3] as f32 / 255.0);
    if src_a <= 0.0 {
        return;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }
    for i in 0..3 {
        let src = color[i] as f32 / 255.0;
        let d = dst[i] as f32 / 255.0;
        let out = (src * src_a + d * dst_a * (1.0 - src_a)) / out_a;
        dst[i] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Measures the pixel size a request would produce without rendering it.
pub fn measure(request: &TextRenderRequest) -> PdfResult<(u32, u32)> {
    let font = FontRef::try_from_slice(font_bytes(request.bold))
        .map_err(|e| PdfError::Internal(format!("bundled font error: {e}")))?;
    let size = request.size_px.max(1.0);
    let scaled = font.as_scaled(PxScale::from(size));
    let lines = wrap_text(&font, size, &request.text, request.wrap_width_px);
    let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap()) * request.line_spacing;
    let text_width = lines.iter().map(|l| measure_line(&font, size, l)).fold(0.0f32, f32::max);
    let pad = request.padding_px as f32;
    Ok((
        (text_width.ceil() + pad * 2.0).max(1.0) as u32,
        ((line_height * lines.len() as f32).ceil() + pad * 2.0).max(1.0) as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unicode_text() {
        let req = TextRenderRequest {
            text: "Gizli / Confidential şğüöç\nLine two".into(),
            size_px: 32.0,
            color: [255, 0, 0, 128],
            ..Default::default()
        };
        let img = render_text(&req).unwrap();
        assert!(img.width > 50);
        assert!(img.height > 30);
        // some pixels must be non-transparent
        assert!(img.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    }

    #[test]
    fn empty_text_is_rejected() {
        assert!(render_text(&TextRenderRequest::default()).is_err());
    }
}
