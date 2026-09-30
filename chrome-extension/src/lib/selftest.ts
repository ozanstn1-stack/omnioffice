// In-browser self test: runs the extension's real operation modules inside
// Chrome (including the pdf.js worker + canvas rendering) and prints a report
// into the DOM. Used by the automated validation:
//
//   chrome --headless=new --dump-dom "http://localhost:4173/app.html?selftest=1"
//
// Everything is local; the demo document is generated in memory.

import { PDFDocument, StandardFonts, rgb } from "pdf-lib";
import {
  addPageNumbers,
  addTextWatermark,
  compressLossless,
  extractPages,
  imagesToPdf,
  loadPdf,
  mergePdfs,
  readInfo,
  rotatePages,
} from "./pdf";
import { extractPageText, renderPage, searchDocument } from "./render";
import { makeDemoPdf } from "./demo";
import { loadStampFonts } from "./session";
import { MAX_REMOTE_BYTES, openRemoteLink, parseRemoteLink } from "./remotelink";
import { takePendingFiles } from "./pending";

interface Check {
  name: string;
  ok: boolean;
  detail: string;
}

async function run(): Promise<Check[]> {
  const checks: Check[] = [];
  const record = async (name: string, action: () => Promise<string>) => {
    try {
      checks.push({ name, ok: true, detail: await action() });
    } catch (error) {
      checks.push({ name, ok: false, detail: error instanceof Error ? error.message : String(error) });
    }
  };

  const demo = await makeDemoPdf(5, "Self test");
  const demoBytes = new Uint8Array(await demo.arrayBuffer());

  await record("pdf-lib loads the generated document", async () => {
    const doc = await loadPdf(demoBytes);
    return `${doc.getPageCount()} pages`;
  });

  await record("pdf.js renders a page to canvas", async () => {
    const rendered = await renderPage(demoBytes, 1, 96);
    const context = rendered.canvas.getContext("2d")!;
    const pixels = context.getImageData(0, 0, rendered.width, rendered.height).data;
    let inkPixels = 0;
    for (let index = 0; index < pixels.length; index += 4 * 97) {
      if (pixels[index] < 240 || pixels[index + 1] < 240 || pixels[index + 2] < 240) inkPixels += 1;
    }
    if (rendered.width < 400 || inkPixels < 10) throw new Error(`blank canvas (${rendered.width}px, ${inkPixels} ink samples)`);
    return `${rendered.width}×${rendered.height}, ${inkPixels} ink samples`;
  });

  await record("pdf.js searches the text layer", async () => {
    const result = await searchDocument(demoBytes, "page", 50);
    if (result.totalMatches < 5) throw new Error(`only ${result.totalMatches} matches`);
    return `${result.totalMatches} matches on ${result.pagesWithMatches} pages`;
  });

  await record("text extraction works", async () => {
    const text = await extractPageText(demoBytes, 2);
    if (!/page 2/i.test(text)) throw new Error(`unexpected text: ${text.slice(0, 60)}`);
    return text.slice(0, 40);
  });

  await record("merge combines documents", async () => {
    const other = await makeDemoPdf(2, "Second");
    const merged = await mergePdfs([
      { name: "a.pdf", bytes: demoBytes },
      { name: "b.pdf", bytes: new Uint8Array(await other.arrayBuffer()) },
    ]);
    const info = await readInfo("merged.pdf", merged);
    if (info.pageCount !== 7) throw new Error(`expected 7 pages, got ${info.pageCount}`);
    return `${info.pageCount} pages`;
  });

  await record("rotation and extraction", async () => {
    const rotated = await rotatePages(demoBytes, [1, 2], 90);
    const doc = await loadPdf(rotated);
    if (doc.getPage(0).getRotation().angle !== 90) throw new Error("page 1 was not rotated");
    const extracted = await extractPages(demoBytes, [1, 3, 5]);
    const info = await readInfo("extracted.pdf", extracted);
    if (info.pageCount !== 3) throw new Error(`expected 3 pages, got ${info.pageCount}`);
    return "rotated 2 pages, extracted 3";
  });

  await record("page numbering writes text", async () => {
    const numbered = await addPageNumbers(demoBytes, {
      position: "bottom_center",
      format: "page_n_of_total",
      startNumber: 1,
      sizePt: 11,
      color: "#333333",
      marginPt: 28,
      countFromStart: true,
      pages: [],
    });
    const text = await extractPageText(numbered, 1);
    if (!/Page 1 of 5/.test(text)) throw new Error(`missing label: ${text.slice(0, 60)}`);
    return "label present";
  });

  await record("watermark embeds the bundled Unicode font", async () => {
    const fonts = await loadStampFonts();
    const watermarked = await addTextWatermark(demoBytes, {
      text: "GİZLİ - CONFIDENTIAL",
      fontFamily: "sans-bold",
      fontBytes: fonts.sansBold,
      sizePt: 42,
      color: "#9aa0a6",
      opacity: 0.3,
      rotationDeg: 45,
      position: "center",
      marginPt: 24,
      tile: false,
      pages: [],
    });
    const text = await extractPageText(watermarked, 3);
    if (!/GİZLİ/.test(text)) throw new Error(`watermark text missing: ${text.slice(0, 60)}`);
    return "watermark text is searchable";
  });

  await record("lossless compression keeps content", async () => {
    const compressed = await compressLossless(demoBytes);
    const info = await readInfo("compressed.pdf", compressed);
    if (info.pageCount !== 5) throw new Error(`expected 5 pages, got ${info.pageCount}`);
    return `${demoBytes.length} -> ${compressed.length} bytes`;
  });

  await record("images to PDF via canvas", async () => {
    const canvas = document.createElement("canvas");
    canvas.width = 240;
    canvas.height = 160;
    const context = canvas.getContext("2d")!;
    context.fillStyle = "#2b6cb0";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "#ffffff";
    context.fillRect(20, 20, 120, 60);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) throw new Error("canvas encoding failed");
    const png = new Uint8Array(await blob.arrayBuffer());
    const album = await imagesToPdf(
      [
        { name: "a.png", bytes: png, type: "image/png", rotation: 0 },
        { name: "b.png", bytes: png, type: "image/png", rotation: 90 },
      ],
      { pageSize: "a4", customWidthPt: 595, customHeightPt: 842, orientation: "auto", fit: "fit", marginPt: 10, assumedDpi: 96 },
    );
    const info = await readInfo("album.pdf", album);
    if (info.pageCount !== 2) throw new Error(`expected 2 pages, got ${info.pageCount}`);
    return `${info.pageCount} pages from canvas PNGs`;
  });

  await record("encrypted input is refused", async () => {
    const doc = await PDFDocument.create();
    const font = await doc.embedFont(StandardFonts.Helvetica);
    const page = doc.addPage([300, 300]);
    page.drawText("secret", { x: 20, y: 200, size: 20, font, color: rgb(0, 0, 0) });
    const bytes = await doc.save({ useObjectStreams: false });
    const text = new TextDecoder("latin1").decode(bytes);
    const patched = new TextEncoder().encode(text.replace("trailer", "/Encrypt 1 0 R\ntrailer"));
    try {
      await loadPdf(patched);
      throw new Error("encrypted file was accepted");
    } catch (error) {
      const code = (error as { code?: string }).code;
      if (code !== "password_required") throw error;
      return "rejected with password_required";
    }
  });

  await record("context-menu deep links are parsed safely", async () => {
    const good = parseRemoteLink(`#url=${encodeURIComponent("https://example.com/docs/report.pdf")}`);
    if (!good || good.host !== "example.com") throw new Error("valid link was not parsed");
    if (parseRemoteLink("#url=javascript:alert(1)") !== null) throw new Error("javascript: URL accepted");
    if (parseRemoteLink("#url=data:text/html,<script>") !== null) throw new Error("data: URL accepted");
    if (parseRemoteLink("#url=file:///etc/passwd") !== null) throw new Error("file: URL accepted");
    if (parseRemoteLink("") !== null) throw new Error("empty hash accepted");
    return good.host;
  });

  await record("linked PDF downloads into the pending file registry", async () => {
    const bytes = new TextEncoder().encode("%PDF-1.4 fake");
    const fakeFetch = async () =>
      new Response(bytes, { status: 200, headers: { "content-length": String(bytes.length) } });
    const file = await openRemoteLink(
      { url: "https://example.com/docs/report.pdf", host: "example.com" },
      { fetch: fakeFetch as unknown as typeof fetch },
    );
    const pending = takePendingFiles();
    if (!pending || pending.length !== 1) throw new Error("no pending file was registered");
    if (pending[0].name !== "report.pdf") throw new Error(`unexpected name ${pending[0].name}`);
    if (pending[0].type !== "application/pdf") throw new Error("unexpected MIME type");
    return `${file.name} (${file.size} bytes)`;
  });

  await record("oversized linked PDFs are refused", async () => {
    const fakeFetch = async () =>
      new Response(new Uint8Array([1]), {
        status: 200,
        headers: { "content-length": String(MAX_REMOTE_BYTES + 1) },
      });
    try {
      await openRemoteLink(
        { url: "https://example.com/big.pdf", host: "example.com" },
        { fetch: fakeFetch as unknown as typeof fetch },
      );
      throw new Error("oversized download was accepted");
    } catch (error) {
      if (error instanceof Error && error.message.includes("accepted")) throw error;
      return "refused";
    }
  });

  return checks;
}

/** Runs the self test and renders the report into the page. */
export async function runSelfTest(): Promise<void> {
  const output = document.createElement("pre");
  output.id = "selftest";
  output.style.cssText = "padding:16px;font:12px/1.6 Cascadia Mono,Consolas,monospace;color:#e6e6e6;background:#111;white-space:pre-wrap";
  output.textContent = "PDF Swiss Army Knife — browser self test\n\n";
  document.body.innerHTML = "";
  document.body.appendChild(output);

  const checks = await run();
  const passed = checks.filter((check) => check.ok).length;
  output.textContent += checks
    .map((check) => `${check.ok ? "[PASS]" : "[FAIL]"} ${check.name} — ${check.detail}`)
    .join("\n");
  output.textContent += `\n\n${passed}/${checks.length} checks passed\nSELFTEST_RESULT=${passed === checks.length ? "OK" : "FAILED"}`;
  document.title = `selftest ${passed}/${checks.length}`;
}
