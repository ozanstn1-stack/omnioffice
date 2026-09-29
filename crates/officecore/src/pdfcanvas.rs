//! Vector PDF output primitives for the office modules.
//!
//! Text uses a bundled OFL font embedded as a CID-keyed Type0 font, so exported
//! PDFs have selectable/searchable text and work without any installed fonts.
//! The canvas works in top-left coordinates (like the editors) and converts to
//! PDF's bottom-left space internally.

use ab_glyph::{Font, FontVec, GlyphId};
use lopdf::{dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

const FONT_REGULAR: &[u8] = include_bytes!("../../pdfcore/assets/fonts/PT_Sans-Web-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../pdfcore/assets/fonts/PT_Sans-Web-Bold.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    pub fn to_pdf(self) -> (f64, f64, f64) {
        (self.0 as f64 / 255.0, self.1 as f64 / 255.0, self.2 as f64 / 255.0)
    }

    pub fn from_hex(value: &str) -> Rgb {
        parse_hex(value).unwrap_or(Rgb::BLACK)
    }
}

pub fn parse_hex(value: &str) -> Option<Rgb> {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() == 3 {
        let mut expanded = String::new();
        for ch in trimmed.chars() {
            expanded.push(ch);
            expanded.push(ch);
        }
        return parse_hex(&expanded);
    }
    if trimmed.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&trimmed[0..2], 16).ok()?;
    let green = u8::from_str_radix(&trimmed[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&trimmed[4..6], 16).ok()?;
    Some(Rgb(red, green, blue))
}

/// One embedded font program, tracked as CIDs are assigned.
pub struct FontFace {
    font: FontVec,
    units_per_em: f32,
    ascent_em: f64,
    descent_em: f64,
    line_gap_em: f64,
    bold: bool,
    cid_of: RefCell<HashMap<char, u16>>,
    char_of: RefCell<Vec<char>>,
    widths: RefCell<Vec<f64>>,
}

impl FontFace {
    pub fn load(bold: bool) -> Self {
        let bytes = if bold { FONT_BOLD } else { FONT_REGULAR };
        let font = FontVec::try_from_vec(bytes.to_vec()).expect("bundled font must parse");
        let units = font.units_per_em().unwrap_or(1000.0);
        let ascent = font.ascent_unscaled() as f64 / units as f64;
        let descent = font.descent_unscaled() as f64 / units as f64;
        let gap = font.line_gap_unscaled() as f64 / units as f64;
        Self {
            font,
            units_per_em: units,
            ascent_em: ascent,
            descent_em: descent,
            line_gap_em: gap,
            bold,
            cid_of: RefCell::new(HashMap::new()),
            char_of: RefCell::new(Vec::new()),
            widths: RefCell::new(Vec::new()),
        }
    }

    pub fn is_bold(&self) -> bool {
        self.bold
    }

    pub fn ascent_pt(&self, size_pt: f64) -> f64 {
        self.ascent_em * size_pt
    }

    pub fn descent_pt(&self, size_pt: f64) -> f64 {
        -self.descent_em * size_pt
    }

    /// Natural line height at a given size and spacing multiplier.
    pub fn line_height_pt(&self, size_pt: f64, spacing: f64) -> f64 {
        (self.ascent_em - self.descent_em + self.line_gap_em) * size_pt * spacing.max(0.5)
    }

    pub fn advance_pt(&self, text: &str, size_pt: f64) -> f64 {
        let scale = size_pt / self.units_per_em as f64;
        text.chars().map(|ch| self.font.h_advance_unscaled(self.glyph(ch)) as f64 * scale).sum()
    }

    fn glyph(&self, ch: char) -> GlyphId {
        let glyph = self.font.glyph_id(ch);
        if glyph.0 == 0 && ch != '\u{0}' {
            self.font.glyph_id('?')
        } else {
            glyph
        }
    }

    fn cid(&self, ch: char) -> u16 {
        let mut map = self.cid_of.borrow_mut();
        if let Some(existing) = map.get(&ch) {
            return *existing;
        }
        let next = map.len() as u16;
        map.insert(ch, next);
        let mut chars = self.char_of.borrow_mut();
        chars.push(ch);
        let advance = self.font.h_advance_unscaled(self.glyph(ch)) as f64 / self.units_per_em as f64 * 1000.0;
        self.widths.borrow_mut().push(advance);
        next
    }

