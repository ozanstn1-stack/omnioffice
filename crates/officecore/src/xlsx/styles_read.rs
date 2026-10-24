//! Import of `xl/styles.xml` into the model's cell styles and differential formats.

use super::*;

/// The style catalogue resolved from `xl/styles.xml`, indexed by cell xf.
#[derive(Debug, Clone, Default)]
pub(super) struct ImportedStyles {
    pub(super) cell_styles: Vec<CellStyle>,
    /// `dxf` fill and font colours, indexed by `dxfId` in conditional rules.
    pub(super) dxf_fills: Vec<Option<String>>,
    pub(super) dxf_colors: Vec<Option<String>>,
    /// Bold and italic flags of the same `dxf` entries.
    pub(super) dxf_bold: Vec<bool>,
    pub(super) dxf_italic: Vec<bool>,
}

#[derive(Debug, Clone, Default)]
struct ImportFont {
    name: Option<String>,
    size_pt: Option<f64>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ImportedXf {
    number_format: String,
    font: Option<ImportFont>,
    fill: Option<String>,
    borders: CellBorders,
    align: String,
    valign: String,
    wrap: bool,
    rotation: i32,
}

// ---------------------------------------------------------------------------
// Import: styles.xml
// ---------------------------------------------------------------------------

pub(super) fn parse_styles(xml: &str) -> OfficeResult<ImportedStyles> {
    let root = parse_xml(xml)?;
    let mut custom: BTreeMap<u32, String> = BTreeMap::new();
    if let Some(node) = root.child("numFmts") {
        for entry in node.children_of("numFmt") {
            if custom.len() >= 4_096 {
                break;
            }
            if let (Some(id), Some(code)) = (parse_u32_attr(entry, "numFmtId"), entry.attr("formatCode")) {
                custom.insert(id, code.to_string());
            }
        }
    }
    let fonts: Vec<ImportFont> = root
        .child("fonts")
        .map(|node| node.children_of("font").into_iter().map(parse_font).collect())
        .unwrap_or_default();
    let fills: Vec<Option<String>> = root
        .child("fills")
        .map(|node| node.children_of("fill").into_iter().map(parse_fill).collect())
        .unwrap_or_default();
    let borders: Vec<CellBorders> = root
        .child("borders")
        .map(|node| node.children_of("border").into_iter().map(parse_border).collect())
        .unwrap_or_default();
    let cell_styles: Vec<CellStyle> = root
        .child("cellXfs")
        .map(|node| {
            node.children_of("xf")
                .into_iter()
                .map(|xf| style_from_xf(&parse_xf(xf, &fonts, &fills, &borders, &custom)))
                .collect()
        })
        .unwrap_or_default();
    let mut dxf_fills = Vec::new();
    let mut dxf_colors = Vec::new();
    let mut dxf_bold = Vec::new();
    let mut dxf_italic = Vec::new();
    if let Some(node) = root.child("dxfs") {
        for dxf in node.children_of("dxf") {
            dxf_fills.push(dxf.child("fill").and_then(parse_fill));
            dxf_colors.push(dxf.child("font").and_then(|font| font.child("color")).and_then(color_of));
            dxf_bold.push(dxf.child("font").and_then(|font| font.child("b")).map(font_flag).unwrap_or(false));
            dxf_italic.push(dxf.child("font").and_then(|font| font.child("i")).map(font_flag).unwrap_or(false));
        }
    }
    Ok(ImportedStyles { cell_styles, dxf_fills, dxf_colors, dxf_bold, dxf_italic })
}

fn parse_xf(
    xf: &XmlNode,
    fonts: &[ImportFont],
    fills: &[Option<String>],
    borders: &[CellBorders],
    custom: &BTreeMap<u32, String>,
) -> ImportedXf {
    let font_id = parse_u32_attr(xf, "fontId").unwrap_or(0) as usize;
    let fill_id = parse_u32_attr(xf, "fillId").unwrap_or(0) as usize;
    let border_id = parse_u32_attr(xf, "borderId").unwrap_or(0) as usize;
    let mut out = ImportedXf {
        number_format: imported_number_format(parse_u32_attr(xf, "numFmtId").unwrap_or(0), custom),
        font: fonts.get(font_id).cloned(),
        fill: fills.get(fill_id).cloned().flatten(),
        borders: borders.get(border_id).cloned().unwrap_or_default(),
        ..Default::default()
    };
    if let Some(alignment) = xf.child("alignment") {
        if let Some(horizontal) = alignment.attr("horizontal") {
            if horizontal != "general" {
                out.align = horizontal.to_string();
            }
        }
        if let Some(vertical) = alignment.attr("vertical") {
            out.valign = vertical.to_string();
        }
        out.wrap = attr_on(alignment, "wrapText");
        out.rotation = alignment.attr("textRotation").and_then(|value| value.trim().parse::<i32>().ok()).unwrap_or(0);
    }
    out
}

fn parse_font(font: &XmlNode) -> ImportFont {
    let mut out = ImportFont::default();
    for child in &font.children {
        match child.local_name() {
            "b" => out.bold = font_flag(child),
            "i" => out.italic = font_flag(child),
            "u" => out.underline = child.attr("val").map(|value| value != "none").unwrap_or(true),
            "strike" => out.strike = font_flag(child),
            "sz" => out.size_pt = child.attr("val").and_then(|value| value.trim().parse::<f64>().ok()),
            "name" => out.name = child.attr("val").map(str::to_string),
            "color" => out.color = color_of(child),
            _ => {}
        }
    }
    out
}

/// A `<b/>`/`<i/>`-style flag: present without `val` means on.
fn font_flag(node: &XmlNode) -> bool {
    node.attr("val").map(|value| !matches!(value.trim(), "0" | "false" | "off")).unwrap_or(true)
}

fn parse_fill(fill: &XmlNode) -> Option<String> {
    let pattern = fill.child("patternFill")?;
    if pattern.attr("patternType").map(|kind| kind.trim() == "none").unwrap_or(false) {
        return None;
    }
    pattern.child("fgColor").and_then(color_of).or_else(|| pattern.child("bgColor").and_then(color_of))
}

fn parse_border(border: &XmlNode) -> CellBorders {
    let side = |name: &str| -> Option<BorderStyle> {
        let node = border.child(name)?;
        let style = node.attr("style")?.trim();
        if style.is_empty() || style == "none" {
            return None;
        }
        let color = node.child("color").and_then(color_of).unwrap_or_else(|| "#000000".into());
        Some(BorderStyle { style: style.to_string(), color })
    };
    CellBorders { top: side("top"), right: side("right"), bottom: side("bottom"), left: side("left") }
}

/// `rgb="FFRRGGBB"` (or `RRGGBB`) to the model's `#RRGGBB`. Theme and indexed
/// colours have no portable value in the model and resolve to `None`.
pub(super) fn color_of(node: &XmlNode) -> Option<String> {
    if attr_on(node, "auto") {
        return None;
    }
    let rgb = node.attr("rgb")?;
    let hex: String = rgb.chars().filter(|character| character.is_ascii_hexdigit()).collect();
    match hex.len() {
        8 => normalize_hex(&hex[2..]),
        6 => normalize_hex(&hex),
        _ => None,
    }
}

fn style_from_xf(xf: &ImportedXf) -> CellStyle {
    let mut style = CellStyle {
        number_format: xf.number_format.clone(),
        fill: xf.fill.clone(),
        align: xf.align.clone(),
        valign: xf.valign.clone(),
        wrap: xf.wrap,
        rotation: xf.rotation,
        borders: xf.borders.clone(),
        ..Default::default()
    };
    if let Some(font) = &xf.font {
        style.font = font.name.clone();
        style.size_pt = font.size_pt;
        style.bold = font.bold;
        style.italic = font.italic;
        style.underline = font.underline;
        style.strike = font.strike;
        style.color = font.color.clone();
    }
    style
}

/// True when a style-only cell is worth keeping in the model. `General` is the
/// model's default number format, so an xf that only says `General` must not
/// materialise empty cells.
pub(super) fn style_worth_a_cell(style: &CellStyle) -> bool {
    let mut probe = style.clone();
    if probe.number_format == "General" {
        probe.number_format.clear();
    }
    probe != CellStyle::default()
}

/// The built-in number formats from ECMA-376; ids without a portable code map
/// to `General` rather than an invented format string.
fn builtin_number_format(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "$#,##0_);($#,##0)",
        6 => "$#,##0_);[Red]($#,##0)",
        7 => "$#,##0.00_);($#,##0.00)",
        8 => "$#,##0.00_);[Red]($#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

fn imported_number_format(id: u32, custom: &BTreeMap<u32, String>) -> String {
    if let Some(code) = custom.get(&id) {
        return code.clone();
    }
    builtin_number_format(id).unwrap_or("General").to_string()
}
