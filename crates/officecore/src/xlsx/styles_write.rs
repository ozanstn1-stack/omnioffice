//! Export of `xl/styles.xml`: the font/fill/border/number-format catalogue.

use super::*;

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

#[derive(Default)]
pub(super) struct StyleTable {
    fonts: Vec<String>,
    fills: Vec<String>,
    borders: Vec<String>,
    num_formats: Vec<(u32, String)>,
    xfs: Vec<String>,
    /// Differential formats of the conditional-formatting rules (`<dxfs>`).
    dxfs: Vec<String>,
    font_keys: BTreeMap<String, usize>,
    fill_keys: BTreeMap<String, usize>,
    border_keys: BTreeMap<String, usize>,
    xf_keys: BTreeMap<String, usize>,
}

impl StyleTable {
    pub(super) fn new() -> Self {
        let mut table = Self::default();
        // Index 0: defaults required by the spec.
        table.fonts.push("<font><sz val=\"11\"/><name val=\"Calibri\"/></font>".into());
        table.fills.push("<fill><patternFill patternType=\"none\"/></fill>".into());
        table.fills.push("<fill><patternFill patternType=\"gray125\"/></fill>".into());
        table.borders.push("<border><left/><right/><top/><bottom/><diagonal/></border>".into());
        table.xfs.push("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>".into());
        table
    }

    fn builtin_number_format(format: &str) -> Option<u32> {
        match format.trim() {
            "0" => Some(1),
            "0.00" => Some(2),
            "#,##0" => Some(3),
            "#,##0.00" => Some(4),
            "0%" => Some(9),
            "0.00%" => Some(10),
            "0.00E+00" => Some(11),
            "# ?/?" => Some(12),
            "@" => Some(49),
            _ => None,
        }
    }

    fn font_id(&mut self, style: &CellStyle) -> usize {
        let size = style.size_pt.unwrap_or(11.0);
        let color = style.color.as_deref().and_then(normalize_hex);
        let mut xml = String::from("<font>");
        if style.bold {
            xml.push_str("<b/>");
        }
        if style.italic {
            xml.push_str("<i/>");
        }
        if style.underline {
            xml.push_str("<u/>");
        }
        if style.strike {
            xml.push_str("<strike/>");
        }
        xml.push_str(&format!("<sz val=\"{size}\"/>"));
        if let Some(color) = &color {
            xml.push_str(&format!("<color rgb=\"FF{}\"/>", color.trim_start_matches('#')));
        }
        xml.push_str(&format!("<name val=\"{}\"/></font>", escape_attr(style.font.as_deref().unwrap_or("Calibri"))));
        if let Some(index) = self.font_keys.get(&xml) {
            return *index;
        }
        self.fonts.push(xml.clone());
        let index = self.fonts.len() - 1;
        self.font_keys.insert(xml, index);
        index
    }

    fn fill_id(&mut self, style: &CellStyle) -> usize {
        let Some(fill) = style.fill.as_deref().and_then(normalize_hex) else { return 0 };
        let xml = format!(
            "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{}\"/><bgColor indexed=\"64\"/></patternFill></fill>",
            fill.trim_start_matches('#')
        );
        if let Some(index) = self.fill_keys.get(&xml) {
            return *index;
        }
        self.fills.push(xml.clone());
        let index = self.fills.len() - 1;
        self.fill_keys.insert(xml, index);
        index
    }

    fn border_id(&mut self, style: &CellStyle) -> usize {
        let render = |name: &str, border: &Option<BorderStyle>| -> String {
            match border {
                Some(border) if border.style != "none" => {
                    let color = normalize_hex(&border.color).unwrap_or_else(|| "#000000".into());
                    let kind = match border.style.as_str() {
                        "thick" => "thick",
                        "dashed" => "dashed",
                        "dotted" => "dotted",
                        "double" => "double",
                        _ => "thin",
                    };
                    format!("<{name} style=\"{kind}\"><color rgb=\"FF{}\"/></{name}>", color.trim_start_matches('#'))
                }
                _ => format!("<{name}/>"),
            }
        };
        let borders = &style.borders;
        if borders.top.is_none() && borders.right.is_none() && borders.bottom.is_none() && borders.left.is_none() {
            return 0;
        }
        let xml = format!(
            "<border>{}{}{}{}<diagonal/></border>",
            render("left", &borders.left),
            render("right", &borders.right),
            render("top", &borders.top),
            render("bottom", &borders.bottom)
        );
        if let Some(index) = self.border_keys.get(&xml) {
            return *index;
        }
        self.borders.push(xml.clone());
        let index = self.borders.len() - 1;
        self.border_keys.insert(xml, index);
        index
    }

    fn number_format_id(&mut self, format: &str) -> u32 {
        let format = format.trim();
        if format.is_empty() || format.eq_ignore_ascii_case("general") {
            return 0;
        }
        if let Some(builtin) = Self::builtin_number_format(format) {
            return builtin;
        }
        if let Some((id, _)) = self.num_formats.iter().find(|(_, existing)| existing == format) {
            return *id;
        }
        let id = 164 + self.num_formats.len() as u32;
        self.num_formats.push((id, format.to_string()));
        id
    }

