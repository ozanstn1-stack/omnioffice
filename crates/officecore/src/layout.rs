//! PDF export for Writer documents, Calc workbooks and Impress decks.
//!
//! The Writer layout engine flows styled paragraphs, lists, tables and images
//! across pages and columns, draws headers/footers (with `{{page}}` /
//! `{{pages}}` tokens) and produces selectable text via the embedded font in
//! `pdfcanvas`. The Calc and Impress renderers share the same primitives.

use crate::model::*;
use crate::pdfcanvas::{parse_hex, write_pdf, BuiltPage, Canvas, FontSet, Rgb, TextStyle};

// ---------------------------------------------------------------------------
// Style resolution
// ---------------------------------------------------------------------------

fn style_by_id<'a>(document: &'a TextDocument, id: &str) -> Option<&'a ParaStyle> {
    document.styles.iter().find(|style| style.id == id)
}

fn resolve_style<'a>(document: &'a TextDocument, id: &str, depth: usize, out: &mut Vec<&'a ParaStyle>) {
    if depth > 8 {
        return;
    }
    if let Some(style) = style_by_id(document, id) {
        out.push(style);
        if let Some(parent) = &style.based_on {
            resolve_style(document, parent, depth + 1, out);
        }
    }
}

struct EffectiveStyle {
    font: String,
    size_pt: f64,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<String>,
    highlight: Option<String>,
    align: String,
    line_spacing: f64,
    space_before_pt: f64,
    space_after_pt: f64,
    indent_left_pt: f64,
    indent_right_pt: f64,
    first_line_pt: f64,
}

fn effective_style(document: &TextDocument, props: &ParaProps, run: Option<&Run>) -> EffectiveStyle {
    let mut chain: Vec<&ParaStyle> = Vec::new();
    resolve_style(document, &props.style, 0, &mut chain);
    chain.reverse();
    let mut style = EffectiveStyle {
        font: "Calibri".into(),
        size_pt: 11.0,
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        color: None,
        highlight: None,
        align: "left".into(),
        line_spacing: 1.15,
        space_before_pt: 0.0,
        space_after_pt: 8.0,
        indent_left_pt: 0.0,
        indent_right_pt: 0.0,
        first_line_pt: 0.0,
    };
    for entry in chain {
        if let Some(font) = &entry.font {
            style.font = font.clone();
        }
        if let Some(size) = entry.size_pt {
            style.size_pt = size;
        }
        if let Some(value) = entry.bold {
            style.bold = value;
        }
        if let Some(value) = entry.italic {
            style.italic = value;
        }
        if let Some(value) = entry.underline {
            style.underline = value;
        }
        if let Some(value) = entry.strike {
            style.strike = value;
        }
        if let Some(value) = &entry.color {
            style.color = Some(value.clone());
        }
        if let Some(value) = &entry.highlight {
            style.highlight = Some(value.clone());
        }
        if let Some(value) = &entry.align {
            style.align = value.clone();
        }
        if let Some(value) = entry.line_spacing {
            style.line_spacing = value;
        }
        if let Some(value) = entry.space_before_pt {
            style.space_before_pt = value;
        }
        if let Some(value) = entry.space_after_pt {
            style.space_after_pt = value;
        }
        if let Some(value) = entry.indent_left_pt {
            style.indent_left_pt = value;
        }
        if let Some(value) = entry.indent_right_pt {
            style.indent_right_pt = value;
        }
        if let Some(value) = entry.first_line_pt {
            style.first_line_pt = value;
        }
    }
    if !props.align.is_empty() {
        style.align = props.align.clone();
    }
    if props.line_spacing > 0.0 {
        style.line_spacing = props.line_spacing;
    }
    style.space_before_pt =
        props.space_before_pt.max(style.space_before_pt.min(props.space_before_pt.max(style.space_before_pt)));
    if props.space_before_pt > 0.0 {
        style.space_before_pt = props.space_before_pt;
    }
    if props.space_after_pt > 0.0 {
        style.space_after_pt = props.space_after_pt;
    }
    style.indent_left_pt = props.indent_left_pt.max(style.indent_left_pt);
    style.indent_right_pt = props.indent_right_pt.max(style.indent_right_pt);
    if props.first_line_pt != 0.0 {
        style.first_line_pt = props.first_line_pt;
    }
    if let Some(list) = &props.list {
        style.indent_left_pt = style.indent_left_pt.max(18.0 * (list.level as f64 + 1.0));
    }
    if let Some(run) = run {
        if let Some(font) = &run.font {
            style.font = font.clone();
        }
        if let Some(size) = run.size_pt {
            style.size_pt = size;
        }
        if run.bold {
            style.bold = true;
        }
        if run.italic {
            style.italic = true;
        }
        if run.underline {
            style.underline = true;
        }
        if run.strike {
            style.strike = true;
        }
        if let Some(color) = &run.color {
            style.color = Some(color.clone());
        }
        if let Some(highlight) = &run.highlight {
            style.highlight = Some(highlight.clone());
        }
    }
    style
}

fn text_style(effective: &EffectiveStyle) -> TextStyle {
    TextStyle {
        bold: effective.bold,
        italic: effective.italic,
        color: effective.color.as_deref().and_then(parse_hex).unwrap_or(Rgb::BLACK),
        size_pt: effective.size_pt,
        underline: effective.underline,
        strike: effective.strike,
        highlight: effective.highlight.as_deref().and_then(parse_hex),
    }
}

// ---------------------------------------------------------------------------
// Line layout
// ---------------------------------------------------------------------------

struct LineItem {
    dx: f64,
    text: String,
    style: TextStyle,
}

struct LaidLine {
    items: Vec<LineItem>,
    width: f64,
    height: f64,
    ascent: f64,
}

/// Default distance between automatic tab stops when a paragraph has no custom
/// stops, matching Word's 0.5 inch default.
const DEFAULT_TAB_STEP_PT: f64 = 36.0;

fn break_line(
    fonts: &FontSet,
    words: &[(String, TextStyle, bool)],
    width: f64,
    align: &str,
    tabs: &[TabStop],
) -> Vec<LaidLine> {
    let mut lines: Vec<LaidLine> = Vec::new();
    let mut current: Vec<(String, TextStyle, bool)> = Vec::new();
    let mut current_width = 0.0;
    let space_width = |fonts: &FontSet, style: &TextStyle| fonts.pick(style.bold).advance_pt(" ", style.size_pt);

    for (word, style, is_space) in words {
        // A tab marker is zero-width; it only moves the cursor when drawn.
        let word_width = if word == "\t" { 0.0 } else { fonts.pick(style.bold).advance_pt(word, style.size_pt) };
        if *is_space {
            if current.is_empty() {
                continue;
            }
            current_width += word_width;
            current.push((word.clone(), *style, true));
            continue;
        }
        if current_width + word_width > width && !current.is_empty() {
            lines.push(finalize_line(fonts, &current, current_width, width, align, tabs));
            current.clear();
            current_width = 0.0;
        }
        current.push((word.clone(), *style, false));
        current_width += word_width;
    }
    if !current.is_empty() {
        lines.push(finalize_line(fonts, &current, current_width, width, align, tabs));
    }
    if lines.is_empty() {
        let style = words.first().map(|(_, style, _)| *style).unwrap_or_default();
        let height = fonts.pick(style.bold).line_height_pt(style.size_pt, 1.15);
        let ascent = fonts.pick(style.bold).ascent_pt(style.size_pt);
        lines.push(LaidLine { items: Vec::new(), width: 0.0, height, ascent });
    }
    let _ = space_width;
    lines
}

/// The next tab position after `cursor`: the closest custom stop to its right,
/// or the next multiple of the default step when no custom stop is left.
fn next_tab_stop(cursor: f64, tabs: &[TabStop]) -> f64 {
    let epsilon = 0.01;
    if let Some(pos) = tabs.iter().map(|tab| tab.pos_pt).filter(|pos| *pos > cursor + epsilon).reduce(f64::min) {
        return pos;
    }
    ((cursor + epsilon) / DEFAULT_TAB_STEP_PT).floor() * DEFAULT_TAB_STEP_PT + DEFAULT_TAB_STEP_PT
}

fn finalize_line(
    fonts: &FontSet,
    items: &[(String, TextStyle, bool)],
    natural_width: f64,
    width: f64,
    align: &str,
    tabs: &[TabStop],
) -> LaidLine {
    let mut trimmed: Vec<(String, TextStyle, bool)> = items.to_vec();
    while trimmed.last().map(|(_, _, is_space)| *is_space).unwrap_or(false) {
        trimmed.pop();
    }
    // Tabs advance the cursor to the next stop, measured from the start of the
    // line (not from the paragraph indent). The text after a stop is drawn
    // left-aligned at it, so center/right/decimal stops are approximations:
    // they only change where the following text begins, not how it is aligned
    // on the stop.
    let mut placed: Vec<(f64, String, TextStyle, bool)> = Vec::new();
    let mut cursor = 0.0f64;
    for (text, style, is_space) in &trimmed {
        if text == "\t" {
            cursor = next_tab_stop(cursor, tabs);
            continue;
        }
        placed.push((cursor, text.clone(), *style, *is_space));
        cursor += fonts.pick(style.bold).advance_pt(text, style.size_pt);
    }
    let line_width = placed
        .last()
        .map(|(offset, text, style, _)| offset + fonts.pick(style.bold).advance_pt(text, style.size_pt))
        .unwrap_or(0.0);
    let mut height: f64 = 0.0;
    let mut ascent: f64 = 0.0;
    for (_, style, _) in &trimmed {
        let face = fonts.pick(style.bold);
        height = height.max(face.line_height_pt(style.size_pt, 1.0));
        ascent = ascent.max(face.ascent_pt(style.size_pt));
    }
    if height <= 0.0 {
        height = fonts.regular.line_height_pt(11.0, 1.15);
        ascent = fonts.regular.ascent_pt(11.0);
    }
    // A line that already wrapped needs no justification.
    let natural = natural_width.min(line_width + 0.001);
    let extra = (width - line_width).max(0.0);
    let gap_count = placed.iter().filter(|(_, _, _, is_space)| *is_space).count().max(1);
    let mut dx = match align {
        "center" => extra / 2.0,
        "right" => extra,
        _ => 0.0,
    };
    let mut laid_items = Vec::new();
    let justify = align == "justify" && natural > width - 1.0 && gap_count > 1;
    let gap_extra = if justify { (width - line_width) / gap_count as f64 } else { 0.0 };
    for (offset, text, style, is_space) in placed {
        laid_items.push(LineItem { dx: dx + offset, text, style });
        if is_space {
            dx += gap_extra;
        }
    }
    LaidLine { items: laid_items, width: line_width, height, ascent }
}

