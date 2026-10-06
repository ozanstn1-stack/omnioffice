/**
 * Wraps one JPEG image into a single-page PDF, entirely in the frontend.
 *
 * Used by the Draw export: a JPEG can be embedded as-is (`/DCTDecode`), so no
 * decoding or temporary file is needed and the result is written in one go to
 * the destination the user picked (on Android that is a content:// URI, on
 * the desktop the only path the fs scope allows).
 */
export function jpegToPdf(
  jpeg: Uint8Array,
  pixelWidth: number,
  pixelHeight: number,
  pageWidthPt: number,
  pageHeightPt: number,
): Uint8Array {
  const encoder = new TextEncoder();
  const chunks: Uint8Array[] = [];
  const offsets: number[] = [];
  let length = 0;
  const push = (chunk: Uint8Array | string) => {
    const bytes = typeof chunk === "string" ? encoder.encode(chunk) : chunk;
    chunks.push(bytes);
    length += bytes.length;
  };
  const object = (body: () => void) => {
    offsets.push(length);
    push(`${offsets.length} 0 obj\n`);
    body();
    push("\nendobj\n");
  };
  const width = round(pageWidthPt);
  const height = round(pageHeightPt);
  const content = `q ${width} 0 0 ${height} 0 0 cm /Im0 Do Q`;

  // Header with a binary comment so transfer tools treat the file as binary.
  push(new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34, 0x0a, 0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a]));
  object(() => push("<< /Type /Catalog /Pages 2 0 R >>"));
  object(() => push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"));
  object(() =>
    push(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${width} ${height}] ` +
        "/Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>",
    ),
  );
  object(() => {
    push(
      `<< /Type /XObject /Subtype /Image /Width ${Math.round(pixelWidth)} /Height ${Math.round(pixelHeight)} ` +
        `/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>\nstream\n`,
    );
    push(jpeg);
    push("\nendstream");
  });
  object(() => push(`<< /Length ${content.length} >>\nstream\n${content}\nendstream`));

  const xref = length;
  push(`xref\n0 ${offsets.length + 1}\n0000000000 65535 f \n`);
  for (const offset of offsets) push(`${String(offset).padStart(10, "0")} 00000 n \n`);
  push(`trailer\n<< /Size ${offsets.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);

  const out = new Uint8Array(length);
  let position = 0;
  for (const chunk of chunks) {
    out.set(chunk, position);
    position += chunk.length;
  }
  return out;
}

function round(value: number): string {
  return String(Math.round(value * 100) / 100);
}