    /// Hex-encoded CID string for a `Tj` operator.
    pub fn encode(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() * 4);
        for ch in text.chars() {
            let cid = self.cid(ch);
            out.push_str(&format!("{cid:04X}"));
        }
        out
    }

    fn cmap(&self) -> String {
        let chars = self.char_of.borrow();
        let mut out = String::new();
        out.push_str("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
        let mut index = 0;
        while index < chars.len() {
            let chunk = (chars.len() - index).min(100);
            out.push_str(&format!("{chunk} begincidchar\n"));
            for offset in 0..chunk {
                let ch = chars[index + offset];
                let code = ch as u32;
                let hex = if code > 0xFFFF {
                    let value = code - 0x10000;
                    format!("{:04X}{:04X}", 0xD800 + (value >> 10) as u16, 0xDC00 + (value & 0x3FF) as u16)
                } else {
                    format!("{code:04X}")
                };
                out.push_str(&format!("<{:04X}> <{hex}>\n", index + offset));
            }
            out.push_str("endcidchar\n");
            index += chunk;
        }
        out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        out
    }

    fn build_font_objects(&self, doc: &mut Document) -> lopdf::ObjectId {
        let file = if self.bold { FONT_BOLD } else { FONT_REGULAR };
        let file_id = doc.add_object(Stream::new(
            dictionary! { "Length1" => file.len() as i64 },
            file.to_vec(),
        ).with_compression(true));

        let scale = 1000.0 / self.units_per_em as f64;
        let ascent = self.ascent_em * 1000.0;
        let descent = self.descent_em * 1000.0;
        let descriptor_id = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => if self.bold { "PT-Sans-Bold" } else { "PT-Sans" },
            "Flags" => 32,
            "FontBBox" => vec![(-200).into(), (descent - 100.0).into(), (1000.0).into(), (ascent + 100.0).into()],
            "ItalicAngle" => 0,
            "Ascent" => ascent,
            "Descent" => descent,
            "CapHeight" => ascent * 0.7,
            "StemV" => if self.bold { 120 } else { 80 },
            "FontFile2" => Object::Reference(file_id),
        });

        let widths = self.widths.borrow();
        let mut w_array: Vec<Object> = Vec::new();
        for (index, width) in widths.iter().enumerate() {
            w_array.push(Object::Integer(index as i64 + 1));
            w_array.push(Object::Array(vec![Object::Real(*width as f32)]));
        }
        let _ = scale;

        let cid_font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => if self.bold { "PT-Sans-Bold" } else { "PT-Sans" },
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => 0,
            },
            "FontDescriptor" => Object::Reference(descriptor_id),
            "DW" => 1000,
            "W" => Object::Array(w_array),
            "CIDToGIDMap" => "Identity",
        });

        let cmap_id = doc.add_object(Stream::new(dictionary! {}, self.cmap().into_bytes()).with_compression(true));

        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => if self.bold { "PT-Sans-Bold" } else { "PT-Sans" },
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Reference(cid_font_id)],
            "ToUnicode" => Object::Reference(cmap_id),
        })
    }
}

pub struct FontSet {
    pub regular: FontFace,
    pub bold: FontFace,
}

impl Default for FontSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FontSet {
    pub fn new() -> Self {
        Self { regular: FontFace::load(false), bold: FontFace::load(true) }
    }

