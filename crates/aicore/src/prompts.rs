//! Prompt building, text chunking and retrieval for the AI features.
//!
//! Keeping prompts here makes them reviewable in one place and lets the UI
//! show the user exactly what kind of instruction will be sent.

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::{AiError, AiResult, ChatMessage};

/// Default character budget per request when the caller does not derive one
/// from the configured context window (about 16k tokens).
pub const CHUNK_CHARS: usize = 40_000;
/// Smallest chunk we ever produce.
const MIN_CHUNK_CHARS: usize = 4_000;

pub const SUMMARY_SYSTEM: &str =
    "You are a precise document analyst. Summarize only what the text says, never invent facts, \
and keep names, numbers and dates exactly as written. Reply in the requested language.";

pub const TRANSLATE_SYSTEM: &str =
    "You are a professional translator. Translate faithfully and completely, keep the original \
paragraph structure and line breaks, do not add commentary, notes or headings of your own. \
Keep numbers, dates, e-mail addresses and URLs unchanged.";

pub const ASK_SYSTEM: &str =
    "You answer questions strictly from the provided document excerpts. If the answer is not in \
them, say that the document does not contain it. Cite the page numbers you used, like (p. 4).";

pub const CLEANUP_SYSTEM: &str =
    "You repair OCR text: fix broken words, hyphenation and spacing, keep the wording, and use \
paragraph breaks where the layout suggests them. Never summarize or drop content.";

pub const METADATA_SYSTEM: &str =
    "You extract document metadata. Reply with a single JSON object and nothing else, using the \
keys title, author, subject, keywords (array of 3-8 short strings). Use empty strings or an \
empty array when a value is not present in the text.";

pub const MAP_SYSTEM: &str = "Summarize this part of a larger document in 3-6 sentences. Keep every important number, \
name and date. Do not add introductions or conclusions.";

pub const REDUCE_SYSTEM: &str =
    "You combine partial summaries into one coherent final summary. Remove repetition, keep all \
key facts, and follow the style and length requested by the user.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryLength {
    Short,
    Medium,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryStyle {
    Paragraph,
    Bullets,
    Executive,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryOptions {
    /// "auto" keeps the document language, otherwise an ISO code or plain name.
    #[serde(default = "auto_language")]
    pub language: String,
    #[serde(default = "medium_length")]
    pub length: SummaryLength,
    #[serde(default = "paragraph_style")]
    pub style: SummaryStyle,
    /// Optional user focus, e.g. "focus on payment terms".
    #[serde(default)]
    pub focus: String,
}

fn auto_language() -> String {
    "auto".into()
}
fn medium_length() -> SummaryLength {
    SummaryLength::Medium
}
fn paragraph_style() -> SummaryStyle {
    SummaryStyle::Paragraph
}

impl Default for SummaryOptions {
    fn default() -> Self {
        Self { language: auto_language(), length: medium_length(), style: paragraph_style(), focus: String::new() }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateOptions {
    /// Target language name or ISO code, e.g. "tr" or "Turkish".
    pub target_language: String,
    /// Keep the original text under each translated page.
    #[serde(default)]
    pub bilingual: bool,
}

/// A ready-to-run request: either one call or a map/reduce pipeline.
pub enum Plan {
    Single { messages: Vec<ChatMessage> },
    MapReduce { chunks: Vec<String>, reduce: String },
}

/// Splits a document into chunks on paragraph boundaries.
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(MIN_CHUNK_CHARS);
    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.len() > max_chars {
            // Hard-split very long paragraphs.
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            let mut remaining = paragraph;
            while remaining.len() > max_chars {
                let split_at = floor_char_boundary(remaining, max_chars);
                chunks.push(remaining[..split_at].to_string());
                remaining = &remaining[split_at..];
            }
            current.push_str(remaining);
            continue;
        }
        if current.len() + paragraph.len() + 2 > max_chars {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(text.to_string());
    }
    chunks
}

fn floor_char_boundary(text: &str, limit: usize) -> usize {
    if limit >= text.len() {
        return text.len();
    }
    let mut index = limit;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn language_instruction(language: &str) -> String {
    let trimmed = language.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        "Reply in the same language as the document.".to_string()
    } else {
        format!("Reply in {trimmed}.")
    }
}

fn length_instruction(length: SummaryLength) -> &'static str {
    match length {
        SummaryLength::Short => "Target length: 3-5 sentences (about 80 words).",
        SummaryLength::Medium => "Target length: 150-250 words.",
        SummaryLength::Detailed => "Target length: 400-600 words with sub-headings.",
    }
}

fn style_instruction(style: SummaryStyle) -> &'static str {
    match style {
        SummaryStyle::Paragraph => "Write flowing paragraphs.",
        SummaryStyle::Bullets => "Write a short intro line followed by bullet points.",
        SummaryStyle::Executive => "Write an executive brief: one-line takeaway first, then key facts as bullets.",
    }
}

