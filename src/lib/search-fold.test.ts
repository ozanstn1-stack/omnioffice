import { describe, expect, it } from "vitest";
import { foldSearch, searchIncludes } from "./search-fold";

describe("Turkish search folding", () => {
  it("folds the four i/I/ı/İ spellings together", () => {
    expect(foldSearch("Şişli")).toBe("sisli");
    expect(foldSearch("SİSLİ")).toBe("sisli");
    expect(foldSearch("sısli")).toBe("sisli");
    expect(foldSearch("sisli")).toBe("sisli");
    for (const spelling of ["Şişli", "SİSLİ", "sısli", "sisli", "ŞİŞLİ"]) {
      expect(searchIncludes(spelling, "sisli")).toBe(true);
      expect(searchIncludes("Şişli", spelling)).toBe(true);
    }
  });

  it("folds the remaining Turkish letters to their ASCII base", () => {
    expect(foldSearch("Güneş ĞDIR ç ö ü")).toBe("gunes gdir c o u");
    expect(searchIncludes("Güneş", "gunes")).toBe(true);
    expect(searchIncludes("Çözüm", "cozum")).toBe(true);
    expect(searchIncludes("ÖĞÜN", "ogun")).toBe(true);
  });

  it("keeps the mapping one-to-one so offsets survive", () => {
    const input = "Şişli Güneş İĞDIR ı";
    expect(foldSearch(input)).toHaveLength(input.length);
  });

  it("does not match unrelated text", () => {
    expect(searchIncludes("Birleştir", "sil")).toBe(false);
    expect(searchIncludes("Birleştir", "birlestir")).toBe(true);
  });
});
