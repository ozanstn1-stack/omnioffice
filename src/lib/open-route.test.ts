import { describe, expect, it } from "vitest";
import { routeForPath } from "./open-route";

describe("document open routing", () => {
  it("sends office documents to the workspace", () => {
    expect(routeForPath("C:/docs/report.docx")).toMatchObject({ screen: "office", office: true });
    expect(routeForPath("C:/docs/book.xlsx")).toMatchObject({ screen: "office", office: true });
    expect(routeForPath("C:/docs/talk.pptx")).toMatchObject({ screen: "office", office: true });
    expect(routeForPath("C:/docs/notes.oswk")).toMatchObject({ screen: "office", office: true });
  });

  it("sends PDFs to the reader with the file attached", () => {
    expect(routeForPath("C:/docs/a.PDF")).toEqual({ screen: "reader", files: ["C:/docs/a.PDF"], office: false });
  });

  it("sends images to the image-to-PDF tool", () => {
    expect(routeForPath("C:/docs/scan.png")).toEqual({
      screen: "imagesToPdf",
      files: ["C:/docs/scan.png"],
      office: false,
    });
  });

  it("returns null for unknown types so the system viewer can open them", () => {
    expect(routeForPath("C:/docs/archive.zip")).toBeNull();
    expect(routeForPath("")).toBeNull();
  });
});