pub fn summarize_prompt(text: &str, options: &SummaryOptions) -> Plan {
    summarize_prompt_with_budget(text, options, CHUNK_CHARS)
}

/// Same as [`summarize_prompt`] but with an explicit per-request character
/// budget (derived from the configured context window, up to ~1M tokens).
pub fn summarize_prompt_with_budget(text: &str, options: &SummaryOptions, chunk_chars: usize) -> Plan {
    let budget = chunk_chars.max(MIN_CHUNK_CHARS);
    let mut instruction = String::from(
        "Summarize the document text below.\n\
         - Cover the purpose, key facts, figures and any decisions or deadlines.\n\
         - Do not invent anything that is not in the text.\n",
    );
    instruction.push_str(&format!("- {}\n", language_instruction(&options.language)));
    instruction.push_str(&format!("- {}\n", length_instruction(options.length)));
    instruction.push_str(&format!("- {}\n", style_instruction(options.style)));
    if !options.focus.trim().is_empty() {
        instruction.push_str(&format!("- Pay special attention to: {}\n", options.focus.trim()));
    }
    instruction.push_str("\nDocument text:\n");

    if text.len() <= budget {
        return Plan::Single {
            messages: vec![ChatMessage::system(SUMMARY_SYSTEM), ChatMessage::user(format!("{instruction}{text}"))],
        };
    }
    Plan::MapReduce { chunks: chunk_text(text, CHUNK_CHARS), reduce: instruction }
}

pub fn translate_prompt(chunks: &[String], options: &TranslateOptions) -> Vec<ChatMessage> {
    let target = options.target_language.trim();
    let instruction = format!(
        "Translate the text between the markers into {target}. Keep the page markers unchanged \
and translate everything else. Do not add any text of your own.\n\n",
    );
    chunks
        .iter()
        .flat_map(|chunk| {
            vec![
                ChatMessage::system(TRANSLATE_SYSTEM),
                ChatMessage::user(format!("{instruction}<<<TEXT\n{chunk}\nTEXT>>>")),
            ]
        })
        .collect()
}

/// Builds a single translation request for one page (streaming friendly).
pub fn translate_page_prompt(page_marker: &str, text: &str, options: &TranslateOptions) -> Vec<ChatMessage> {
    let target = options.target_language.trim();
    let bilingual = if options.bilingual {
        "\nAfter the translation, add a blank line and then the original text unchanged under the heading 'Original'."
    } else {
        ""
    };
    vec![
        ChatMessage::system(TRANSLATE_SYSTEM),
        ChatMessage::user(format!(
            "Translate the following text into {target}. Keep paragraphs and line breaks.{bilingual}\n\n\
             Page marker: {page_marker}\n\n<<<TEXT\n{text}\nTEXT>>>",
        )),
    ]
}

pub fn ask_prompt(context: &str, question: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(ASK_SYSTEM),
        ChatMessage::user(format!("Document excerpts:\n\n{context}\n\nQuestion: {question}\n\nAnswer:")),
    ]
}

pub fn cleanup_prompt(text: &str) -> Vec<ChatMessage> {
    vec![ChatMessage::system(CLEANUP_SYSTEM), ChatMessage::user(format!("Repair the following OCR text:\n\n{text}"))]
}

pub fn metadata_prompt(text: &str) -> Vec<ChatMessage> {
    let excerpt: String = text.chars().take(8_000).collect();
    vec![ChatMessage::system(METADATA_SYSTEM), ChatMessage::user(format!("Document text:\n\n{excerpt}"))]
}

