import { describe, expect, it } from "vitest";
import { clamp, dirName, fileBaseName, fileStem, joinPath } from "./format";
import { rememberTokenName } from "./file-token-names";

describe("path formatting helpers (E3.6)", () => {
  it("extracts base names across POSIX, Windows, bare names, and registered tokens", () => {
    expect(fileBaseName("/home/user/docs/report.pdf")).toBe("report.pdf");
    expect(fileBaseName("C:\\Users\\ozan\\Desktop\\dilekce.docx")).toBe("dilekce.docx");
    expect(fileBaseName("plain.xlsx")).toBe("plain.xlsx");

    const token = "saf://content/12345";
    rememberTokenName(token, "mobil-belge.pdf");
    expect(fileBaseName(token)).toBe("mobil-belge.pdf");
  });

  it("computes stems, directory names, joined paths, and numeric clamping consistently", () => {
    expect(fileStem("/tmp/archive.tar.gz")).toBe("archive.tar");
    expect(dirName("C:\\work\\test.pdf")).toBe("C:\\work");
    expect(joinPath("/tmp", "out.pdf")).toBe("/tmp/out.pdf");
    expect(joinPath("C:\\work\\", "out.pdf")).toBe("C:\\work\\out.pdf");
    expect(clamp(-5, 0, 10)).toBe(0);
    expect(clamp(4, 0, 10)).toBe(4);
    expect(clamp(25, 0, 10)).toBe(10);
  });
});