    pub(super) fn xf_id(&mut self, style: &CellStyle) -> usize {
        let font = self.font_id(style);
        let fill = self.fill_id(style);
        let border = self.border_id(style);
        let number_format = self.number_format_id(&style.number_format);
        let mut alignment = String::new();
        if !style.align.is_empty() && style.align != "general" {
            alignment.push_str(&format!(" horizontal=\"{}\"", style.align));
        }
        if !style.valign.is_empty() && style.valign != "bottom" {
            alignment.push_str(&format!(" vertical=\"{}\"", style.valign));
        }
        if style.wrap {
            alignment.push_str(" wrapText=\"1\"");
        }
        if style.rotation != 0 {
            alignment.push_str(&format!(" textRotation=\"{}\"", style.rotation));
        }
        let mut xml = format!("<xf numFmtId=\"{number_format}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyNumberFormat=\"1\"");
        if alignment.is_empty() {
            xml.push_str("/>");
        } else {
            xml.push_str(&format!(" applyAlignment=\"1\"><alignment{alignment}/></xf>"));
        }
        if let Some(index) = self.xf_keys.get(&xml) {
            return *index;
        }
        self.xfs.push(xml.clone());
        let index = self.xfs.len() - 1;
        self.xf_keys.insert(xml, index);
        index
    }

    /// The `<dxf>` index for a highlight rule's look. A rule without any look
    /// gets the shared highlight fill, so it still shows something.
    pub(super) fn dxf_id(&mut self, fill: Option<&str>, color: Option<&str>, bold: bool, italic: bool) -> usize {
        let fill = fill.and_then(normalize_hex);
        let color = color.and_then(normalize_hex);
        let fill = match (&fill, &color, bold || italic) {
            (None, None, false) => Some(CONDITIONAL_FILL.to_string()),
            _ => fill,
        };
        let mut xml = String::from("<dxf>");
        if bold || italic || color.is_some() {
            xml.push_str("<font>");
            if bold {
                xml.push_str("<b/>");
            }
            if italic {
                xml.push_str("<i/>");
            }
            if let Some(color) = &color {
                xml.push_str(&format!("<color rgb=\"{}\"/>", argb_of(color)));
            }
            xml.push_str("</font>");
        }
        if let Some(fill) = &fill {
            xml.push_str(&format!("<fill><patternFill><bgColor rgb=\"{}\"/></patternFill></fill>", argb_of(fill)));
        }
        xml.push_str("</dxf>");
        if let Some(index) = self.dxfs.iter().position(|existing| existing == &xml) {
            return index;
        }
        self.dxfs.push(xml);
        self.dxfs.len() - 1
    }

    pub(super) fn xml(&self) -> String {
        let mut writer = XmlWriter::new();
        writer.declaration();
        writer.open("styleSheet", &[("xmlns", "http://schemas.openxmlformats.org/spreadsheetml/2006/main")]);
        if !self.num_formats.is_empty() {
            writer.raw(&format!("<numFmts count=\"{}\">", self.num_formats.len()));
            for (id, format) in &self.num_formats {
                writer.raw(&format!("<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", escape_attr(format)));
            }
            writer.raw("</numFmts>");
        }
        writer.raw(&format!("<fonts count=\"{}\">", self.fonts.len()));
        for font in &self.fonts {
            writer.raw(font);
        }
        writer.raw("</fonts>");
        writer.raw(&format!("<fills count=\"{}\">", self.fills.len()));
        for fill in &self.fills {
            writer.raw(fill);
        }
        writer.raw("</fills>");
        writer.raw(&format!("<borders count=\"{}\">", self.borders.len()));
        for border in &self.borders {
            writer.raw(border);
        }
        writer.raw("</borders>");
        writer.raw(
            "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
        );
        writer.raw(&format!("<cellXfs count=\"{}\">", self.xfs.len()));
        for xf in &self.xfs {
            writer.raw(xf);
        }
        writer.raw("</cellXfs>");
        writer.raw("<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>");
        // Every highlight rule points at its own differential format, so the
        // colours a rule shows in the editor are the colours a spreadsheet shows
        // after the round trip.
        writer.raw(&format!("<dxfs count=\"{}\">", self.dxfs.len()));
        for dxf in &self.dxfs {
            writer.raw(dxf);
        }
        writer.raw("</dxfs>");
        writer.raw("</styleSheet>");
        writer.finish()
    }
}

/// `#RRGGBB` (or `RRGGBB`) to the `AARRGGBB` form XLSX attributes use.
pub(super) fn argb_of(color: &str) -> String {
    let hex: String = color.chars().filter(|character| character.is_ascii_hexdigit()).collect();
    match hex.len() {
        6 => format!("FF{hex}"),
        8 => hex,
        _ => "FF000000".into(),
    }
}