/// Very small keyword retriever used by the Q&A feature: scores pages by the
/// number of question terms they contain and returns the best ones.
pub fn select_relevant_pages<'a>(pages: &'a [(u32, String)], question: &str, max_chars: usize) -> Vec<(u32, &'a str)> {
    let terms: Vec<String> = question
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.len() > 2)
        .map(|term| term.to_string())
        .collect();

    let mut scored: Vec<(usize, u32, &str)> = pages
        .iter()
        .map(|(page, text)| {
            let lower = text.to_lowercase();
            let score: usize = terms.iter().map(|term| lower.matches(term.as_str()).count()).sum();
            (score, *page, text.as_str())
        })
        .collect();
    scored.sort_by_key(|entry| std::cmp::Reverse(entry.0));

    let mut selected: Vec<(u32, &str)> = Vec::new();
    let mut used = 0usize;
    for (score, page, text) in scored {
        if used + text.len() > max_chars && !selected.is_empty() {
            continue;
        }
        if score == 0 && !selected.is_empty() {
            break;
        }
        used += text.len();
        selected.push((page, text));
        if used >= max_chars {
            break;
        }
    }
    if selected.is_empty() {
        pages.iter().take(1).map(|(page, text)| (*page, text.as_str())).collect()
    } else {
        selected.sort_by_key(|(page, _)| *page);
        selected
    }
}

