import { describe, expect, it, vi } from "vitest";

import { openTokenDocument } from "./file-refs";

function handlers() {
  return {
    showClosedUdf: vi.fn(),
    openOffice: vi.fn(),
    openScreen: vi.fn(),
    openExternal: vi.fn(),
  };
}

describe("token document routing", () => {
  it("opens a PDF in the reader with the token, not the name", () => {
    const actions = handlers();
    openTokenDocument({ token: "tok-pdf", name: "report.pdf" }, actions);
    expect(actions.openScreen).toHaveBeenCalledWith("reader", ["tok-pdf"]);
    expect(actions.openOffice).not.toHaveBeenCalled();
  });

  it("opens office documents in the workspace by their display name", () => {
    const actions = handlers();
    openTokenDocument({ token: "tok-doc", name: "report.docx" }, actions);
    expect(actions.openOffice).toHaveBeenCalledWith("tok-doc");
  });

  it("hands unknown types to the system viewer", () => {
    const actions = handlers();
    openTokenDocument({ token: "tok-bin", name: "archive.rar" }, actions);
    expect(actions.openExternal).toHaveBeenCalledWith("tok-bin");
  });
});
