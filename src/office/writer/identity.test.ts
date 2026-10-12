import { describe, expect, it } from "vitest";
import { commentAuthor, revisionAuthor } from "./identity";

describe("commentAuthor", () => {
  it("keeps the historical default when no name is set", () => {
    expect(commentAuthor("")).toBe("Me");
    expect(commentAuthor("   ")).toBe("Me");
  });

  it("uses the configured name, trimmed", () => {
    expect(commentAuthor("Ayşe Yılmaz")).toBe("Ayşe Yılmaz");
    expect(commentAuthor("  Ayşe Yılmaz  ")).toBe("Ayşe Yılmaz");
  });
});

describe("revisionAuthor", () => {
  it("prefers the document author, then the setting, then the default", () => {
    expect(revisionAuthor("Ada", "Ayşe")).toBe("Ada");
    expect(revisionAuthor("  ", "Ayşe")).toBe("Ayşe");
    expect(revisionAuthor("", "")).toBe("You");
  });

  it("trims the document author", () => {
    expect(revisionAuthor("  Ada  ", "Ayşe")).toBe("Ada");
  });
});