/// Formats page texts into the "page marker + text" shape used by prompts and
/// the Markdown exports.
pub fn format_pages(pages: &[(u32, String)], with_headers: bool) -> String {
    pages
        .iter()
        .map(|(page, text)| {
            if with_headers {
                format!("## Page {page}\n\n{}", text.trim())
            } else {
                format!("[page {page}]\n{}", text.trim())
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Parses the JSON reply of the metadata prompt.
pub fn parse_metadata_reply(reply: &str) -> Option<MetadataSuggestion> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let slice = &reply[start..=end];
    let value: serde_json::Value = serde_json::from_str(slice).ok()?;
    let string_field = |key: &str| value.get(key).and_then(|item| item.as_str()).unwrap_or_default().trim().to_string();
    let keywords = value
        .get("keywords")
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(MetadataSuggestion {
        title: string_field("title"),
        author: string_field("author"),
        subject: string_field("subject"),
        keywords,
    })
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSuggestion {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: Vec<String>,
}

// ---------------------------------------------------------------------------
// Document chat
// ---------------------------------------------------------------------------

pub const DOC_CHAT_SYSTEM: &str =
    "You answer questions about a document strictly from the provided context. Use only facts that \
appear in the context, cite the pages you used like [page 3], and never invent page numbers. If the \
answer is not in the context, reply exactly: \"I could not find this in the document.\"";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocChatMessage {
    pub role: String,
    pub content: String,
}

impl DocChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

/// Builds the request for a document chat turn: system rules, the retrieved
/// context, the previous conversation, then the new question.
pub fn doc_chat_messages(question: &str, context: &[(u32, String)], history: &[DocChatMessage]) -> Vec<ChatMessage> {
    let context_text =
        context.iter().map(|(page, text)| format!("[page {page}]\n{}", text.trim())).collect::<Vec<_>>().join("\n\n");
    let mut messages = Vec::with_capacity(history.len() + 3);
    messages.push(ChatMessage::system(DOC_CHAT_SYSTEM));
    messages.push(ChatMessage::user(format!("Document context:\n\n{context_text}")));
    for item in history {
        messages.push(ChatMessage { role: item.role.clone(), content: item.content.clone(), image: None });
    }
    messages.push(ChatMessage::user(question));
    messages
}

/// Keyword retrieval for document chat: scores pages by how many question
/// terms they contain, keeps the best `max_pages`, and falls back to the first
/// pages when nothing matches.
pub fn doc_chat_focus(question: &str, pages: &[(u32, String)], max_pages: usize) -> Vec<(u32, String)> {
    if max_pages == 0 || pages.is_empty() {
        return Vec::new();
    }
    let terms: Vec<String> = question
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.len() > 2)
        .map(|term| term.to_string())
        .collect();

    let lower: Vec<String> = pages.iter().map(|(_, text)| text.to_lowercase()).collect();
    let mut scored: Vec<(usize, u32)> = pages
        .iter()
        .enumerate()
        .map(|(index, (page, _))| {
            let score: usize = terms.iter().map(|term| lower[index].matches(term.as_str()).count()).sum();
            (score, *page)
        })
        .collect();
    let has_hits = scored.iter().any(|(score, _)| *score > 0);
    if !has_hits {
        return pages.iter().take(max_pages).cloned().collect();
    }

    scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    scored.truncate(max_pages);
    scored.sort_by_key(|(_, page)| *page);
    scored
        .into_iter()
        .map(|(_, page)| {
            let (_, text) = pages.iter().find(|(candidate, _)| *candidate == page).expect("page present");
            (page, text.clone())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Writer actions
// ---------------------------------------------------------------------------

pub const WRITER_SYSTEM: &str =
    "You are a precise text editor. Apply the requested transformation and reply with only the \
transformed text: no explanations, no notes, no surrounding quotes and no markdown fences. Never \
add facts that are not in the original text.";

/// Transformation prompts for the Writer: rewrite, formalize, simplify,
/// shorten, expand, grammar, translate (optionally `translate:de`) and
/// continue. Unknown actions return [`AiError::Unsupported`].
pub fn writer_action_prompt(action: &str, text: &str) -> AiResult<Vec<ChatMessage>> {
    let normalized = action.trim().to_lowercase();
    let instruction = match normalized.as_str() {
        "rewrite" => "Rewrite the text so it reads clearly and smoothly. Keep the meaning, the facts and roughly the original length.".to_string(),
        "formalize" => "Rewrite the text in a formal, professional tone. Keep the meaning and every fact.".to_string(),
        "simplify" => "Rewrite the text in simple, plain language with short sentences. Keep the meaning and every fact.".to_string(),
        "shorten" => "Shorten the text while keeping every key fact, number and name.".to_string(),
        "expand" => "Expand the text with more detail and clearer transitions. Do not invent facts.".to_string(),
        "grammar" => "Fix spelling, grammar, punctuation and capitalization. Change nothing else.".to_string(),
        "continue" => "Continue writing the text naturally from where it ends. Match the tone and style.".to_string(),
        _ if normalized == "translate"
            || normalized.starts_with("translate:")
            || normalized.starts_with("translate_") =>
        {
            let target = normalized
                .strip_prefix("translate:")
                .or_else(|| normalized.strip_prefix("translate_"))
                .filter(|target| !target.is_empty())
                .unwrap_or("English");
            format!("Translate the text into {target}. Keep the meaning, the formatting and any numbers or names.")
        }
        _ => return Err(AiError::Unsupported(format!("unknown writer action: {normalized}"))),
    };
    Ok(vec![ChatMessage::system(WRITER_SYSTEM), ChatMessage::user(format!("{instruction}\n\nText:\n{text}"))])
}

// ---------------------------------------------------------------------------
// Calculator actions
// ---------------------------------------------------------------------------

pub const CALC_SYSTEM: &str =
    "You are a spreadsheet formula assistant. Use English function names and comma separators, \
like =SUM(A1:A10). Never invent cell contents that were not provided in the context.";

/// Formula prompts for Calc: explain_formula, suggest_formula, repair_formula,
/// explain_error and generate_formula. The formula-producing actions ask the
/// model for exactly one fenced ```formula block.
pub fn calc_action_prompt(action: &str, question: &str, context: &str) -> AiResult<Vec<ChatMessage>> {
    let normalized = action.trim().to_lowercase();
    let instruction = match normalized.as_str() {
        "explain_formula" => "Explain what the formula in the context does, step by step. Quote the formula first in a single fenced ```formula block, then explain it briefly.",
        "suggest_formula" => "Suggest the best formula for the request. Reply with exactly one fenced ```formula block and nothing else.",
        "repair_formula" => "Repair the broken formula. Reply with exactly one fenced ```formula block and nothing else.",
        "generate_formula" => "Write the formula for the request. Reply with exactly one fenced ```formula block and nothing else.",
        "explain_error" => "Explain the spreadsheet error described in the question and how to fix it. Do not invent cell contents.",
        _ => return Err(AiError::Unsupported(format!("unknown calculator action: {normalized}"))),
    };
    let mut prompt = String::new();
    if !context.trim().is_empty() {
        prompt.push_str("Context:\n");
        prompt.push_str(context.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str(&format!("Request: {}\n\n{instruction}", question.trim()));
    Ok(vec![ChatMessage::system(CALC_SYSTEM), ChatMessage::user(prompt)])
}

fn fenced_block(text: &str, language: &str) -> Option<String> {
    let start = text.find("```")?;
    let after = &text[start + 3..];
    let after = {
        let position = after.find('\n')?;
        let tag = after[..position].trim();
        if !tag.is_empty() && !tag.eq_ignore_ascii_case(language) {
            return None;
        }
        &after[position + 1..]
    };
    let end = after.find("```")?;
    Some(after[..end].to_string())
}

/// Extracts a formula from a model reply: strips a ```formula / ``` fence when
/// present, keeps a leading `=`, and trims. Replies without any formula return
/// `None`.
pub fn parse_formula_reply(reply: &str) -> Option<String> {
    let candidate = fenced_block(reply, "formula")
        .or_else(|| reply.lines().map(str::trim).find(|line| line.starts_with('=')).map(|line| line.to_string()))?;
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ---------------------------------------------------------------------------
// Document actions
// ---------------------------------------------------------------------------

pub const DOC_ACTION_SYSTEM: &str =
    "You are a precise document analyst. Work only from the text you are given, never invent \
facts, and keep names, numbers and dates exactly as written.";

/// Document actions: summarize, extract_table (TSV), extract_dates,
/// extract_names and action_items (JSON arrays).
pub fn doc_action_prompt(action: &str, text: &str) -> AiResult<Vec<ChatMessage>> {
    let normalized = action.trim().to_lowercase();
    let instruction = match normalized.as_str() {
        "summarize" => "Summarize the document text below. Cover the purpose, key facts, figures and dates, and any decisions or deadlines. Do not invent anything that is not in the text.",
        "extract_table" => "Extract the table from the document text. Reply with TSV (tab-separated values): one row per line, cells separated by single tabs, including a header row when the text has one. No markdown fences, no commentary.",
        "extract_dates" => "Extract every date mentioned in the document text. Reply with a JSON array of strings in document order, and nothing else.",
        "extract_names" => "Extract every person, company and organization name mentioned in the document text. Reply with a JSON array of strings in document order, and nothing else.",
        "action_items" => "Extract the action items from the document text. Reply with a JSON array of objects using the keys \"task\", \"owner\" and \"deadline\" (empty string when unknown), and nothing else.",
        _ => return Err(AiError::Unsupported(format!("unknown document action: {normalized}"))),
    };
    Ok(vec![
        ChatMessage::system(DOC_ACTION_SYSTEM),
        ChatMessage::user(format!("{instruction}\n\nDocument text:\n{text}")),
    ])
}

/// Summarizes the current selection, in the requested language ("auto" keeps
/// the language of the selection).
pub fn summarize_selection_prompt(text: &str, language: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(SUMMARY_SYSTEM),
        ChatMessage::user(format!(
            "Summarize the selected text below.\n- {}\n- Do not invent anything that is not in the text.\n\nSelected text:\n{text}",
            language_instruction(language)
        )),
    ]
}

// ---------------------------------------------------------------------------
// Invoice & receipt extraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceTableRow {
    #[serde(default, deserialize_with = "de_lenient_page")]
    pub page: u32,
    #[serde(default, deserialize_with = "de_lenient_string")]
    pub vendor: String,
    #[serde(default, deserialize_with = "de_lenient_string")]
    pub date: String,
    #[serde(default, alias = "invoice_number", deserialize_with = "de_lenient_string")]
    pub invoice_number: String,
    #[serde(default, deserialize_with = "de_lenient_string")]
    pub currency: String,
    #[serde(default, deserialize_with = "de_lenient_money")]
    pub subtotal: Option<f64>,
    #[serde(default, deserialize_with = "de_lenient_money")]
    pub tax: Option<f64>,
    #[serde(default, deserialize_with = "de_lenient_money")]
    pub total: Option<f64>,
    #[serde(default, alias = "line_items", deserialize_with = "de_lenient_string")]
    pub line_items: String,
    #[serde(default, alias = "custom_fields", deserialize_with = "de_lenient_custom_fields")]
    pub custom_fields: BTreeMap<String, String>,
}

/// Accepts a page number as a JSON number or a numeric string; anything else
/// falls back to 0 so a single odd field does not drop the whole row.
fn de_lenient_page<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    struct PageVisitor;
    impl<'de> Visitor<'de> for PageVisitor {
        type Value = u32;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a page number or a numeric string")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u32, E> {
            Ok(u32::try_from(v).unwrap_or(0))
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u32, E> {
            Ok(u32::try_from(v).unwrap_or(0))
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u32, E> {
            Ok(if v.is_finite() && v >= 0.0 && v.fract() == 0.0 { v as u32 } else { 0 })
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u32, E> {
            Ok(v.trim().parse().unwrap_or(0))
        }
        fn visit_unit<E: de::Error>(self) -> Result<u32, E> {
            Ok(0)
        }
        fn visit_none<E: de::Error>(self) -> Result<u32, E> {
            Ok(0)
        }
    }
    deserializer.deserialize_any(PageVisitor)
}

/// Accepts strings, numbers and booleans and normalises them to a string.
fn de_lenient_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringVisitor;
    impl<'de> Visitor<'de> for StringVisitor {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string or scalar value")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            Ok(v)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_bool<E: de::Error>(self, v: bool) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_unit<E: de::Error>(self) -> Result<String, E> {
            Ok(String::new())
        }
        fn visit_none<E: de::Error>(self) -> Result<String, E> {
            Ok(String::new())
        }
    }
    deserializer.deserialize_any(StringVisitor)
}

/// Accepts amounts as JSON numbers, numeric strings with Turkish or English
/// separators (`1.250,00`, `1,250.00`, `1250 TL`, `₺1.500`) or null.
fn de_lenient_money<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct MoneyVisitor;
    impl<'de> Visitor<'de> for MoneyVisitor {
        type Value = Option<f64>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an amount as a number, a numeric string or null")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, inner: D2) -> Result<Self::Value, D2::Error> {
            inner.deserialize_any(self)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok(v.is_finite().then_some(v))
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v as f64))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v as f64))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(parse_money_str(v))
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            Ok(parse_money_str(&v))
        }
    }
    deserializer.deserialize_option(MoneyVisitor)
}