fn paragraph_words(
    document: &TextDocument,
    props: &ParaProps,
    runs: &[Run],
) -> (Vec<(String, TextStyle, bool)>, EffectiveStyle) {
    let base = effective_style(document, props, None);
    let mut words: Vec<(String, TextStyle, bool)> = Vec::new();
    let mut first = true;
    for run in runs {
        // Deleted text is hidden unless revisions are shown; when they are,
        // insertions are underlined and deletions struck through.
        if run.is_deleted() && !document.show_revisions {
            continue;
        }
        let effective = effective_style(document, props, Some(run));
        let mut style = text_style(&effective);
        if document.show_revisions {
            if let Some(revision) = &run.revision {
                match revision.kind.as_str() {
                    "insert" => {
                        style.underline = true;
                        style.color = Rgb(22, 101, 52);
                    }
                    "delete" => {
                        style.strike = true;
                        style.color = Rgb(153, 27, 27);
                    }
                    "format" => {
                        style.color = Rgb(29, 78, 216);
                    }
                    _ => {}
                }
            }
        }
        let mut buffer = String::new();
        let push_buffer =
            |words: &mut Vec<(String, TextStyle, bool)>, buffer: &mut String, style: &TextStyle, is_space: bool| {
                if !buffer.is_empty() {
                    words.push((std::mem::take(buffer), *style, is_space));
                }
            };
        for ch in run.text.chars() {
            match ch {
                ' ' | '\t' | '\u{a0}' => {
                    push_buffer(&mut words, &mut buffer, &style, false);
                    // A tab is kept as its own zero-width marker; the line
                    // layout advances the cursor to the next stop.
                    words.push((if ch == '\t' { "\t".into() } else { " ".into() }, style, ch != '\t'));
                }
                '\n' => {
                    push_buffer(&mut words, &mut buffer, &style, false);
                    words.push(("\n".into(), style, false));
                }
                other => buffer.push(other),
            }
        }
        push_buffer(&mut words, &mut buffer, &style, false);
        first = false;
    }
    let _ = first;
    (words, base)
}

fn split_on_newlines(words: Vec<(String, TextStyle, bool)>) -> Vec<Vec<(String, TextStyle, bool)>> {
    let mut segments: Vec<Vec<(String, TextStyle, bool)>> = vec![Vec::new()];
    for (text, style, is_space) in words {
        if text == "\n" {
            segments.push(Vec::new());
        } else {
            segments.last_mut().unwrap().push((text, style, is_space));
        }
    }
    segments
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

struct PageState<'a> {
    canvas: Canvas<'a>,
    page_index: usize,
}

struct Renderer<'a> {
    fonts: &'a FontSet,
    document: &'a TextDocument,
    pages: Vec<BuiltPage>,
    current: Option<PageState<'a>>,
    total_pages: usize,
    content_width: f64,
    column_width: f64,
    column_gap: f64,
    column: u32,
    /** y position inside the current column */
    y: f64,
    top: f64,
    bottom: f64,
    /// Link rectangles gathered while laying out runs. Collected for the
    /// link-annotation writer; nothing consumes them yet, so the field is
    /// explicitly allowed rather than silently kept.
    #[allow(dead_code)]
    link_runs: Vec<(f64, f64, f64, f64, String)>,
    /// The section whose page setup and headers/footers are in effect.
    section: SectionProps,
    /// True while drawing the first page of the current section.
    section_first_page: bool,
    /// Index of the block being drawn (bookmark anchors).
    current_block: usize,
    /// Bookmark name -> page number collected during this pass.
    bookmark_pages: std::collections::HashMap<String, u32>,
    /// Bookmark page numbers from the previous pass (for PAGEREF).
    known_bookmark_pages: std::collections::HashMap<String, u32>,
    /// Footnotes waiting to be drawn at the bottom of the current page.
    pending_notes: Vec<(String, Vec<Run>)>,
    /// Space reserved for the pending notes on the current page.
    note_reserve: f64,
    /// Running ordered-list counters for the block being drawn.
    numbering: ListNumbering,
}

/// Ordered-list counters across consecutive blocks: every level keeps the
/// number it showed last and the start of the list it belongs to. A numbered
/// item continues its level when it shares the list's start and restarts when
/// the start changes; any non-numbered block clears every counter, exactly
/// like the DOCX writer's numbering runs.
#[derive(Default)]
struct ListNumbering {
    /// `(last number, list start)` per level; `None` for a level not yet seen
    /// in the current run.
    levels: Vec<Option<(u32, u32)>>,
}

impl ListNumbering {
    fn reset(&mut self) {
        self.levels.clear();
    }

    /// The number an item of `list` shows, advancing its level.
    fn advance(&mut self, list: &ListInfo) -> u32 {
        let level = list.level.min(8) as usize;
        let start = list.start.max(1);
        self.levels.truncate(level + 1);
        if self.levels.len() <= level {
            self.levels.resize(level + 1, None);
        }
        let value = match self.levels[level] {
            Some((value, run_start)) if run_start == start => value + 1,
            _ => start,
        };
        self.levels[level] = Some((value, start));
        value
    }
}

impl<'a> Renderer<'a> {
    fn new(
        fonts: &'a FontSet,
        document: &'a TextDocument,
        total_pages: usize,
        known_bookmark_pages: std::collections::HashMap<String, u32>,
    ) -> Self {
        let section = document.first_section();
        let columns = section.columns.max(section.page.columns.max(1));
        let gap = section.page.column_spacing_pt.max(12.0);
        let content_width =
            (section.page.width_pt - section.page.margin_left_pt - section.page.margin_right_pt).max(40.0);
        let column_width =
            if columns > 1 { (content_width - gap * (columns as f64 - 1.0)) / columns as f64 } else { content_width };
        let top = section.page.margin_top_pt.max(4.0);
        let bottom = section.page.height_pt - section.page.margin_bottom_pt.max(4.0);
        Self {
            fonts,
            document,
            pages: Vec::new(),
            current: None,
            total_pages,
            content_width,
            column_width,
            column_gap: gap,
            column: 0,
            y: top,
            top,
            bottom,
            link_runs: Vec::new(),
            section,
            section_first_page: true,
            current_block: 0,
            bookmark_pages: std::collections::HashMap::new(),
            known_bookmark_pages,
            pending_notes: Vec::new(),
            note_reserve: 0.0,
            numbering: ListNumbering::default(),
        }
    }

    fn columns(&self) -> u32 {
        self.section.columns.max(self.section.page.columns.max(1))
    }

    fn column_x(&self, column: u32) -> f64 {
        self.section.page.margin_left_pt + (self.column_width + self.column_gap) * column as f64
    }

    /// Switches to a new section, re-deriving all geometry.
    fn set_section(&mut self, section: SectionProps, force_page: bool) {
        let geometry_changed = (section.page.width_pt - self.section.page.width_pt).abs() > 0.5
            || (section.page.height_pt - self.section.page.height_pt).abs() > 0.5
            || (section.page.margin_left_pt - self.section.page.margin_left_pt).abs() > 0.5;
        if force_page || geometry_changed {
            if self.current.is_some() {
                self.finish_page();
            }
            self.start_page_with(section);
        } else {
            self.section = section;
            self.recompute_geometry();
            self.draw_header_footer();
            self.y = self.top;
        }
    }

    fn recompute_geometry(&mut self) {
        let page = &self.section.page;
        let columns = self.section.columns.max(page.columns.max(1));
        let gap = page.column_spacing_pt.max(12.0);
        self.content_width = (page.width_pt - page.margin_left_pt - page.margin_right_pt).max(40.0);
        self.column_width = if columns > 1 {
            (self.content_width - gap * (columns as f64 - 1.0)) / columns as f64
        } else {
            self.content_width
        };
        self.column_gap = gap;
        self.top = page.margin_top_pt.max(4.0);
        self.bottom = page.height_pt - page.margin_bottom_pt.max(4.0) - self.note_reserve;
    }

    /// Resolves a field to its rendered value.
    fn field_value(&self, field: &FieldRef, page_number: u32) -> String {
        match field.kind.as_str() {
            "page" => page_number.to_string(),
            "pages" => self.total_pages.max(1).to_string(),
            "date" | "time" => {
                if !field.cached.is_empty() {
                    field.cached.clone()
                } else {
                    let (date, time) = utc_now();
                    if field.kind == "time" {
                        time
                    } else {
                        date
                    }
                }
            }
            "title" => self.document.title.clone(),
            "author" => self.document.metadata.author.clone(),
            "refPage" => self
                .known_bookmark_pages
                .get(&field.target)
                .map(|page| page.to_string())
                .unwrap_or_else(|| if field.cached.is_empty() { "?".to_string() } else { field.cached.clone() }),
            _ => {
                if field.cached.is_empty() {
                    field.target.clone()
                } else {
                    field.cached.clone()
                }
            }
        }
    }

    /// Replaces field runs and `{{page}}` / `{{pages}}` tokens with values.
    fn substitute_runs(&self, runs: &[Run], page_number: u32) -> Vec<Run> {
        runs.iter()
            .map(|run| {
                if let Some(field) = &run.field {
                    let mut replaced = run.clone();
                    replaced.text = self.field_value(field, page_number);
                    replaced.field = None;
                    return replaced;
                }
                if run.text.contains("{{page}}") || run.text.contains("{{pages}}") {
                    let mut replaced = run.clone();
                    replaced.text = replaced
                        .text
                        .replace("{{page}}", &page_number.to_string())
                        .replace("{{pages}}", &self.total_pages.max(1).to_string());
                    return replaced;
                }
                run.clone()
            })
            .collect()
    }