    pub fn pick(&self, bold: bool) -> &FontFace {
        if bold {
            &self.bold
        } else {
            &self.regular
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub color: Rgb,
    pub size_pt: f64,
    pub underline: bool,
    pub strike: bool,
    pub highlight: Option<Rgb>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { bold: false, italic: false, color: Rgb::BLACK, size_pt: 11.0, underline: false, strike: false, highlight: None }
    }
}

#[derive(Debug, Clone)]
pub struct PageImage {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// JPEG bytes that can be embedded directly with DCTDecode.
    pub jpeg: Option<Vec<u8>>,
    /// Raw RGB samples (flate-compressed by the writer) when not JPEG.
    pub rgb: Option<Vec<u8>>,
    /// Raw grayscale samples.
    pub gray: Option<Vec<u8>>,
    /// Optional alpha plane (8-bit) for an SMask.
    pub alpha: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct LinkAnnot {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub url: String,
}

#[derive(Default, Clone)]
pub struct BuiltPage {
    pub width: f64,
    pub height: f64,
    pub ops: String,
    pub images: Vec<PageImage>,
    pub links: Vec<LinkAnnot>,
}

/// Drawing surface for one page (top-left origin, y grows downwards).
pub struct Canvas<'a> {
    pub page: BuiltPage,
    fonts: &'a FontSet,
    image_counter: usize,
}

impl<'a> Canvas<'a> {
    pub fn new(width: f64, height: f64, fonts: &'a FontSet) -> Self {
        Self {
            page: BuiltPage { width, height, ..Default::default() },
            fonts,
            image_counter: 0,
        }
    }

    pub fn finish(self) -> BuiltPage {
        self.page
    }

    fn flip(&self, y: f64) -> f64 {
        self.page.height - y
    }

    pub fn save(&mut self) {
        self.page.ops.push_str("q\n");
    }

    pub fn restore(&mut self) {
        self.page.ops.push_str("Q\n");
    }

    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgb, opacity: f64) {
        let (r, g, b) = color.to_pdf();
        if opacity < 1.0 {
            self.page.ops.push_str("q\n");
            self.page.ops.push_str(&format!("/GS{:.0} gs\n", (opacity.clamp(0.0, 1.0)) * 100.0));
        }
        self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\n"));
        self.page.ops.push_str(&format!("{:.2} {:.2} {:.2} {:.2} re\nf\n", x, self.flip(y + h), w, h));
        if opacity < 1.0 {
            self.page.ops.push_str("Q\n");
        }
    }