/// Normalises an amount written with Turkish or English separators. The last
/// separator in the string is treated as the decimal separator; a lone
/// separator followed by exactly three digits is treated as grouping
/// (`1.250` -> 1250), matching Turkish invoices. `None` when no digits remain.
fn parse_money_str(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let negative = (trimmed.starts_with('(') && trimmed.ends_with(')')) || trimmed.starts_with('-');
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    if digits.is_empty() {
        return None;
    }
    let dot = digits.rfind('.');
    let comma = digits.rfind(',');
    let normalized = match (dot, comma) {
        (Some(d), Some(c)) => {
            if d > c {
                digits.replace(',', "")
            } else {
                digits.replace('.', "").replacen(',', ".", 1)
            }
        }
        (Some(d), None) => {
            let after = digits.len() - d - 1;
            let before = &digits[..d];
            if digits.matches('.').count() > 1 || (after == 3 && before != "0" && !before.is_empty()) {
                digits.replace('.', "")
            } else {
                digits.clone()
            }
        }
        (None, Some(c)) => {
            let after = digits.len() - c - 1;
            let before = &digits[..c];
            if digits.matches(',').count() > 1 || (after == 3 && before != "0" && !before.is_empty()) {
                digits.replace(',', "")
            } else {
                digits.replacen(',', ".", 1)
            }
        }
        (None, None) => digits.clone(),
    };
    let value: f64 = normalized.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// Accepts a JSON object whose values may be strings, numbers or booleans;
/// values are normalised to strings. `null` becomes an empty map.
fn de_lenient_custom_fields<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<BTreeMap<String, serde_json::Value>>::deserialize(deserializer)?;
    Ok(raw.unwrap_or_default().into_iter().map(|(key, value)| (key, json_scalar_to_string(value))).collect())
}