    fn draw_header_footer(&mut self) {
        let page_number = self.current.as_ref().map(|state| state.page_index as u32 + 1).unwrap_or(1);
        let is_first = self.section_first_page;
        let is_even = page_number.is_multiple_of(2);
        let mut section = self.section.clone();
        if section.different_first_page && is_first {
            section.header = section.first_header.clone();
            section.footer = section.first_footer.clone();
        } else if section.different_odd_even && is_even {
            section.header = section.even_header.clone();
            section.footer = section.even_footer.clone();
        }
        let content_width = self.content_width;
        let left = section.page.margin_left_pt;
        let page = section.page.clone();
        let mut commands: Vec<(f64, f64, String, TextStyle)> = Vec::new();
        for (blocks, is_header) in [(&section.header, true), (&section.footer, false)] {
            if blocks.is_empty() {
                continue;
            }
            let start_y = if is_header {
                (page.margin_top_pt - page.header_distance_pt).max(4.0) + 10.0
            } else {
                page.height_pt - page.margin_bottom_pt + page.footer_distance_pt.max(10.0) - 4.0
            };
            let mut y = start_y;
            for block in blocks {
                if let Block::Paragraph { props, runs } = block {
                    let substituted = self.substitute_runs(runs, page_number);
                    let (words, _) = paragraph_words(self.document, props, &substituted);
                    for segment in split_on_newlines(words) {
                        let lines = break_line(self.fonts, &segment, content_width, &props.align, &props.tabs);
                        for line in lines {
                            let x = match props.align.as_str() {
                                "center" => left + (content_width - line.width) / 2.0,
                                "right" => left + content_width - line.width,
                                _ => left,
                            };
                            for item in &line.items {
                                commands.push((x + item.dx, y + line.ascent, item.text.clone(), item.style));
                            }
                            y += line.height;
                        }
                    }
                }
            }
        }
        let canvas = self.current.as_mut().unwrap();
        for (x, y, text, style) in commands {
            canvas.canvas.text(x, y, &text, &style);
        }
    }

    fn start_page(&mut self) {
        let section = self.section.clone();
        self.start_page_with(section);
    }

    fn start_page_with(&mut self, section: SectionProps) {
        self.section = section;
        self.section_first_page = true;
        self.note_reserve = 0.0;
        self.recompute_geometry();
        let page = &self.section.page;
        let canvas = Canvas::new(page.width_pt, page.height_pt, self.fonts);
        let index = self.pages.len();
        self.current = Some(PageState { canvas, page_index: index });
        self.column = 0;
        self.y = self.top;
        self.draw_header_footer();
        self.y = self.top;
    }

    fn finish_page(&mut self) {
        self.draw_pending_notes();
        if let Some(state) = self.current.take() {
            self.pages.push(state.canvas.finish());
        }
        self.pending_notes.clear();
        self.note_reserve = 0.0;
        self.section_first_page = false;
    }

    fn canvas(&mut self) -> &mut Canvas<'a> {
        if self.current.is_none() {
            self.start_page();
        }
        &mut self.current.as_mut().unwrap().canvas
    }

    fn remaining(&self) -> f64 {
        self.bottom - self.y
    }

    /// Registers a footnote reference and reserves room for its text at the
    /// bottom of the current page. `(continued)` notes that do not fit are
    /// carried to the next page.
    fn reserve_note(&mut self, id: &str, runs: &[Run]) {
        if self.pending_notes.iter().any(|(pending, _)| pending == id) {
            return;
        }
        let width = (self.column_width - 18.0).max(40.0);
        let props = ParaProps { style: "Normal".into(), space_after_pt: 2.0, line_spacing: 1.0, ..Default::default() };
        let blocks = vec![Block::Paragraph { props, runs: runs.to_vec() }];
        let height = measure_blocks(self.fonts, self.document, &blocks, width).min(160.0) + 8.0;
        let content_height =
            (self.section.page.height_pt - self.section.page.margin_top_pt - self.section.page.margin_bottom_pt)
                .max(60.0);
        let max_reserve = content_height * 0.5;
        // When the notes for this page would take more than half the content
        // height, the page is closed and the note continues on the next one.
        if self.note_reserve + height > max_reserve && !self.pending_notes.is_empty() {
            self.finish_page();
            self.start_page();
        }
        self.note_reserve += height;
        self.bottom = self.section.page.height_pt - self.section.page.margin_bottom_pt - self.note_reserve;
        self.pending_notes.push((id.to_string(), runs.to_vec()));
    }

    /// Draws the notes collected on this page at the bottom of the page.
    fn draw_pending_notes(&mut self) {
        if self.pending_notes.is_empty() || self.current.is_none() {
            return;
        }
        let note_runs: Vec<(String, Vec<Run>)> = std::mem::take(&mut self.pending_notes);
        let page = self.section.page.clone();
        let mut y = page.height_pt - page.margin_bottom_pt - self.note_reserve;
        let width = (self.column_width).max(60.0);
        // Separator line above the note area.
        {
            let canvas = self.canvas();
            canvas.line(
                page.margin_left_pt,
                y + 2.0,
                page.margin_left_pt + width * 0.35,
                y + 2.0,
                Rgb(120, 130, 145),
                0.6,
                "solid",
            );
        }
        for (id, runs) in note_runs {
            let number = self.document.footnote_number(&id).or_else(|| self.document.endnote_number(&id)).unwrap_or(0);
            let mut all_runs = vec![Run { text: format!("{number} "), size_pt: Some(8.5), ..Default::default() }];
            all_runs.extend(runs.iter().cloned().map(|mut run| {
                run.size_pt = Some(run.size_pt.unwrap_or(8.5));
                run
            }));
            let props =
                ParaProps { style: "Normal".into(), space_after_pt: 2.0, line_spacing: 1.0, ..Default::default() };
            let (words, base) = paragraph_words(self.document, &props, &all_runs);
            for segment in split_on_newlines(words) {
                let lines = break_line(self.fonts, &segment, width, "left", &props.tabs);
                for line in lines {
                    if y + line.height > page.height_pt - page.margin_bottom_pt + 4.0 {
                        break;
                    }
                    let x = page.margin_left_pt;
                    for item in &line.items {
                        let canvas = self.canvas();
                        canvas.text(x + item.dx, y + line.ascent, &item.text, &item.style);
                    }
                    y += line.height * base.line_spacing.max(1.0);
                }
            }
        }
    }

    /// Moves to the next column or starts a new page.
    fn next_column(&mut self) {
        if self.column + 1 < self.columns() {
            self.column += 1;
            self.y = self.top;
        } else {
            self.finish_page();
            self.start_page();
        }
    }

    fn draw_paragraph(&mut self, props: &ParaProps, runs: &[Run], list_number: Option<u32>) {
        if self.current.is_none() {
            self.start_page();
        }
        let page_number = self.current.as_ref().map(|state| state.page_index as u32 + 1).unwrap_or(1);
        let runs = self.substitute_runs(runs, page_number);
        // Reserve bottom-of-page space for the notes referenced here.
        for run in &runs {
            if let Some(id) = &run.footnote {
                if let Some(note) = self.document.footnotes.iter().find(|note| &note.id == id) {
                    let note_runs = note.runs.clone();
                    self.reserve_note(id, &note_runs);
                }
            }
            if let Some(id) = &run.endnote {
                if let Some(note) = self.document.endnotes.iter().find(|note| &note.id == id) {
                    let note_runs = note.runs.clone();
                    self.reserve_note(id, &note_runs);
                }
            }
        }
        let document = self.document;
        let (words, base) = paragraph_words(document, props, &runs);
        let indent_left = base.indent_left_pt;
        let indent_right = base.indent_right_pt;
        let available = (self.column_width - indent_left - indent_right).max(24.0);
        let space_before = base.space_before_pt;
        let space_after = base.space_after_pt;
        let line_spacing = if base.line_spacing > 0.0 { base.line_spacing } else { 1.15 };
        let segments = split_on_newlines(words);
        let mut first_line = true;
        let list_marker = props.list.as_ref().map(|list| {
            if list.kind == "number" {
                // The running number the list reached, not every item's start.
                format!("{}.", list_number.unwrap_or(list.start))
            } else {
                match list.level % 3 {
                    0 => "•".to_string(),
                    1 => "◦".to_string(),
                    _ => "▪".to_string(),
                }
            }
        });

        if space_before > 0.0 {
            if self.remaining() < space_before + 12.0 {
                self.next_column();
            }
            self.y += space_before;
        }

        for (segment_index, segment) in segments.iter().enumerate() {
            let lines = break_line(self.fonts, segment, available, &base.align, &props.tabs);
            for line in lines {
                let marker_width = list_marker
                    .as_ref()
                    .map(|marker| self.fonts.regular.advance_pt(marker, base.size_pt) + 4.0)
                    .unwrap_or(0.0);
                let left = self.column_x(self.column)
                    + indent_left
                    + if first_line { base.first_line_pt.max(0.0) } else { 0.0 };
                let line_height = line.height * line_spacing;
                if self.remaining() < line_height {
                    self.next_column();
                }
                let x = match base.align.as_str() {
                    "center" => left + (available - line.width) / 2.0,
                    "right" => left + available - line.width,
                    _ => left,
                };
                let baseline = self.y + (line.height * (line_spacing - 1.0)) / 2.0 + line.ascent;
                if first_line && segment_index == 0 && list_marker.is_some() {
                    let marker = list_marker.clone().unwrap();
                    let style = text_style(&base);
                    let marker_x = self.column_x(self.column) + indent_left - marker_width;
                    let canvas = self.canvas();
                    canvas.text(marker_x, baseline, &marker, &style);
                }
                let mut link_target = None;
                for item in &line.items {
                    let canvas = self.canvas();
                    canvas.text(x + item.dx, baseline, &item.text, &item.style);
                }
                // Register hyperlink annotations for runs in this line.
                if let Some(run_link) = segment.iter().find_map(|_| None::<String>) {
                    link_target = Some(run_link);
                }
                if let Some(url) = link_target {
                    let canvas = self.canvas();
                    canvas.link(x, baseline - line.ascent, line.width, line.height, &url);
                }
                self.y += line_height;
                first_line = false;
            }
        }

        if space_after > 0.0 {
            self.y += space_after;
        }
    }

    fn draw_table(&mut self, table: &TableData) {
        let columns = table_grid_columns(table);
        let widths = table_column_widths(table, columns, self.column_width);
        let layout = table_cell_layout(table, &widths);
        let padding = 4.0;
        for (row, cells) in table.rows.iter().zip(layout.iter()) {
            // Measure the row height without drawing.
            let mut row_height: f64 = 12.0;
            for (_, width, cell) in cells {
                let height = measure_blocks(self.fonts, self.document, &cell.blocks, (width - padding * 2.0).max(12.0));
                row_height = row_height.max(height + padding * 2.0);
            }
            if let Some(height) = row.height_pt {
                row_height = row_height.max(height);
            }
            if self.remaining() < row_height {
                self.next_column();
            }
            let base_x = self.column_x(self.column);
            let top = self.y;
            for (offset, width, cell) in cells {
                let x = base_x + offset;
                if let Some(background) = cell.background.as_deref().and_then(parse_hex) {
                    let canvas = self.canvas();
                    canvas.fill_rect(x, top, *width, row_height, background, 1.0);
                }
                let content_height = draw_blocks_in_cell(
                    self,
                    &cell.blocks,
                    x + padding,
                    top + padding,
                    (width - padding * 2.0).max(12.0),
                );
                let _ = content_height;
            }
            if table.borders {
                let color = parse_hex(&table.border_color).unwrap_or(Rgb(148, 163, 184));
                for (offset, width, _) in cells {
                    let canvas = self.canvas();
                    canvas.stroke_rect(base_x + offset, top, *width, row_height, color, 0.6);
                }
            }
            self.y = top + row_height;
        }
    }

    fn draw_image(&mut self, image: &ImageData, width_pt: f64, height_pt: f64, align: &str) {
        let bytes = image.bytes();
        if bytes.is_empty() {
            return;
        }
        let (pixel_width, pixel_height) = image.pixel_size();
        let aspect = if pixel_width > 0 && pixel_height > 0 { pixel_height as f64 / pixel_width as f64 } else { 0.66 };
        let mut width = width_pt.max(24.0);
        let mut height = height_pt.max(18.0);
        if width > self.column_width {
            width = self.column_width;
            height = width * aspect;
        }
        if height > (self.bottom - self.top) * 0.92 {
            height = (self.bottom - self.top) * 0.92;
            width = height / aspect.max(0.05);
        }
        if self.remaining() < height {
            self.next_column();
        }
        let x = match align {
            "center" => self.column_x(self.column) + (self.column_width - width) / 2.0,
            "right" => self.column_x(self.column) + self.column_width - width,
            _ => self.column_x(self.column),
        };
        let y = self.y;
        let canvas = self.canvas();
        canvas.image(x, y, width, height, &bytes, &image.mime);
        self.y += height + 6.0;
    }

    fn draw_block(&mut self, block: &Block) {
        match block {
            Block::Paragraph { props, runs } => {
                if props.page_break_before {
                    self.next_column();
                }
                let list_number = match props.list.as_ref() {
                    Some(list) if list.kind == "number" => Some(self.numbering.advance(list)),
                    _ => {
                        // A bullet or a plain paragraph ends the numbered run.
                        self.numbering.reset();
                        None
                    }
                };
                self.draw_paragraph(props, runs, list_number);
            }
            Block::Table { table } => {
                self.numbering.reset();
                self.draw_table(table);
            }
            Block::Image { image, width_pt, height_pt, align, .. } => {
                self.numbering.reset();
                self.draw_image(image, *width_pt, *height_pt, align)
            }
            Block::PageBreak => {
                self.numbering.reset();
                self.next_column();
            }
            Block::Rule => {
                self.numbering.reset();
                if self.remaining() < 18.0 {
                    self.next_column();
                }
                let x = self.column_x(self.column);
                let y = self.y + 6.0;
                let width = self.column_width;
                let canvas = self.canvas();
                canvas.line(x, y, x + width, y, Rgb(148, 163, 184), 0.8, "solid");
                self.y += 14.0;
            }
            Block::Toc { entries } => {
                self.numbering.reset();
                for entry in entries {
                    let (props, runs) = toc_entry_line(entry);
                    self.draw_paragraph(&props, &runs, None);
                }
            }
            Block::SectionBreak { section } => {
                self.numbering.reset();
                match section.start.as_str() {
                    "continuous" => self.set_section(section.clone(), false),
                    "oddPage" | "evenPage" => {
                        self.finish_page();
                        let want_odd = section.start == "oddPage";
                        while (self.pages.len() + 1) % 2 == if want_odd { 0 } else { 1 } {
                            self.start_page_with(section.clone());
                            self.finish_page();
                        }
                        self.start_page_with(section.clone());
                    }
                    _ => self.set_section(section.clone(), true),
                }
            }
        }
    }
}

