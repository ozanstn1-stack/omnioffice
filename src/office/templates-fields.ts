/**
 * Writer templates that ask the user for values: every placeholder here is
 * filled by TemplateFieldsDialog, and the same `{{alan}}` syntax the file-based
 * mail merge reads. Kept apart from `templates.ts` so the big catalogue file
 * only grows by the import and the spread.
 */
import {
  defaultParaProps,
  defaultRun,
  newTextDocument,
  type Block,
  type ParaProps,
  type TextDocument,
} from "../lib/office-types";
import type { OfficeTemplate } from "./templates";

function paragraph(text: string, style = "Normal", extra: Partial<ParaProps> = {}): Block {
  return {
    type: "paragraph",
    props: { ...defaultParaProps(style), ...extra },
    runs: [{ ...defaultRun(text) }],
  };
}

function writerTemplate(
  id: string,
  name: string,
  description: string,
  build: (document: TextDocument) => void,
): OfficeTemplate {
  return {
    id,
    kind: "writer",
    name,
    description,
    language: "tr",
    build: () => {
      const document = newTextDocument(name);
      build(document);
      return document;
    },
  };
}

export const TEMPLATES_WITH_FIELDS: OfficeTemplate[] = [
  writerTemplate(
    "dilekce-alanli",
    "Dilekçe (alanlı)",
    "Kurum, konu ve kişi alanlarını doldurarak hazır dilekçe.",
    (document) => {
      document.blocks = [
        paragraph("T.C.", "Title"),
        paragraph("{{kurum}}", "Subtitle"),
        paragraph("{{tarih}}"),
        paragraph("SAYIN {{makam}}"),
        paragraph("Konu: {{konu}}", "Heading2"),
        paragraph(
          "{{ad_soyad}} tarafından sunulan dilekçemde, yukarıdaki konuya ilişkin talebimin değerlendirilmesini arz ederim.",
        ),
        paragraph("Adres: {{adres}}"),
        paragraph("Saygılarımla,"),
        paragraph("{{ad_soyad}}", "Caption", { align: "right", spaceBeforePt: 24 }),
      ];
    },
  ),
];