fn json_scalar_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The model reply could not be turned into invoice rows. Carries the raw
/// reply so the caller can show exactly what the model sent instead of
/// silently reporting zero rows.
#[derive(Debug, Clone, PartialEq)]
pub struct InvoiceParseError {
    pub detail: String,
    pub raw_reply: String,
}

impl InvoiceParseError {
    fn new(detail: impl Into<String>, raw_reply: &str) -> Self {
        Self { detail: detail.into(), raw_reply: raw_reply.to_string() }
    }
}

impl fmt::Display for InvoiceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not parse the invoice extraction reply ({}). Raw model reply:\n{}",
            self.detail, self.raw_reply
        )
    }
}

impl std::error::Error for InvoiceParseError {}

pub const INVOICE_EXTRACT_SYSTEM: &str =
    "You are an expert at structured data extraction from financial documents (invoices, receipts, bills). \
Extract all invoice/receipt data from the provided document pages and output strictly valid JSON with no markdown formatting and no extra text. \
The JSON must be an object with a \"rows\" array of objects containing: \
page (number), vendor (string), date (string), invoiceNumber (string), currency (string), subtotal (number or null), tax (number or null), total (number or null), lineItems (string summary), customFields (key-value object of strings).";

pub fn invoice_extract_prompt(pages: &[(u32, String)], extra_fields: &[String]) -> Vec<ChatMessage> {
    let pages_text = format_pages(pages, false);
    let mut prompt = format!("Document pages:\n\n{pages_text}\n\nExtract all invoice and receipt rows as JSON.");
    if !extra_fields.is_empty() {
        prompt.push_str(&format!(
            "\nPlease also extract these additional fields into customFields: {}",
            extra_fields.join(", ")
        ));
    }
    vec![ChatMessage::system(INVOICE_EXTRACT_SYSTEM), ChatMessage::user(prompt)]
}