    pub fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgb, width_pt: f64) {
        let (r, g, b) = color.to_pdf();
        self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} RG\n{width_pt:.2} w\n"));
        self.page.ops.push_str(&format!("{:.2} {:.2} {:.2} {:.2} re\nS\n", x, self.flip(y + h), w, h));
    }

    pub fn rounded_rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64, fill: Option<Rgb>, stroke: Option<(Rgb, f64)>) {
        let radius = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        let top = self.flip(y);
        let bottom = self.flip(y + h);
        let k = 0.5523 * radius;
        self.page.ops.push_str(&format!(
            "{:.2} {:.2} m\n{:.2} {:.2} l\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} l\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} l\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} l\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\nh\n",
            x + radius, top,
            x + w - radius, top,
            x + w - radius + k, top, x + w, top - radius + k, x + w, top - radius,
            x + w, bottom + radius,
            x + w, bottom + radius - k, x + w - radius + k, bottom, x + w - radius, bottom,
            x + radius, bottom,
            x + radius - k, bottom, x, bottom + radius - k, x, bottom + radius,
            x, top - radius,
            x, top - radius + k, x + radius - k, top, x + radius, top,
        ));
        if let Some(color) = fill {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\nf\n"));
        }
        if let Some((color, width)) = stroke {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} RG\n{width:.2} w\nS\n"));
        }
    }

    pub fn ellipse(&mut self, x: f64, y: f64, w: f64, h: f64, fill: Option<Rgb>, stroke: Option<(Rgb, f64)>) {
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        let rx = w / 2.0;
        let ry = h / 2.0;
        let k = 0.5523;
        let top = self.flip(cy - ry);
        let bottom = self.flip(cy + ry);
        let mid = self.flip(cy);
        self.page.ops.push_str(&format!(
            "{:.2} {:.2} m\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\nh\n",
            cx, top,
            cx + k * rx, top, cx + rx, mid - k * ry, cx + rx, mid,
            cx + rx, mid + k * ry, cx + k * rx, bottom, cx, bottom,
            cx - k * rx, bottom, cx - rx, mid + k * ry, cx - rx, mid,
            cx - rx, mid - k * ry, cx - k * rx, top, cx, top,
        ));
        if let Some(color) = fill {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\nf\n"));
        }
        if let Some((color, width)) = stroke {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} RG\n{width:.2} w\nS\n"));
        }
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: Rgb, width_pt: f64, dash: &str) {
        let (r, g, b) = color.to_pdf();
        self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} RG\n{width_pt:.2} w\n"));
        match dash {
            "dashed" => self.page.ops.push_str("[6 4] 0 d\n"),
            "dotted" => self.page.ops.push_str("[1 3] 0 d\n"),
            _ => self.page.ops.push_str("[] 0 d\n"),
        }
        self.page.ops.push_str(&format!("{:.2} {:.2} m\n{:.2} {:.2} l\nS\n", x1, self.flip(y1), x2, self.flip(y2)));
        self.page.ops.push_str("[] 0 d\n");
    }

    pub fn polygon(&mut self, points: &[(f64, f64)], fill: Option<Rgb>, stroke: Option<(Rgb, f64)>) {
        if points.is_empty() {
            return;
        }
        for (index, (x, y)) in points.iter().enumerate() {
            let operator = if index == 0 { "m" } else { "l" };
            self.page.ops.push_str(&format!("{:.2} {:.2} {operator}\n", x, self.flip(*y)));
        }
        self.page.ops.push_str("h\n");
        if let Some(color) = fill {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\nf\n"));
        }
        if let Some((color, width)) = stroke {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} RG\n{width:.2} w\nS\n"));
        }
    }

    /// Cubic Bezier path in top-left coordinates.
    pub fn bezier_path(&mut self, start: (f64, f64), curves: &[(f64, f64, f64, f64, f64, f64)], fill: Option<Rgb>, stroke: Option<(Rgb, f64)>) {
        self.page.ops.push_str(&format!("{:.2} {:.2} m\n", start.0, self.flip(start.1)));
        for (c1x, c1y, c2x, c2y, x, y) in curves {
            self.page.ops.push_str(&format!(
                "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n",
                c1x,
                self.flip(*c1y),
                c2x,
                self.flip(*c2y),
                x,
                self.flip(*y)
            ));
        }
        if fill.is_some() {
            self.page.ops.push_str("h\n");
        }
        if let Some(color) = fill {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\nf\n"));
        }
        if let Some((color, width)) = stroke {
            let (r, g, b) = color.to_pdf();
            self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} RG\n{width:.2} w\nS\n"));
        }
    }

    /// Draws text with the baseline at `baseline_y` (top-left coordinates).
    pub fn text(&mut self, x: f64, baseline_y: f64, text: &str, style: &TextStyle) {
        if text.is_empty() {
            return;
        }
        let face = self.fonts.pick(style.bold);
        let encoded = face.encode(text);
        if let Some(highlight) = style.highlight {
            let width = face.advance_pt(text, style.size_pt);
            let height = style.size_pt * 1.25;
            self.fill_rect(x - 0.5, baseline_y - style.size_pt * 1.0, width + 1.0, height, highlight, 0.75);
        }
        let (r, g, b) = style.color.to_pdf();
        let shear = if style.italic { 0.21 } else { 0.0 };
        self.page.ops.push_str("BT\n");
        self.page.ops.push_str(&format!("/{} {:.2} Tf\n", if style.bold { "F2" } else { "F1" }, style.size_pt));
        self.page.ops.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\n"));
        self.page.ops.push_str(&format!("1 0 {shear:.2} 1 {:.2} {:.2} Tm\n", x, self.flip(baseline_y)));
        self.page.ops.push_str(&format!("<{encoded}> Tj\n"));
        self.page.ops.push_str("ET\n");
        if style.underline {
            let width = face.advance_pt(text, style.size_pt);
            let y = baseline_y + style.size_pt * 0.12;
            self.line(x, y, x + width, y, style.color, (style.size_pt * 0.06).max(0.5), "solid");
        }
        if style.strike {
            let width = face.advance_pt(text, style.size_pt);
            let y = baseline_y - style.size_pt * 0.28;
            self.line(x, y, x + width, y, style.color, (style.size_pt * 0.06).max(0.5), "solid");
        }
    }

    pub fn measure(&self, text: &str, style: &TextStyle) -> f64 {
        self.fonts.pick(style.bold).advance_pt(text, style.size_pt)
    }

    /// Places an image so that its top-left corner is at (x, y).
    pub fn image(&mut self, x: f64, y: f64, w: f64, h: f64, bytes: &[u8], mime: &str) -> bool {
        let prepared = match prepare_image(bytes, mime, self.image_counter) {
            Some(value) => value,
            None => return false,
        };
        let name = prepared.name.clone();
        self.image_counter += 1;
        self.page.images.push(prepared);
        self.page.ops.push_str("q\n");
        self.page.ops
            .push_str(&format!("{:.2} 0 0 {:.2} {:.2} {:.2} cm\n", w, h, x, self.flip(y + h)));
        self.page.ops.push_str(&format!("/{name} Do\nQ\n"));
        true
    }

    pub fn rotated_image(&mut self, cx: f64, cy: f64, w: f64, h: f64, rotation_deg: f64, bytes: &[u8], mime: &str) -> bool {
        let prepared = match prepare_image(bytes, mime, self.image_counter) {
            Some(value) => value,
            None => return false,
        };
        let name = prepared.name.clone();
        self.image_counter += 1;
        self.page.images.push(prepared);
        let angle = rotation_deg.to_radians();
        let (sin, cos) = angle.sin_cos();
        let px = cx;
        let py = self.flip(cy);
        self.page.ops.push_str("q\n");
        self.page.ops.push_str(&format!(
            "{:.4} {:.4} {:.4} {:.4} {:.2} {:.2} cm\n",
            w * cos,
            w * sin,
            -h * sin,
            h * cos,
            px - (w * cos - h * sin) / 2.0,
            py - (w * sin + h * cos) / 2.0
        ));
        self.page.ops.push_str(&format!("/{name} Do\nQ\n"));
        true
    }

    pub fn link(&mut self, x: f64, y: f64, w: f64, h: f64, url: &str) {
        self.page.links.push(LinkAnnot { x, y, w, h, url: url.to_string() });
    }

    /// Draws a check box or marker glyph (used by Impress/Calc renderers).
    pub fn marker(&mut self, x: f64, y: f64, size: f64, color: Rgb, symbol: &str) {
        let style = TextStyle { color, size_pt: size, ..Default::default() };
        self.text(x, y, symbol, &style);
    }
}

