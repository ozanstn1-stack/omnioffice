import { describe, expect, it } from "vitest";
import {
  INCOMING_ERROR_REASONS,
  firstIncomingError,
  incomingErrorKey,
  isIncomingErrorReason,
  parseIncomingError,
} from "./incoming";
import { makeTranslate } from "./i18n";

const en = makeTranslate("en");
const tr = makeTranslate("tr");

describe("parseIncomingError", () => {
  it("parses a refused document token into reason and name", () => {
    expect(parseIncomingError("__error:TOO_LARGE:huge.pdf")).toEqual({ reason: "TOO_LARGE", name: "huge.pdf" });
    expect(parseIncomingError("__error:UNKNOWN_TYPE:IMG-0001.jpg")).toEqual({
      reason: "UNKNOWN_TYPE",
      name: "IMG-0001.jpg",
    });
  });

  it("accepts every reason the Android shell can queue", () => {
    for (const reason of INCOMING_ERROR_REASONS) {
      expect(parseIncomingError(`__error:${reason}:report.pdf`), reason).toEqual({ reason, name: "report.pdf" });
    }
  });

  it("returns a null name when the shell could not resolve one", () => {
    expect(parseIncomingError("__error:NO_ACCESS:")).toEqual({ reason: "NO_ACCESS", name: null });
    expect(parseIncomingError("__error:UNKNOWN_TYPE")).toEqual({ reason: "UNKNOWN_TYPE", name: null });
  });

  it("keeps a colon that a legacy name may contain in the name field", () => {
    // Sanitised names never contain ':', but a token queued before the app
    // updated could; the parser must not mistake it for another field.
    expect(parseIncomingError("__error:FAILED:report:final.pdf")).toEqual({
      reason: "FAILED",
      name: "report:final.pdf",
    });
  });

  it("ignores tokens that are not refused documents", () => {
    expect(parseIncomingError("/data/user/0/cache/incoming/report.pdf")).toBeNull();
    expect(parseIncomingError("__action:scan")).toBeNull();
    expect(parseIncomingError("__error:EXPLODED:report.pdf")).toBeNull();
    expect(parseIncomingError("")).toBeNull();
    expect(parseIncomingError("__error:")).toBeNull();
  });

  it("rejects lowercase reason names", () => {
    expect(parseIncomingError("__error:too_large:report.pdf")).toBeNull();
  });
});

describe("firstIncomingError", () => {
  it("finds the first refused document among real paths and actions", () => {
    expect(
      firstIncomingError(["/cache/incoming/a.pdf", "__error:TOO_LARGE:video.mp4", "__error:FAILED:notes.docx"]),
    ).toEqual({ reason: "TOO_LARGE", name: "video.mp4" });
  });

  it("returns null when nothing was refused", () => {
    expect(firstIncomingError([])).toBeNull();
    expect(firstIncomingError(["/cache/incoming/a.pdf", "__action:recent_files"])).toBeNull();
  });
});

describe("isIncomingErrorReason", () => {
  it("narrows only the known reasons", () => {
    expect(isIncomingErrorReason("UNKNOWN_TYPE")).toBe(true);
    expect(isIncomingErrorReason("NO_ACCESS")).toBe(true);
    expect(isIncomingErrorReason("EXPLODED")).toBe(false);
  });
});

describe("incomingErrorKey", () => {
  it("maps every reason to a dictionary key that exists in both languages", () => {
    for (const reason of INCOMING_ERROR_REASONS) {
      const key = incomingErrorKey(reason);
      expect(en(key), `en ${reason}`).not.toBe(key);
      expect(tr(key), `tr ${reason}`).not.toBe(key);
    }
  });

  it("groups the reasons into four user-facing messages", () => {
    expect(incomingErrorKey("TOO_LARGE")).toBe("errors.incomingTooLarge");
    expect(incomingErrorKey("NO_STREAM")).toBe("errors.incomingUnreadable");
    expect(incomingErrorKey("NO_ACCESS")).toBe("errors.incomingUnreadable");
    expect(incomingErrorKey("FAILED")).toBe("errors.incomingUnreadable");
    expect(incomingErrorKey("UNKNOWN_TYPE")).toBe("errors.incomingUnknownType");
    expect(incomingErrorKey("NO_NAME")).toBe("errors.unsupported");
    expect(incomingErrorKey("UNSUPPORTED")).toBe("errors.unsupported");
  });
});