/// Parses the model reply into invoice rows. Rows are parsed one by one: a
/// single malformed row does not drop the others. A reply that yields no rows
/// at all (no JSON, wrong shape, or only unparsable rows) returns
/// [`InvoiceParseError`] carrying the raw reply, so the UI can surface it
/// instead of silently showing an empty table.
pub fn parse_invoice_extract_reply(reply: &str) -> Result<Vec<InvoiceTableRow>, InvoiceParseError> {
    let json_str = strip_json_fence(reply).trim();
    if json_str.is_empty() {
        return Err(InvoiceParseError::new("the reply was empty", reply));
    }
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| InvoiceParseError::new(format!("invalid JSON: {e}"), reply))?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(mut map) => match map.remove("rows") {
            Some(serde_json::Value::Array(items)) => items,
            Some(other) => {
                return Err(InvoiceParseError::new(
                    format!("\"rows\" must be an array, got {}", json_kind(&other)),
                    reply,
                ))
            }
            None => return Err(InvoiceParseError::new("the JSON has no \"rows\" array", reply)),
        },
        other => {
            return Err(InvoiceParseError::new(
                format!("the reply must be a JSON object or array, got {}", json_kind(&other)),
                reply,
            ))
        }
    };

    let mut rows = Vec::with_capacity(items.len());
    let mut failures: Vec<String> = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        match serde_json::from_value::<InvoiceTableRow>(item) {
            Ok(row) => rows.push(row),
            Err(err) => failures.push(format!("#{}: {err}", index + 1)),
        }
    }
    if rows.is_empty() && !failures.is_empty() {
        return Err(InvoiceParseError::new(format!("no row could be parsed ({})", failures.join("; ")), reply));
    }
    Ok(rows)
}

/// Returns the body of the first ``` / ```json fenced block, or the input.
fn strip_json_fence(reply: &str) -> &str {
    let cleaned = reply.trim();
    if let Some(start) = cleaned.find("```json") {
        let after = &cleaned[start + 7..];
        return after.find("```").map_or(after, |end| &after[..end]);
    }
    if let Some(start) = cleaned.find("```") {
        let after = &cleaned[start + 3..];
        return after.find("```").map_or(after, |end| &after[..end]);
    }
    cleaned
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

// ---------------------------------------------------------------------------
// Document classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationResult {
    pub label: String,
    pub reason: String,
}

pub const CLASSIFY_SYSTEM: &str =
    "You are a document classification assistant. Classify the provided text into exactly one of the given candidate labels. \
Reply strictly with a JSON object: {\"label\": \"<chosen_label>\", \"reason\": \"<brief_justification>\"} with no markdown formatting and no other text.";

pub fn classify_document_prompt(text: &str, labels: &[String]) -> Vec<ChatMessage> {
    let sample = if text.len() > 16_000 {
        let mut boundary = 16_000;
        while boundary > 0 && !text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &text[..boundary]
    } else {
        text
    };
    let prompt = format!("Candidate labels: {}\n\nDocument text:\n\n{}", labels.join(", "), sample);
    vec![ChatMessage::system(CLASSIFY_SYSTEM), ChatMessage::user(prompt)]
}

pub fn parse_classification_reply(reply: &str, fallback_label: &str) -> ClassificationResult {
    let cleaned = reply.trim();
    let json_str = if let Some(start) = cleaned.find('{') {
        if let Some(end) = cleaned.rfind('}') {
            &cleaned[start..=end]
        } else {
            cleaned
        }
    } else {
        cleaned
    };
    if let Ok(res) = serde_json::from_str::<ClassificationResult>(json_str) {
        return res;
    }
    ClassificationResult { label: fallback_label.to_string(), reason: reply.trim().to_string() }
}
