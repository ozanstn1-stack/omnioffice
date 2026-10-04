import { PDFDocument, StandardFonts, rgb } from "pdf-lib";

/**
 * Builds a small sample document in memory.
 *
 * Used by the `?demo=1` development hook so the browser build can be verified
 * end to end in automation (the same way the desktop app uses PDFSAK_DEV_*).
 * No personal data is involved.
 */
export async function makeDemoPdf(pages = 5, label = "Demo document"): Promise<File> {
  const doc = await PDFDocument.create();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  const bold = await doc.embedFont(StandardFonts.HelveticaBold);
  for (let index = 1; index <= pages; index += 1) {
    const page = doc.addPage([595.28, 841.89]);
    page.drawRectangle({ x: 40, y: 40, width: 515, height: 760, color: rgb(0.94, 0.94, 0.97) });
    page.drawText(`${label} - page ${index}`, { x: 72, y: 700, size: 26, font: bold, color: rgb(0.1, 0.1, 0.2) });
    page.drawText("Generated in the browser for testing. Nothing is uploaded.", { x: 72, y: 660, size: 12, font, color: rgb(0.3, 0.3, 0.35) });
    page.drawRectangle({ x: 80, y: 480, width: 300, height: 120, borderColor: rgb(0.35, 0.4, 0.85), borderWidth: 3 });
    if (index % 2 === 0) {
      page.drawText("invoices 2026 report sample", { x: 90, y: 520, size: 14, font, color: rgb(0.2, 0.2, 0.25) });
    }
  }
  doc.setTitle("Demo document");
  doc.setAuthor("OmniOffice (extension demo)");
  const bytes = await doc.save({ useObjectStreams: false });
  return new File([bytes as BlobPart], "demo-document.pdf", { type: "application/pdf" });
}
