/**
 * The page photos the scanner flow uses, generated at run time so no binary is
 * checked in.
 *
 * The geometry matches the Rust unit tests in `crates/pdfcore`
 * (`scan_detects_the_page_corners` and `scan_reports_no_page_on_a_blank_wall`):
 * a light quadrilateral on a dark desk for the page the detector must find, and
 * a flat grey wall for the one it must not.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { deflateSync } from "node:zlib";

import { work } from "./driver.mjs";

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let index = 0; index < 256; index += 1) {
    let value = index;
    for (let bit = 0; bit < 8; bit += 1) {
      value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    table[index] = value >>> 0;
  }
  return table;
})();

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([length, body, crc]);
}

/** An 8-bit RGB PNG built from a per-pixel colour function. */
function writePng(path, width, height, pixelAt) {
  const raw = Buffer.alloc(height * (1 + width * 3));
  let offset = 0;
  for (let y = 0; y < height; y += 1) {
    raw[offset] = 0; // filter: none
    offset += 1;
    for (let x = 0; x < width; x += 1) {
      const [r, g, b] = pixelAt(x, y);
      raw[offset] = r;
      raw[offset + 1] = g;
      raw[offset + 2] = b;
      offset += 3;
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bit depth
  header[9] = 2; // colour type: truecolour
  writeFileSync(
    path,
    Buffer.concat([
      Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      chunk("IHDR", header),
      chunk("IDAT", deflateSync(raw)),
      chunk("IEND", Buffer.alloc(0)),
    ]),
  );
}

/** Ray casting, so the quad may be any convex or concave shape. */
function inside(x, y, corners) {
  let hit = false;
  for (let index = 0, previous = corners.length - 1; index < corners.length; previous = index, index += 1) {
    const [xi, yi] = corners[index];
    const [xp, yp] = corners[previous];
    if (yi > y !== yp > y && x < ((xp - xi) * (y - yi)) / (yp - yi) + xi) hit = !hit;
  }
  return hit;
}

/**
 * Writes the two photos into the work directory and returns their paths.
 * The names are fixed, so a flow can recompute the PDF path the app derives.
 */
export function prepareScanPages() {
  mkdirSync(work, { recursive: true });
  const photo = join(work, "scan-photo.png");
  const blank = join(work, "scan-blank.png");

  const corners = [
    [46, 30],
    [278, 38],
    [270, 208],
    [40, 200],
  ];
  writePng(photo, 320, 240, (x, y) => (inside(x, y, corners) ? [246, 243, 236] : [64, 66, 72]));
  writePng(blank, 320, 240, () => [120, 120, 120]);
  return { photo, blank };
}