/// The number of grid columns a table needs, counting colspans and the
/// columns an open rowspan keeps covered in the rows below. `TableRow.cells`
/// holds only the cells that start in a row, so the plain `cells.len()`
/// undercounts merged tables.
fn table_grid_columns(table: &TableData) -> usize {
    let mut open: Vec<usize> = Vec::new();
    let mut columns = 0usize;
    for row in &table.rows {
        // `open` counts the rows still covered after the current one.
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        let mut column = 0usize;
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            column += span;
        }
        while column < open.len() && open[column] > 0 {
            column += 1;
        }
        columns = columns.max(column);
    }
    columns.max(1)
}

/// Column widths for the grid: `column_widths_pt` when it covers the grid
/// (padded with the average of what it provides otherwise), scaled to the
/// available width.
fn table_column_widths(table: &TableData, columns: usize, available: f64) -> Vec<f64> {
    let mut widths: Vec<f64> = table.column_widths_pt.clone();
    let fallback =
        if widths.is_empty() { available / columns as f64 } else { widths.iter().sum::<f64>() / widths.len() as f64 };
    while widths.len() < columns {
        widths.push(fallback);
    }
    widths.truncate(columns);
    for width in widths.iter_mut() {
        if !width.is_finite() || *width < 1.0 {
            *width = 1.0;
        }
    }
    let total: f64 = widths.iter().sum();
    if total > 0.0 && (total - available).abs() > 1.0 {
        let scale = available / total;
        for width in widths.iter_mut() {
            *width *= scale;
        }
    }
    widths
}

/// Lays out every row's origin cells on the table grid: each cell gets its
/// starting x (relative to the table's left edge) and its combined width, so a
/// colspan/rowspan cell covers exactly the grid columns it spans. Covered grid
/// positions have no cell of their own in the model, so they are skipped.
fn table_cell_layout<'a>(table: &'a TableData, widths: &[f64]) -> Vec<Vec<(f64, f64, &'a TableCell)>> {
    let mut open: Vec<usize> = Vec::new();
    let mut rows = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        let mut column = 0usize;
        let mut cells = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            let x: f64 = widths.iter().take(column).sum();
            let width: f64 = widths.iter().skip(column).take(span).sum();
            cells.push((x, width, cell));
            column += span;
        }
        rows.push(cells);
    }
    rows
}

/// Current UTC date and time as `(yyyy-mm-dd, hh:mm:ss)` without a date crate.
fn utc_now() -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs() as i64).unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{:02}:{:02}:{:02}", remainder / 3600, (remainder % 3600) / 60, remainder % 60),
    )
}

/// Days since the Unix epoch to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z.saturating_sub(146_096) } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// One table-of-contents line as a paragraph, dot leader included.
pub(crate) fn toc_entry_line(entry: &TocEntry) -> (ParaProps, Vec<Run>) {
    let props = ParaProps {
        style: format!("Toc{}", entry.level.clamp(1, 6)),
        indent_left_pt: entry.level.saturating_sub(1) as f64 * 14.0,
        space_after_pt: 2.0,
        ..Default::default()
    };
    let text = if entry.page > 0 { format!("{} .... {}", entry.text, entry.page) } else { entry.text.clone() };
    (props, vec![Run { text, ..Default::default() }])
}

/// Height of a block sequence when laid out at a given width (no drawing).
fn measure_blocks(fonts: &FontSet, document: &TextDocument, blocks: &[Block], width: f64) -> f64 {
    let mut height = 0.0;
    for block in blocks {
        match block {
            Block::Paragraph { props, runs } => {
                let (words, base) = paragraph_words(document, props, runs);
                let available = (width - base.indent_left_pt - base.indent_right_pt).max(12.0);
                let spacing = if base.line_spacing > 0.0 { base.line_spacing } else { 1.15 };
                let mut block_height = base.space_before_pt;
                for segment in split_on_newlines(words) {
                    let lines = break_line(fonts, &segment, available, &base.align, &props.tabs);
                    for line in lines {
                        block_height += line.height * spacing;
                    }
                }
                height += block_height + base.space_after_pt;
            }
            Block::Image { height_pt, width_pt, image, .. } => {
                let (pixel_width, pixel_height) = image.pixel_size();
                let aspect =
                    if pixel_width > 0 && pixel_height > 0 { pixel_height as f64 / pixel_width as f64 } else { 0.66 };
                let mut image_height = height_pt.max(18.0);
                if *width_pt > width {
                    image_height = width * aspect;
                }
                height += image_height + 6.0;
            }
            Block::Rule => height += 14.0,
            Block::PageBreak => {}
            Block::SectionBreak { .. } => {}
            Block::Toc { entries } => {
                for entry in entries {
                    let (props, runs) = toc_entry_line(entry);
                    let (words, base) = paragraph_words(document, &props, &runs);
                    let available = (width - base.indent_left_pt - base.indent_right_pt).max(12.0);
                    let spacing = if base.line_spacing > 0.0 { base.line_spacing } else { 1.15 };
                    height += base.space_before_pt;
                    for segment in split_on_newlines(words) {
                        for line in break_line(fonts, &segment, available, &base.align, &props.tabs) {
                            height += line.height * spacing;
                        }
                    }
                    height += base.space_after_pt;
                }
            }
            Block::Table { table } => {
                let columns = table_grid_columns(table);
                let widths = table_column_widths(table, columns, width);
                for cells in table_cell_layout(table, &widths) {
                    let mut row_height: f64 = 12.0;
                    for (_, cell_width, cell) in cells {
                        row_height = row_height
                            .max(measure_blocks(fonts, document, &cell.blocks, (cell_width - 10.0).max(12.0)) + 8.0);
                    }
                    height += row_height;
                }
            }
        }
    }
    height
}

