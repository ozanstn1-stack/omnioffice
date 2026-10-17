import { describe, expect, it } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DocumentHealthScreen } from "./DocumentHealthScreen";

describe("DocumentHealthScreen (v5.4.0)", () => {
  it("renders health screen with high score on clean text", () => {
    render(<DocumentHealthScreen initialText="Temiz metin içeriği" />);
    expect(screen.getByText("Belge Sağlığı ve KVKK Denetimi")).toBeInTheDocument();
    expect(screen.getByText("Belge Sağlık Skoru")).toBeInTheDocument();
    expect(screen.getByText("Harika! Belgenizde herhangi bir risk bulunamadı.")).toBeInTheDocument();
  });

  it("detects personal data and allows 1-click fix interaction", () => {
    const dirtyText = "Ad: Mehmet, TCKN: 10000000146, IBAN: TR33 0006 1005 1978 6457 8412 01";
    render(
      <DocumentHealthScreen
        initialText={dirtyText}
        initialMetadata={{ author: "Mehmet Yılmaz", creator: "Word" }}
      />
    );

    expect(screen.getByText("T.C. Kimlik Numarası (TCKN) Tespit Edildi")).toBeInTheDocument();
    expect(screen.getByText("IBAN Numarası Tespit Edildi")).toBeInTheDocument();
    expect(screen.getByText("Gizli Belge Meta Verileri")).toBeInTheDocument();

    const fixButtons = screen.getAllByText("Tek Tıkla Düzelt");
    expect(fixButtons.length).toBeGreaterThan(0);

    // Click the metadata fix button
    fireEvent.click(fixButtons[0]);
  });
});