fn prepare_image(bytes: &[u8], mime: &str, index: usize) -> Option<PageImage> {
    if bytes.is_empty() {
        return None;
    }
    let name = format!("Im{index}");
    let is_jpeg = mime.contains("jpeg") || mime.contains("jpg") || bytes.starts_with(&[0xFF, 0xD8]);
    if is_jpeg {
        if let Some((width, height, components)) = jpeg_info(bytes) {
            if components == 3 || components == 1 {
                return Some(PageImage {
                    name,
                    width,
                    height,
                    jpeg: Some(bytes.to_vec()),
                    rgb: None,
                    gray: None,
                    alpha: None,
                });
            }
        }
    }
    let decoded = image::load_from_memory(bytes).ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    let mut alpha = Vec::with_capacity((width * height) as usize);
    let mut has_alpha = false;
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[0..3]);
        alpha.push(pixel.0[3]);
        if pixel.0[3] != 255 {
            has_alpha = true;
        }
    }
    Some(PageImage {
        name,
        width,
        height,
        jpeg: None,
        rgb: Some(rgb),
        gray: None,
        alpha: if has_alpha { Some(alpha) } else { None },
    })
}

fn jpeg_info(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut at = 2usize;
    while at + 9 < bytes.len() {
        if bytes[at] != 0xFF {
            at += 1;
            continue;
        }
        let marker = bytes[at + 1];
        if (0xC0..=0xC3).contains(&marker) || (0xC5..=0xC7).contains(&marker) || (0xC9..=0xCB).contains(&marker) || (0xCD..=0xCF).contains(&marker) {
            let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]) as u32;
            let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]) as u32;
            let components = bytes[at + 9];
            return Some((width, height, components));
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            at += 2;
            continue;
        }
        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        at += 2 + length;
    }
    None
}

