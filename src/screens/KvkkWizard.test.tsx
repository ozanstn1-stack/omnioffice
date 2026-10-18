import { describe, expect, it } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { KvkkWizard } from "./KvkkWizard";

describe("KvkkWizard Screen (v5.4.0 - Issue #254)", () => {
  it("renders KVKK wizard and switches template types", () => {
    render(<KvkkWizard />);
    expect(screen.getByText("KVKK Aydınlatma ve Açık Rıza Sihirbazı")).toBeInTheDocument();
    expect(screen.getByText("Çalışan")).toBeInTheDocument();
    expect(screen.getByText("Müşteri / Web")).toBeInTheDocument();
    expect(screen.getByText("DOCX İndir")).toBeInTheDocument();
    expect(screen.getByText("PDF İndir")).toBeInTheDocument();

    // Click on Müşteri template button
    fireEvent.click(screen.getByText("Müşteri / Web"));
    expect(screen.getByDisplayValue(/MÜŞTERİ VE ÇEVRİMİÇİ ZİYARETÇİ AYDINLATMA METNİ/)).toBeInTheDocument();

    // Fill in company name
    const companyInput = screen.getByPlaceholderText("Örn: Örnek Bilişim Tic. A.Ş.");
    fireEvent.change(companyInput, { target: { value: "Test Bilişim Ltd." } });

    expect(screen.getAllByDisplayValue(/Test Bilişim Ltd\./).length).toBeGreaterThan(0);
  });
});