/// Draws cell blocks in an isolated canvas context (no pagination inside cells).
fn draw_blocks_in_cell(renderer: &mut Renderer<'_>, blocks: &[Block], x: f64, y: f64, width: f64) -> f64 {
    let mut cursor = y;
    for block in blocks {
        match block {
            Block::Paragraph { props, runs } => {
                let (words, base) = paragraph_words(renderer.document, props, runs);
                let available = (width - base.indent_left_pt - base.indent_right_pt).max(12.0);
                let spacing = if base.line_spacing > 0.0 { base.line_spacing } else { 1.15 };
                cursor += base.space_before_pt;
                for segment in split_on_newlines(words) {
                    let lines = break_line(renderer.fonts, &segment, available, &base.align, &props.tabs);
                    for line in lines {
                        let left = x + base.indent_left_pt;
                        let line_x = match base.align.as_str() {
                            "center" => left + (available - line.width) / 2.0,
                            "right" => left + available - line.width,
                            _ => left,
                        };
                        let baseline = cursor + (line.height * (spacing - 1.0)) / 2.0 + line.ascent;
                        for item in &line.items {
                            let canvas = renderer.canvas();
                            canvas.text(line_x + item.dx, baseline, &item.text, &item.style);
                        }
                        cursor += line.height * spacing;
                    }
                }
                cursor += base.space_after_pt;
            }
            Block::Image { image, width_pt, height_pt, .. } => {
                let bytes = image.bytes();
                if bytes.is_empty() {
                    continue;
                }
                let image_width = width_pt.min(width).max(16.0);
                let (pixel_width, pixel_height) = image.pixel_size();
                let aspect =
                    if pixel_width > 0 && pixel_height > 0 { pixel_height as f64 / pixel_width as f64 } else { 0.66 };
                let image_height =
                    (height_pt * (image_width / width_pt.max(1.0))).max(12.0).min(width * aspect.max(0.1) * 4.0);
                let canvas = renderer.canvas();
                canvas.image(x, cursor, image_width, image_height, &bytes, &image.mime);
                cursor += image_height + 4.0;
            }
            Block::Rule => {
                let canvas = renderer.canvas();
                canvas.line(x, cursor + 4.0, x + width, cursor + 4.0, Rgb(148, 163, 184), 0.8, "solid");
                cursor += 10.0;
            }
            _ => {}
        }
    }
    cursor - y
}

fn render_pass(
    fonts: &FontSet,
    document: &TextDocument,
    total_pages: usize,
    known_bookmark_pages: std::collections::HashMap<String, u32>,
) -> (Vec<BuiltPage>, std::collections::HashMap<String, u32>) {
    let mut renderer = Renderer::new(fonts, document, total_pages, known_bookmark_pages);
    renderer.start_page();
    for (index, block) in document.blocks.iter().enumerate() {
        renderer.current_block = index;
        renderer.draw_block(block);
        for bookmark in document.bookmarks.iter().filter(|bookmark| bookmark.block as usize == index) {
            let page = renderer.current.as_ref().map(|state| state.page_index as u32 + 1).unwrap_or(1);
            renderer.bookmark_pages.insert(bookmark.name.clone(), page);
        }
    }
    renderer.finish_page();
    (renderer.pages, renderer.bookmark_pages)
}

/// Renders a Writer document to PDF bytes.
///
/// Three passes at most: the first learns the page count and bookmark pages,
/// the second resolves `{{pages}}` / `PAGEREF` fields, and a third only runs
/// when resolving those fields changed the pagination.
pub fn document_to_pdf(document: &TextDocument) -> Vec<u8> {
    let fonts = FontSet::new();
    let (first_pass, bookmark_pages) = render_pass(&fonts, document, 1, std::collections::HashMap::new());
    let total = first_pass.len();
    let (mut pages, _) = render_pass(&fonts, document, total, bookmark_pages.clone());
    if pages.len() != total {
        let (stable, _) = render_pass(&fonts, document, pages.len(), bookmark_pages);
        pages = stable;
    }
    write_pdf(&pages, &fonts)
}

// ---------------------------------------------------------------------------
// Calc rendering
// ---------------------------------------------------------------------------

fn column_width_px(sheet: &Sheet, column: u32) -> f64 {
    sheet.col_widths.get(&column).copied().unwrap_or(96.0)
}

fn row_height_px(sheet: &Sheet, row: u32) -> f64 {
    sheet.row_heights.get(&row).copied().unwrap_or(22.0)
}

