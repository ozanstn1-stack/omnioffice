/**
 * File-type-aware tool recommendations for the Home screen.
 *
 * A dropped or picked file should suggest the tools that actually fit it: a PDF
 * suggests the PDF tools, a DOCX suggests Writer, a spreadsheet suggests Calc,
 * an image suggests "images to PDF" and OCR. The list is data-driven and pure,
 * so it is easy to keep honest and to test.
 */
import type { ScreenId } from "./nav";

/** One extension family and the tools it suggests, in priority order. */
const RULES: { matches: (extension: string) => boolean; tools: ScreenId[] }[] = [
  {
    matches: (extension) => extension === "pdf",
    tools: ["reader", "merge", "split", "compress", "ocr", "watermark", "protect", "organize", "redact", "annotate"],
  },
  {
    // Writer formats, including the UYAP `.udf` the app opens.
    matches: (extension) =>
      ["docx", "docm", "dotx", "odt", "udf", "rtf", "txt", "md", "markdown", "html", "htm"].includes(extension),
    tools: ["documents", "converter", "cleaner", "annotate", "templates"],
  },
  {
    matches: (extension) => ["xlsx", "xlsm", "xls", "ods", "csv", "tsv"].includes(extension),
    tools: ["spreadsheets", "converter", "cleaner"],
  },
  {
    matches: (extension) => ["pptx", "pptm", "odp"].includes(extension),
    tools: ["presentations", "converter", "cleaner"],
  },
  {
    matches: (extension) => ["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff"].includes(extension),
    tools: ["imagesToPdf", "ocr", "reader"],
  },
];

/** Lowercase extension without the dot, or "". */
export function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/**
 * The ordered, de-duplicated tools that fit the given files. A mixed selection
 * keeps the first file's family first. An unknown extension contributes
 * nothing; an empty result means "no recommendation".
 */
export function recommendTools(paths: string[], limit = 8): ScreenId[] {
  const out: ScreenId[] = [];
  for (const path of paths) {
    const extension = extensionOf(path);
    const rule = RULES.find((entry) => entry.matches(extension));
    if (!rule) continue;
    for (const tool of rule.tools) {
      if (!out.includes(tool)) out.push(tool);
    }
  }
  return out.slice(0, limit);
}
