import { fileBaseName } from "../../lib/format";
import type { Navigate } from "../../lib/nav";
import { useOfficeTabs } from "../../lib/office-store";
import { newParaBlock, newTextDocument, type TextDocument } from "../../lib/office-types";

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

/** Creates a new Writer tab from an AI answer and navigates to the Office workspace. */
export function openAnswerInWriter(docPath: string, answer: string, onNavigate?: Navigate): string {
  const doc = buildWriterDocumentFromAnswer(docPath, answer);
  const tabId = useOfficeTabs.getState().create("writer", doc.title, doc);
  onNavigate?.("office");
  return tabId;
}
