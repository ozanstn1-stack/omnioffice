import { fileBaseName } from "../../lib/format";
import type { Navigate } from "../../lib/nav";
import { useOfficeTabs } from "../../lib/office-store";
import { newParaBlock, newTextDocument, type TextDocument } from "../../lib/office-types";

export interface ReaderConversationEntry {
  role: "user" | "assistant";
  content: string;
  summary?: boolean;
}

/** Formats a selected PDF passage as a draft AI question for ReaderAiPanel. */
export function formatSelectionPrompt(excerpt: string, page: number, language: string): string {
  const normalized = excerpt.replace(/\s+/g, " ").trim().slice(0, 800);
  const safePage = Math.max(1, Math.round(page) || 1);
  if (language === "tr") {
    return `Sayfa ${safePage} içindeki şu seçili bölümü açıkla:\n"${normalized}"`;
  }
  return `Explain this selected passage from page ${safePage}:\n"${normalized}"`;
}

/** Builds a Writer TextDocument from an AI assistant answer while keeping line breaks. */
export function buildWriterDocumentFromAnswer(docName: string, answer: string): TextDocument {
  const base = fileBaseName(docName) || "Document";
  const doc = newTextDocument(`${base} — AI`);
  const paragraphs = answer
    .split(/\n{2,}/)
    .map((chunk) => chunk.trim())
    .filter(Boolean);

  doc.blocks = [
    newParaBlock("Heading 2", base),
    ...(paragraphs.length > 0
      ? paragraphs.map((text) => newParaBlock("Normal", text))
      : [newParaBlock("Normal", answer.trim())]),
  ];
  return doc;
}

/** Builds a Writer TextDocument from a selected Reader excerpt with its source page number. */
export function buildWriterDocumentFromExcerpt(docName: string, excerpt: string, page: number): TextDocument {
  const base = fileBaseName(docName) || "Document";
  const safePage = Math.max(1, Math.round(page) || 1);
  const doc = newTextDocument(`${base} — p.${safePage}`);
  const normalized = excerpt.replace(/\r\n/g, "\n").trim();
  const paragraphs = normalized
    .split(/\n{2,}/)
    .map((chunk) => chunk.trim())
    .filter(Boolean);

  doc.blocks = [
    newParaBlock("Heading 2", `${base} (p. ${safePage})`),
    ...(paragraphs.length > 0
      ? paragraphs.map((text) => newParaBlock("Normal", text))
      : [newParaBlock("Normal", normalized)]),
  ];
  return doc;
}

/** Formats a multi-turn Reader AI session as portable Markdown. */
export function formatConversationMarkdown(docName: string, entries: readonly ReaderConversationEntry[]): string {
  const base = fileBaseName(docName) || "Document";
  const turns = entries.filter((entry) => entry.content.trim());
  const lines = [`# ${base}`, ""];
  for (const entry of turns) {
    lines.push(entry.role === "user" ? "## Question" : "## Answer", "", entry.content.trim(), "");
  }
  return lines.join("\n").trim();
}

/** Builds a structured Writer report across all non-empty turns in a Reader AI session. */
export function buildWriterDocumentFromConversation(
  docName: string,
  entries: readonly ReaderConversationEntry[],
): TextDocument {
  const base = fileBaseName(docName) || "Document";
  const doc = newTextDocument(`${base} — AI Report`);
  const turns = entries.filter((entry) => entry.content.trim());
  const blocks = [newParaBlock("Heading 2", base)];

  for (const entry of turns) {
    if (entry.role === "user") {
      blocks.push(newParaBlock("Heading 3", entry.content.trim()));
      continue;
    }
    const paragraphs = entry.content
      .split(/\n{2,}/)
      .map((chunk) => chunk.trim())
      .filter(Boolean);
    blocks.push(...paragraphs.map((text) => newParaBlock("Normal", text)));
  }

  doc.blocks = blocks;
  return doc;
}

/** Creates a new Writer tab from an AI answer and navigates to the Office workspace. */
export function openAnswerInWriter(docPath: string, answer: string, onNavigate?: Navigate): string {
  const doc = buildWriterDocumentFromAnswer(docPath, answer);
  const tabId = useOfficeTabs.getState().create("writer", doc.title, doc);
  onNavigate?.("office");
  return tabId;
}

/** Sends a selected Reader passage to a new Writer tab and opens Office. */
export function openExcerptInWriter(docPath: string, excerpt: string, page: number, onNavigate?: Navigate): string {
  const doc = buildWriterDocumentFromExcerpt(docPath, excerpt, page);
  const tabId = useOfficeTabs.getState().create("writer", doc.title, doc);
  onNavigate?.("office");
  return tabId;
}

/** Exports the entire Reader AI chat transcript to a new Writer report tab. */
export function openConversationInWriter(
  docPath: string,
  entries: readonly ReaderConversationEntry[],
  onNavigate?: Navigate,
): string {
  const doc = buildWriterDocumentFromConversation(docPath, entries);
  const tabId = useOfficeTabs.getState().create("writer", doc.title, doc);
  onNavigate?.("office");
  return tabId;
}