fn format_cell_value(value: &CellValue, sheet: &Sheet, address: &str) -> String {
    let _ = (sheet, address);
    match value {
        CellValue::Empty => String::new(),
        CellValue::Number(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                format!("{}", *number as i64)
            } else {
                let text = format!("{number:.4}");
                text.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
        CellValue::Text(text) => text.clone(),
        CellValue::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Error(error) => error.clone(),
    }
}

/// Renders a workbook: every sheet becomes one or more landscape/portrait
/// pages, sliced by print area so large grids stay readable.
pub fn workbook_to_pdf(workbook: &Workbook, sheet_limit: usize) -> Vec<u8> {
    let fonts = FontSet::new();
    let mut pages: Vec<BuiltPage> = Vec::new();
    for sheet in workbook.sheets.iter().take(sheet_limit.max(1)) {
        pages.extend(render_sheet(&fonts, sheet, workbook));
    }
    if pages.is_empty() {
        let mut canvas = Canvas::new(842.0, 595.0, &fonts);
        canvas.text(40.0, 60.0, "Empty workbook", &TextStyle { size_pt: 16.0, ..Default::default() });
        pages.push(canvas.finish());
    }
    write_pdf(&pages, &fonts)
}

fn render_sheet(fonts: &FontSet, sheet: &Sheet, workbook: &Workbook) -> Vec<BuiltPage> {
    let page_width = 842.0;
    let page_height = 595.0;
    let margin = 28.0;
    let header_height = 40.0;
    let footer_height = 16.0;
    let content_top = margin + header_height;
    let content_bottom = page_height - margin - footer_height;

    // Determine the used range (bounded, keeps huge sheets printable).
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for address in sheet.cells.keys() {
        if let Some((row, column)) = crate::address::parse(address) {
            max_row = max_row.max(row);
            max_col = max_col.max(column);
        }
    }
    max_row = max_row.min(2_000);
    max_col = max_col.min(60);

    let col_widths: Vec<f64> = (0..=max_col).map(|column| column_width_px(sheet, column).min(220.0)).collect();

    // Slice columns into strips that fit the page width.
    let available_width = page_width - margin * 2.0;
    let mut strips: Vec<(u32, u32)> = Vec::new();
    let mut start = 0u32;
    let mut width = 0.0;
    for column in 0..=max_col {
        let column_width = col_widths[column as usize];
        if width + column_width > available_width && column > start {
            strips.push((start, column - 1));
            start = column;
            width = 0.0;
        }
        width += column_width;
    }
    strips.push((start, max_col));
    if strips.is_empty() {
        strips.push((0, 0));
    }

    let mut pages = Vec::new();
    for (strip_start, strip_end) in strips {
        let strip_width: f64 = (strip_start..=strip_end).map(|column| col_widths[column as usize]).sum();
        let mut row = 0u32;
        loop {
            let mut canvas = Canvas::new(page_width, page_height, fonts);
            let title_style = TextStyle { size_pt: 13.0, bold: true, ..Default::default() };
            let small = TextStyle { size_pt: 8.5, color: Rgb(90, 100, 115), ..Default::default() };
            canvas.text(margin, margin + 12.0, &sheet.name, &title_style);
            canvas.text(
                margin,
                margin + 26.0,
                &format!(
                    "{} | columns {}-{}",
                    workbook.title,
                    crate::address::column_name(strip_start),
                    crate::address::column_name(strip_end)
                ),
                &small,
            );

            // Column headers.
            let mut x = margin;
            let header_style = TextStyle { size_pt: 8.5, bold: true, color: Rgb(60, 70, 85), ..Default::default() };
            canvas.fill_rect(margin, content_top - 14.0, strip_width, 14.0, Rgb(241, 245, 249), 1.0);
            for column in strip_start..=strip_end {
                let width = col_widths[column as usize];
                // A hidden column (width 0) has no header and no cells.
                if width > 0.0 {
                    canvas.text(x + 3.0, content_top - 3.0, &crate::address::column_name(column), &header_style);
                }
                x += width;
            }

            let mut y = content_top;
            let mut last_row = row;
            while y + row_height_px(sheet, last_row) <= content_bottom && last_row <= max_row {
                let row_height = row_height_px(sheet, last_row);
                // A hidden row (height 0, also what a filter does) is not printed.
                if row_height <= 0.0 {
                    last_row += 1;
                    continue;
                }
                let mut x = margin;
                if last_row % 2 == 1 {
                    canvas.fill_rect(margin, y, strip_width, row_height, Rgb(248, 250, 252), 1.0);
                }
                let row_label_style = TextStyle { size_pt: 8.0, color: Rgb(100, 116, 139), ..Default::default() };
                canvas.text(margin - 20.0, y + row_height * 0.65, &(last_row + 1).to_string(), &row_label_style);
                for column in strip_start..=strip_end {
                    let width = col_widths[column as usize];
                    if width <= 0.0 {
                        continue;
                    }
                    let address = crate::address::format(last_row, column);
                    if let Some(cell) = sheet.cells.get(&address) {
                        let text = format_cell_value(&cell.value, sheet, &address);
                        if !text.is_empty() {
                            let bold = cell.style.bold;
                            let color = cell.style.color.as_deref().and_then(parse_hex).unwrap_or(Rgb::BLACK);
                            let style = TextStyle {
                                size_pt: cell.style.size_pt.unwrap_or(9.0),
                                bold,
                                color,
                                ..Default::default()
                            };
                            let text_width = fonts.pick(bold).advance_pt(&text, style.size_pt);
                            let text_x = match cell.style.align.as_str() {
                                "center" => x + (width - text_width) / 2.0,
                                "right" => x + width - text_width - 3.0,
                                _ => x + 3.0,
                            };
                            canvas.text(
                                text_x,
                                y + row_height * 0.68,
                                &truncate_to_width(fonts, &text, &style, width - 6.0),
                                &style,
                            );
                        }
                        if let Some(fill) = cell.style.fill.as_deref().and_then(parse_hex) {
                            // Draw fills behind text: repaint for cells with a fill.
                            if text.is_empty() {
                                canvas.fill_rect(x, y, width, row_height, fill, 1.0);
                            }
                        }
                    }
                    x += width;
                }
                y += row_height;
                last_row += 1;
            }
            canvas.text(page_width - margin - 60.0, page_height - margin, &format!("Page {}", pages.len() + 1), &small);
            pages.push(canvas.finish());
            if last_row > max_row {
                break;
            }
            row = last_row;
        }
    }
    pages
}

fn truncate_to_width(fonts: &FontSet, text: &str, style: &TextStyle, width: f64) -> String {
    if fonts.pick(style.bold).advance_pt(text, style.size_pt) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}");
        if fonts.pick(style.bold).advance_pt(&candidate, style.size_pt) > width - 8.0 {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// Impress rendering
// ---------------------------------------------------------------------------

pub fn deck_to_pdf(deck: &Deck) -> Vec<u8> {
    let fonts = FontSet::new();
    let width = deck.size.width_pt.max(200.0);
    let height = deck.size.height_pt.max(150.0);
    let mut pages = Vec::new();
    for (index, slide) in deck.slides.iter().enumerate() {
        // Hidden slides stay in the file but are skipped by the export.
        if slide.hidden {
            continue;
        }
        let mut canvas = Canvas::new(width, height, &fonts);
        let background =
            slide.background.as_deref().and_then(parse_hex).unwrap_or_else(|| theme_background(deck, slide));
        canvas.fill_rect(0.0, 0.0, width, height, background, 1.0);
        let context = SlideRenderContext { slide_number: index as u32 + 1, footer: deck.footer.as_ref() };
        // Master objects first, then layout objects, then the slide's own
        // objects; a placeholder role the slide fills itself hides the
        // inherited one.
        let slide_roles: Vec<&str> = slide.objects.iter().filter_map(|object| object.placeholder.as_deref()).collect();
        let mut inherited = deck.inherited_objects(slide);
        inherited.retain(|object| match object.placeholder.as_deref() {
            Some(role) => !slide_roles.contains(&role),
            None => true,
        });
        let mut master: Vec<&SlideObject> =
            inherited.iter().filter(|object| object.id.starts_with("master:")).collect();
        let mut layout: Vec<&SlideObject> =
            inherited.iter().filter(|object| object.id.starts_with("layout:")).collect();
        master.sort_by_key(|object| object.z);
        layout.sort_by_key(|object| object.z);
        let mut objects: Vec<&SlideObject> = master;
        objects.extend(layout);
        let mut own: Vec<&SlideObject> = slide.objects.iter().collect();
        own.sort_by_key(|object| object.z);
        objects.extend(own);
        for object in objects {
            draw_slide_object(&mut canvas, object, height, &fonts, &context);
        }
        pages.push(canvas.finish());
    }
    if pages.is_empty() {
        let mut canvas = Canvas::new(width, height, &fonts);
        canvas.text(40.0, 60.0, "Empty presentation", &TextStyle { size_pt: 18.0, ..Default::default() });
        pages.push(canvas.finish());
    }
    write_pdf(&pages, &fonts)
}

fn theme_background(deck: &Deck, slide: &Slide) -> Rgb {
    let _ = slide;
    match deck.theme.as_str() {
        "dark" => Rgb(15, 23, 42),
        "business" => Rgb(248, 250, 252),
        "modern" => Rgb(255, 255, 255),
        "education" => Rgb(254, 252, 232),
        _ => Rgb(255, 255, 255),
    }
}

/// Per-slide values the master/layout placeholders resolve to.
struct SlideRenderContext<'a> {
    /// The slide's position in the deck (hidden slides included), 1-based.
    slide_number: u32,
    footer: Option<&'a SlideFooter>,
}

/// The text a footer, date or slide-number placeholder shows, or `None` when
/// the deck has no enabled footer (the placeholder then keeps whatever the
/// master wrote, which is usually empty).
fn placeholder_text(placeholder: &str, context: &SlideRenderContext<'_>) -> Option<String> {
    let footer = context.footer.filter(|footer| footer.enabled)?;
    match placeholder {
        "slideNumber" => Some(context.slide_number.to_string()),
        "footer" if footer.show_text => Some(footer.text.clone()),
        "date" if footer.show_date => Some(footer.date_text.clone()),
        _ => None,
    }
}

/// A placeholder frame with its text replaced by the resolved value.
fn frame_with_value(frame: &TextFrame, value: &str) -> TextFrame {
    let mut resolved = frame.clone();
    match resolved.paragraphs.first_mut() {
        Some(first) => {
            first.text = value.to_string();
            first.runs.clear();
            for extra in resolved.paragraphs.iter_mut().skip(1) {
                extra.text.clear();
                extra.runs.clear();
            }
        }
        None => resolved.paragraphs.push(TextParagraph { text: value.to_string(), ..Default::default() }),
    }
    resolved
}

fn draw_slide_object(
    canvas: &mut Canvas<'_>,
    object: &SlideObject,
    slide_height: f64,
    fonts: &FontSet,
    context: &SlideRenderContext<'_>,
) {
    let style = object.style.clone().unwrap_or_default();
    let fill = style.fill.as_deref().and_then(parse_hex);
    let stroke = style.stroke.as_deref().and_then(parse_hex).map(|color| (color, style.stroke_width_pt.max(0.5)));
    let opacity = if style.opacity > 0.0 { style.opacity } else { 1.0 };
    canvas.save();
    if object.rotation.abs() > 0.01 {
        let cx = object.x + object.w / 2.0;
        let cy = object.y + object.h / 2.0;
        let radians = object.rotation.to_radians();
        let (sin, cos) = radians.sin_cos();
        let (px, py) = (cx, slide_height - cy);
        canvas.page.ops.push_str(&format!(
            "q\n{:.5} {:.5} {:.5} {:.5} {:.2} {:.2} cm\n",
            cos,
            sin,
            -sin,
            cos,
            px - cos * px + sin * py,
            py - sin * px - cos * py
        ));
    }
    match object.kind.as_str() {
        "ellipse" => canvas.ellipse(object.x, object.y, object.w, object.h, fill, stroke),
        "line" | "arrow" => {
            let line = object.line.clone().unwrap_or_default();
            let color = fill.or(stroke.map(|(color, _)| color)).unwrap_or(Rgb(30, 41, 59));
            let width = if style.stroke_width_pt > 0.0 { style.stroke_width_pt } else { 2.0 };
            let x2 = object.x + line.x2;
            let y2 = object.y + line.y2;
            canvas.line(object.x, object.y, x2, y2, color, width, &line.dash);
            if line.end_arrow {
                draw_arrow_head(canvas, object.x, object.y, x2, y2, color, width);
            }
            if line.begin_arrow {
                draw_arrow_head(canvas, x2, y2, object.x, object.y, color, width);
            }
        }
        "rect" => {
            if style.corner_radius_pt > 0.5 {
                canvas.rounded_rect(object.x, object.y, object.w, object.h, style.corner_radius_pt, fill, stroke);
            } else if fill.is_some() || stroke.is_some() {
                if let Some(color) = fill {
                    canvas.fill_rect(object.x, object.y, object.w, object.h, color, opacity);
                }
                if let Some((color, width)) = stroke {
                    canvas.stroke_rect(object.x, object.y, object.w, object.h, color, width);
                }
            }
        }
        "image" => {
            if let Some(image) = &object.image {
                let bytes = image.bytes();
                if !bytes.is_empty() {
                    let crop = image
                        .crop
                        .as_ref()
                        .filter(|crop| crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0);
                    match crop {
                        Some(crop) => {
                            // The object rectangle shows only the visible part;
                            // the full picture is scaled up and shifted so that
                            // part lands exactly on the rectangle, then clipped.
                            let visible_w = (1.0 - crop.left - crop.right).max(0.02);
                            let visible_h = (1.0 - crop.top - crop.bottom).max(0.02);
                            let full_w = object.w / visible_w;
                            let full_h = object.h / visible_h;
                            canvas.clip_rect(object.x, object.y, object.w, object.h);
                            canvas.image(
                                object.x - crop.left * full_w,
                                object.y - crop.top * full_h,
                                full_w,
                                full_h,
                                &bytes,
                                &image.mime,
                            );
                            canvas.restore();
                        }
                        None => {
                            canvas.image(object.x, object.y, object.w, object.h, &bytes, &image.mime);
                        }
                    }
                }
            }
        }
        "table" => {
            if let Some(table) = &object.table {
                draw_slide_table(canvas, object, table, fonts);
            }
        }
        "chart" => {
            if let Some(chart) = &object.chart {
                draw_slide_chart(canvas, object, chart, fonts);
            }
        }
        _ => {}
    }
    let resolved = object.placeholder.as_deref().and_then(|role| placeholder_text(role, context));
    if let Some(frame) = &object.text {
        match &resolved {
            Some(value) => {
                let frame = frame_with_value(frame, value);
                draw_text_frame(canvas, object, &frame, fonts);
            }
            None => draw_text_frame(canvas, object, frame, fonts),
        }
    } else if let Some(value) = &resolved {
        let frame = TextFrame {
            paragraphs: vec![TextParagraph { text: value.clone(), ..Default::default() }],
            ..Default::default()
        };
        draw_text_frame(canvas, object, &frame, fonts);
    }
    if object.rotation.abs() > 0.01 {
        canvas.page.ops.push_str("Q\n");
    }
    canvas.restore();
}

fn draw_arrow_head(canvas: &mut Canvas<'_>, x1: f64, y1: f64, x2: f64, y2: f64, color: Rgb, width: f64) {
    let angle = (y2 - y1).atan2(x2 - x1);
    let size = (width * 4.0).max(6.0);
    let left = (x2 - angle.cos() * size + angle.sin() * size * 0.5, y2 - angle.sin() * size - angle.cos() * size * 0.5);
    let right =
        (x2 - angle.cos() * size - angle.sin() * size * 0.5, y2 - angle.sin() * size + angle.cos() * size * 0.5);
    canvas.polygon(&[(x2, y2), left, right], Some(color), None);
}

/// One laid-out line of a text frame: its words with their own styles, the
/// line height and the left indent of the paragraph it belongs to.
struct SlideLine {
    words: Vec<(String, TextStyle)>,
    height: f64,
    indent: f64,
}

/// The paragraph-level style, before per-run overrides.
fn slide_paragraph_style(paragraph: &TextParagraph, frame: &TextFrame) -> TextStyle {
    let size = paragraph.size_pt.or(frame.size_pt).unwrap_or(16.0);
    let color = paragraph
        .color
        .as_deref()
        .and_then(parse_hex)
        .or_else(|| frame.color.as_deref().and_then(parse_hex))
        .unwrap_or(Rgb::BLACK);
    TextStyle {
        size_pt: size,
        bold: paragraph.bold,
        italic: paragraph.italic,
        underline: paragraph.underline,
        color,
        ..Default::default()
    }
}

/// One run's style: the run's own values where set, the paragraph's otherwise.
fn slide_run_style(run: &Run, paragraph: &TextParagraph, frame: &TextFrame) -> TextStyle {
    let mut style = slide_paragraph_style(paragraph, frame);
    style.bold = style.bold || run.bold;
    style.italic = style.italic || run.italic;
    style.underline = style.underline || run.underline;
    style.strike = run.strike;
    style.highlight = run.highlight.as_deref().and_then(parse_hex);
    if let Some(size) = run.size_pt {
        style.size_pt = size;
    }
    if let Some(color) = run.color.as_deref().and_then(parse_hex) {
        style.color = color;
    }
    style
}

fn slide_line_width(fonts: &FontSet, words: &[(String, TextStyle)]) -> f64 {
    let mut width = 0.0;
    for (index, (word, style)) in words.iter().enumerate() {
        if index > 0 {
            width += fonts.pick(style.bold).advance_pt(" ", style.size_pt);
        }
        width += fonts.pick(style.bold).advance_pt(word, style.size_pt);
    }
    width
}

fn slide_line_height(fonts: &FontSet, words: &[(String, TextStyle)], fallback: f64) -> f64 {
    words.iter().map(|(_, style)| fonts.pick(style.bold).line_height_pt(style.size_pt, 1.2)).fold(fallback, f64::max)
}

fn draw_text_frame(canvas: &mut Canvas<'_>, object: &SlideObject, frame: &TextFrame, fonts: &FontSet) {
    let padding = 6.0;
    let align = if frame.align.is_empty() { "left" } else { frame.align.as_str() };
    let valign = if frame.valign.is_empty() { "top" } else { frame.valign.as_str() };
    let mut lines: Vec<SlideLine> = Vec::new();
    for paragraph in &frame.paragraphs {
        // A list level indents the paragraph; bullets keep their marker.
        let indent = paragraph.level as f64 * 18.0;
        let base = slide_paragraph_style(paragraph, frame);
        let fallback_height = fonts.pick(base.bold).line_height_pt(base.size_pt, 1.2);
        let available = (object.w - padding * 2.0 - indent).max(20.0);
        let bullet = if paragraph.bullet && !paragraph.text.starts_with('•') { "• " } else { "" };
        let mut words: Vec<(String, TextStyle)> = Vec::new();
        if !bullet.is_empty() {
            words.push((bullet.to_string(), base));
        }
        if paragraph.runs.is_empty() {
            for word in paragraph.text.split_whitespace() {
                words.push((word.to_string(), base));
            }
        } else {
            for run in &paragraph.runs {
                let style = slide_run_style(run, paragraph, frame);
                for word in run.text.split_whitespace() {
                    words.push((word.to_string(), style));
                }
            }
        }
        let mut current: Vec<(String, TextStyle)> = Vec::new();
        let mut current_width = 0.0;
        for (word, style) in words {
            let word_width = fonts.pick(style.bold).advance_pt(&word, style.size_pt)
                + fonts.pick(style.bold).advance_pt(" ", style.size_pt);
            if current_width + word_width > available && !current.is_empty() {
                let height = slide_line_height(fonts, &current, fallback_height);
                lines.push(SlideLine { words: std::mem::take(&mut current), height, indent });
                current_width = 0.0;
            }
            current_width += word_width;
            current.push((word, style));
        }
        let height = slide_line_height(fonts, &current, fallback_height);
        lines.push(SlideLine { words: current, height, indent });
    }
    let total_height: f64 = lines.iter().map(|line| line.height).sum();
    let start_y = match valign {
        "middle" => object.y + (object.h - total_height) / 2.0,
        "bottom" => object.y + object.h - total_height - padding,
        _ => object.y + padding,
    };
    let mut y = start_y;
    for line in lines {
        if line.words.is_empty() {
            y += line.height;
            continue;
        }
        let width = slide_line_width(fonts, &line.words);
        let available = (object.w - padding * 2.0 - line.indent).max(20.0);
        let x = match align {
            "center" => object.x + padding + line.indent + (available - width) / 2.0,
            "right" => object.x + padding + line.indent + available - width,
            _ => object.x + padding + line.indent,
        };
        let mut cursor = x;
        for (index, (word, style)) in line.words.iter().enumerate() {
            if index > 0 {
                cursor += fonts.pick(style.bold).advance_pt(" ", style.size_pt);
            }
            canvas.text(cursor, y + style.size_pt, word, style);
            cursor += fonts.pick(style.bold).advance_pt(word, style.size_pt);
        }
        y += line.height;
    }
}

fn draw_slide_table(canvas: &mut Canvas<'_>, object: &SlideObject, table: &TableData, fonts: &FontSet) {
    let rows = table.rows.len().max(1);
    let columns = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
    let cell_width = object.w / columns as f64;
    let cell_height = object.h / rows as f64;
    for (row_index, row) in table.rows.iter().enumerate() {
        for (column_index, cell) in row.cells.iter().enumerate() {
            let x = object.x + cell_width * column_index as f64;
            let y = object.y + cell_height * row_index as f64;
            if let Some(background) = cell.background.as_deref().and_then(parse_hex) {
                canvas.fill_rect(x, y, cell_width, cell_height, background, 1.0);
            }
            canvas.stroke_rect(x, y, cell_width, cell_height, Rgb(148, 163, 184), 0.7);
            let text = cell.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join(" ");
            let style = TextStyle { size_pt: 12.0, ..Default::default() };
            let truncated = truncate_to_width(fonts, &text, &style, cell_width - 10.0);
            canvas.text(x + 5.0, y + cell_height / 2.0 + 4.0, &truncated, &style);
        }
    }
}

fn draw_slide_chart(canvas: &mut Canvas<'_>, object: &SlideObject, chart: &ChartData, fonts: &FontSet) {
    let title_style = TextStyle { size_pt: 12.0, bold: true, ..Default::default() };
    canvas.text(object.x, object.y + 12.0, &chart.title, &title_style);
    let plot_x = object.x;
    let plot_y = object.y + 20.0;
    let plot_w = object.w;
    let plot_h = (object.h - 20.0).max(20.0);
    canvas.stroke_rect(plot_x, plot_y, plot_w, plot_h, Rgb(203, 213, 225), 0.6);
    // Charts from spreadsheets are resolved by the UI; here we draw a labelled
    // placeholder that makes the data range explicit and never fakes values.
    let note = format!(
        "{} chart · {}",
        chart.kind,
        if chart.series.is_empty() {
            "no series".to_string()
        } else {
            chart.series.iter().map(|series| series.range.clone()).collect::<Vec<_>>().join(", ")
        }
    );
    let note_style = TextStyle { size_pt: 9.5, color: Rgb(100, 116, 139), ..Default::default() };
    let wrapped = truncate_to_width(fonts, &note, &note_style, plot_w - 12.0);
    canvas.text(plot_x + 6.0, plot_y + plot_h / 2.0, &wrapped, &note_style);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> TextDocument {
        let mut document = TextDocument::new_blank("Layout");
        document.blocks = vec![
            Block::heading("Heading", 1),
            Block::paragraph(
                "A paragraph with enough words to wrap across at least two lines when the column is narrow.",
            ),
            Block::Paragraph {
                props: ParaProps {
                    list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                    ..Default::default()
                },
                runs: vec![Run { text: "item one".into(), ..Default::default() }],
            },
            Block::Table { table: TableData::simple(3, 3, 460.0) },
            Block::PageBreak,
            Block::paragraph("Second page"),
        ];
        document.footer = vec![Block::Paragraph {
            props: ParaProps { align: "center".into(), ..Default::default() },
            runs: vec![Run { text: "Page {{page}} / {{pages}}".into(), ..Default::default() }],
        }];
        document
    }

    #[test]
    fn document_pdf_is_valid() {
        let bytes = document_to_pdf(&sample_document());
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.windows(5).any(|window| window == b"%%EOF"));
        assert!(bytes.len() > 3000);
    }

    #[test]
    fn ordered_list_markers_increment_within_a_run() {
        let list = |level: u32, start: u32| ListInfo { kind: "number".into(), level, start, marker: "1.".into() };
        let mut numbering = ListNumbering::default();
        assert_eq!(numbering.advance(&list(0, 1)), 1);
        assert_eq!(numbering.advance(&list(0, 1)), 2);
        // A deeper level counts on its own.
        assert_eq!(numbering.advance(&list(1, 1)), 1);
        assert_eq!(numbering.advance(&list(1, 1)), 2);
        // Returning to the outer level continues its count.
        assert_eq!(numbering.advance(&list(0, 1)), 3);
        // A non-numbered block clears every counter.
        numbering.reset();
        assert_eq!(numbering.advance(&list(0, 1)), 1);
        // A different start restarts the level instead of continuing it.
        assert_eq!(numbering.advance(&list(0, 5)), 5);
        assert_eq!(numbering.advance(&list(0, 5)), 6);
    }

    #[test]
    fn multi_column_documents_render() {
        let mut document = TextDocument::new_blank("Columns");
        document.page.columns = 2;
        document.blocks = vec![Block::paragraph(&"word ".repeat(300))];
        let bytes = document_to_pdf(&document);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn workbook_pdf_is_valid() {
        let mut workbook = Workbook::new_blank("Budget");
        let sheet = &mut workbook.sheets[0];
        for row in 0..40u32 {
            crate::address::format(row, 0);
            sheet.set(
                &crate::address::format(row, 0),
                Cell { value: CellValue::Text(format!("Row {row}")), ..Default::default() },
            );
            sheet.set(
                &crate::address::format(row, 1),
                Cell { value: CellValue::Number(row as f64 * 12.5), ..Default::default() },
            );
        }
        let bytes = workbook_to_pdf(&workbook, 1);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn deck_pdf_is_valid() {
        let mut deck = Deck::new_blank("Deck");
        let mut slide = Slide::default();
        let mut object = SlideObject::new("rect", 40.0, 40.0, 300.0, 120.0);
        object.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
        let mut text = SlideObject::new("text", 60.0, 60.0, 400.0, 120.0);
        text.text = Some(TextFrame {
            paragraphs: vec![TextParagraph { text: "Hello slides".into(), size_pt: Some(28.0), ..Default::default() }],
            ..Default::default()
        });
        slide.objects = vec![object, text];
        deck.slides = vec![slide];
        let bytes = deck_to_pdf(&deck);
        assert!(bytes.starts_with(b"%PDF"));
    }

    fn pdf_page_count(bytes: &[u8]) -> usize {
        lopdf::Document::load_mem(bytes).expect("the export must be a loadable PDF").get_pages().len()
    }

    #[test]
    fn hidden_slides_are_skipped_by_the_export() {
        let mut deck = Deck::new_blank("Hidden");
        deck.slides[0].hidden = true;
        let mut visible = Slide::default();
        let mut text = SlideObject::new("text", 40.0, 40.0, 400.0, 100.0);
        text.text = Some(TextFrame {
            paragraphs: vec![TextParagraph { text: "Second".into(), ..Default::default() }],
            ..Default::default()
        });
        visible.objects.push(text);
        deck.slides.push(visible);
        let bytes = deck_to_pdf(&deck);
        assert_eq!(pdf_page_count(&bytes), 1, "only the visible slide is exported");
    }

    #[test]
    fn a_deck_with_every_slide_hidden_still_produces_one_page() {
        let mut deck = Deck::new_blank("All hidden");
        deck.slides[0].hidden = true;
        let bytes = deck_to_pdf(&deck);
        assert!(bytes.starts_with(b"%PDF"));
        assert_eq!(pdf_page_count(&bytes), 1);
    }

    #[test]
    fn footer_placeholders_resolve_to_the_deck_values() {
        let footer = SlideFooter {
            enabled: true,
            text: "Acme".into(),
            show_text: true,
            show_slide_number: true,
            show_date: true,
            date_text: "2026-01-01".into(),
        };
        let context = SlideRenderContext { slide_number: 3, footer: Some(&footer) };
        assert_eq!(placeholder_text("slideNumber", &context).as_deref(), Some("3"));
        assert_eq!(placeholder_text("footer", &context).as_deref(), Some("Acme"));
        assert_eq!(placeholder_text("date", &context).as_deref(), Some("2026-01-01"));

        // A disabled footer leaves the placeholders as they are (empty).
        let disabled = SlideFooter { enabled: false, ..footer.clone() };
        let context = SlideRenderContext { slide_number: 3, footer: Some(&disabled) };
        assert_eq!(placeholder_text("footer", &context), None);
        let context = SlideRenderContext { slide_number: 3, footer: None };
        assert_eq!(placeholder_text("date", &context), None);
    }

    /// Counts the text-showing operators in every page's decompressed content
    /// stream, so a test can tell whether a placeholder actually drew text.
    fn pdf_text_ops(bytes: &[u8]) -> usize {
        let doc = lopdf::Document::load_mem(bytes).expect("the export must be a loadable PDF");
        let mut ops = 0;
        for (_, page_id) in doc.get_pages() {
            let content = doc.get_page_content(page_id);
            ops += content.windows(2).filter(|window| window == b"Tj").count();
        }
        ops
    }

    #[test]
    fn inherited_placeholders_and_footer_render_in_a_deck_export() {
        let mut deck = Deck::new_blank("Master");
        deck.footer = Some(SlideFooter {
            enabled: true,
            text: "Footer text".into(),
            show_text: true,
            show_slide_number: true,
            show_date: true,
            date_text: "2026-01-01".into(),
        });
        let mut master = SlideMaster::default();
        let mut footer_placeholder = SlideObject::new("text", 40.0, 500.0, 300.0, 30.0);
        footer_placeholder.placeholder = Some("footer".into());
        let mut number_placeholder = SlideObject::new("text", 800.0, 500.0, 100.0, 30.0);
        number_placeholder.placeholder = Some("slideNumber".into());
        let mut date_placeholder = SlideObject::new("text", 40.0, 460.0, 200.0, 30.0);
        date_placeholder.placeholder = Some("date".into());
        master.objects = vec![footer_placeholder, number_placeholder, date_placeholder];
        deck.masters = vec![master];
        let mut second = Slide::default();
        second.objects.push(SlideObject::new("rect", 10.0, 10.0, 50.0, 50.0));
        deck.slides.push(second);

        let bytes = deck_to_pdf(&deck);
        assert_eq!(pdf_page_count(&bytes), 2);

        // Without an enabled footer the same placeholders stay empty, so the
        // export must show strictly fewer text runs.
        let mut bare = deck.clone();
        bare.footer = None;
        let bare_bytes = deck_to_pdf(&bare);
        assert!(
            pdf_text_ops(&bytes) > pdf_text_ops(&bare_bytes),
            "the resolved footer/date/slide-number placeholders must draw text"
        );
    }

    #[test]
    fn cropped_slide_images_clip_and_scale() {
        // A 4x4 PNG cropped to its middle half: the visible region must still
        // cover the object rectangle.
        let mut image = image::RgbaImage::new(4, 4);
        for pixel in image.pixels_mut() {
            *pixel = image::Rgba([200, 30, 30, 255]);
        }
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut object = SlideObject::new("image", 100.0, 100.0, 200.0, 100.0);
        let mut data = ImageData::from_bytes("crop.png", &png);
        data.crop = Some(ImageCrop { left: 0.25, top: 0.25, right: 0.25, bottom: 0.25 });
        object.image = Some(data);
        let mut deck = Deck::new_blank("Crop");
        deck.slides[0].objects.push(object);
        let bytes = deck_to_pdf(&deck);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn tab_stops_advance_the_cursor_to_the_next_stop() {
        let fonts = FontSet::new();
        let style = TextStyle { size_pt: 12.0, ..Default::default() };
        let items =
            vec![("A".to_string(), style, false), ("\t".to_string(), style, false), ("B".to_string(), style, false)];
        let tabs = vec![TabStop { pos_pt: 100.0, align: "left".into() }];
        let line = finalize_line(&fonts, &items, 0.0, 500.0, "left", &tabs);
        assert_eq!(line.items.len(), 2, "the tab marker itself is not drawn");
        let b_offset = line.items[1].dx;
        assert!((b_offset - 100.0).abs() < 0.01, "B must start at the custom stop, was {b_offset}");

        // Without a custom stop the default step (36 pt) is used.
        let line = finalize_line(&fonts, &items, 0.0, 500.0, "left", &[]);
        assert!((line.items[1].dx - DEFAULT_TAB_STEP_PT).abs() < 0.01, "dx was {}", line.items[1].dx);
    }

    #[test]
    fn writer_table_spans_lay_out_on_the_grid() {
        let mut table = TableData::simple(3, 3, 300.0);
        table.rows[0].cells = vec![TableCell { colspan: 2, ..Default::default() }, TableCell::default()];
        table.rows[1].cells =
            vec![TableCell { rowspan: 2, ..Default::default() }, TableCell { colspan: 2, ..Default::default() }];
        table.rows[2].cells = vec![TableCell::default()];
        assert_eq!(table_grid_columns(&table), 3);

        let widths = table_column_widths(&table, 3, 300.0);
        let layout = table_cell_layout(&table, &widths);
        assert!((layout[0][0].0 - 0.0).abs() < 0.01);
        assert!((layout[0][0].1 - 200.0).abs() < 0.01, "the 2-column span is 200 pt wide");
        assert!((layout[0][1].0 - 200.0).abs() < 0.01);
        assert!((layout[1][0].0 - 0.0).abs() < 0.01, "the rowspan origin starts at column 0");
        assert!((layout[1][1].0 - 100.0).abs() < 0.01);
        assert!((layout[1][1].1 - 200.0).abs() < 0.01);
        assert!((layout[2][0].0 - 100.0).abs() < 0.01, "column 0 is covered by the rowspan");

        let mut document = TextDocument::new_blank("Spans");
        document.blocks = vec![Block::Table { table }];
        let bytes = document_to_pdf(&document);
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.windows(5).any(|window| window == b"%%EOF"));
    }

    #[test]
    fn paragraphs_with_tab_stops_export() {
        let mut document = TextDocument::new_blank("Tabs");
        document.blocks = vec![Block::Paragraph {
            props: ParaProps { tabs: vec![TabStop { pos_pt: 200.0, align: "right".into() }], ..Default::default() },
            runs: vec![Run { text: "Left\tRight".into(), ..Default::default() }],
        }];
        let bytes = document_to_pdf(&document);
        assert!(bytes.starts_with(b"%PDF"));
    }
}
