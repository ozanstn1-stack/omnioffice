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
  it("prefers the configured name, then the document author, then the default", () => {
    // The user is the one making the change, so their name wins over the
    // document's own author.
    expect(revisionAuthor("Ada", "Ayşe")).toBe("Ayşe");
    expect(revisionAuthor("Ada", "  ")).toBe("Ada");
    expect(revisionAuthor("", "")).toBe("You");
  });

  it("trims the name it uses", () => {
    expect(revisionAuthor("Ada", "  Ayşe  ")).toBe("Ayşe");
    expect(revisionAuthor("  Ada  ", "")).toBe("Ada");
  });
});