/// Serializes the built pages into a PDF document.
pub fn write_pdf(pages: &[BuiltPage], fonts: &FontSet) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    // Fonts are built after layout so the CID tables cover every used glyph.
    let regular_id = fonts.regular.build_font_objects(&mut doc);
    let bold_id = fonts.bold.build_font_objects(&mut doc);
    let regular_ref = Object::Reference(regular_id);
    let bold_ref = Object::Reference(bold_id);

    let mut page_ids = Vec::new();
    for page in pages {
        let mut font_dict = lopdf::Dictionary::new();
        font_dict.set("F1", regular_ref.clone());
        font_dict.set("F2", bold_ref.clone());

        let mut xobject_dict = lopdf::Dictionary::new();
        for image in &page.images {
            let smask_id = image.alpha.as_ref().map(|alpha| {
                doc.add_object(
                    Stream::new(
                        dictionary! {
                            "Type" => "XObject",
                            "Subtype" => "Image",
                            "Width" => image.width as i64,
                            "Height" => image.height as i64,
                            "ColorSpace" => "DeviceGray",
                            "BitsPerComponent" => 8,
                        },
                        alpha.clone(),
                    )
                    .with_compression(true),
                )
            });
            let mut dict = if image.jpeg.is_some() {
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => image.width as i64,
                    "Height" => image.height as i64,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                    "Filter" => "DCTDecode",
                }
            } else if image.gray.is_some() {
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => image.width as i64,
                    "Height" => image.height as i64,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8,
                }
            } else {
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => image.width as i64,
                    "Height" => image.height as i64,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                }
            };
            if let Some(id) = smask_id {
                dict.set("SMask", Object::Reference(id));
            }
            let payload = if let Some(jpeg) = &image.jpeg {
                jpeg.clone()
            } else if let Some(gray) = &image.gray {
                gray.clone()
            } else {
                image.rgb.clone().unwrap_or_default()
            };
            let stream = Stream::new(dict, payload);
            let stream = if image.jpeg.is_some() { stream } else { stream.with_compression(true) };
            let id = doc.add_object(stream);
            xobject_dict.set(image.name.clone(), Object::Reference(id));
        }

        let mut resources = lopdf::Dictionary::new();
        resources.set("Font", Object::Dictionary(font_dict));
        if !xobject_dict.is_empty() {
            resources.set("XObject", Object::Dictionary(xobject_dict));
        }

        let content_id = doc.add_object(Stream::new(dictionary! {}, page.ops.clone().into_bytes()).with_compression(true));

        let mut page_dict = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), page.width.into(), page.height.into()],
            "Resources" => Object::Dictionary(resources),
            "Contents" => Object::Reference(content_id),
        };
        if !page.links.is_empty() {
            let mut annots = Vec::new();
            for link in &page.links {
                annots.push(Object::Dictionary(dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Link",
                    "Rect" => vec![link.x.into(), (page.height - link.y - link.h).into(), (link.x + link.w).into(), (page.height - link.y).into()],
                    "Border" => vec![0.into(), 0.into(), 0.into()],
                    "A" => dictionary! {
                        "Type" => "Action",
                        "S" => "URI",
                        "URI" => Object::string_literal(link.url.clone()),
                    },
                }));
            }
            page_dict.set("Annots", Object::Array(annots));
        }
        page_ids.push(doc.add_object(page_dict));
    }

    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_ids.len() as i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut bytes = Vec::new();
    let _ = doc.save_to(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#ff0000"), Some(Rgb(255, 0, 0)));
        assert_eq!(parse_hex("00ff00"), Some(Rgb(0, 255, 0)));
        assert_eq!(parse_hex("#abc"), Some(Rgb(170, 187, 204)));
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn text_encoding_and_width() {
        let fonts = FontSet::new();
        let width = fonts.regular.advance_pt("Hello", 12.0);
        assert!(width > 20.0 && width < 40.0, "width was {width}");
        let encoded = fonts.regular.encode("Hi");
        assert_eq!(encoded.len(), 8);
        assert_eq!(fonts.regular.encode("Hi"), encoded);
    }

    #[test]
    fn pdf_export_smoke() {
        let fonts = FontSet::new();
        let mut canvas = Canvas::new(595.28, 841.89, &fonts);
        canvas.text(72.0, 100.0, "Merhaba dünya", &TextStyle { size_pt: 14.0, ..Default::default() });
        canvas.fill_rect(72.0, 120.0, 100.0, 20.0, Rgb(30, 90, 200), 1.0);
        let page = canvas.finish();
        let bytes = write_pdf(&[page], &fonts);
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
    }
}
