import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { NavTree, NewMenu, buildNavHubs } from "./sidebar";
import { CreateRow, greetingKey, todayLabel } from "./create-row";
import { useOfficeTabs } from "../lib/office-store";
import type { ScreenId } from "../lib/nav";

const icons = new Proxy({}, { get: () => null }) as Record<ScreenId, null>;
const hubs = () => buildNavHubs((key) => key, icons);

describe("hub navigation (7.31)", () => {
  it("folds the routes into a few hubs, with scan, images and OCR together", () => {
    const list = hubs();
    expect(list.map((hub) => hub.id)).toEqual([
      "top",
      "office",
      "productivity",
      "convert",
      "pdfTools",
      "security",
      "ai",
      "system",
    ]);
    const convert = list.find((hub) => hub.id === "convert")!.items.map((item) => item.id);
    expect(convert).toEqual(expect.arrayContaining(["scanner", "pdfToImages", "imagesToPdf", "ocr"]));
    expect(list[0].label).toBeUndefined();
  });

  it("opens the hub of the current screen and keeps closed items findable", async () => {
    const user = userEvent.setup();
    const pick = vi.fn();
    render(<NavTree hubs={hubs()} screen="merge" onPick={pick} showLabels />);

    const pdf = screen.getByRole("button", { name: "nav.pdfTools" });
    expect(pdf).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "nav.merge" })).toHaveAttribute("aria-current", "page");

    const security = screen.getByRole("button", { name: "nav.security" });
    expect(security).toHaveAttribute("aria-expanded", "false");
    // Hidden, not removed: the e2e flows and shortcuts still reach it.
    expect(screen.getByText("nav.protect", { selector: "span" }).closest("[hidden]")).not.toBeNull();

    await user.click(security);
    expect(security).toHaveAttribute("aria-expanded", "true");
    await user.click(screen.getByRole("button", { name: "nav.protect" }));
    expect(pick).toHaveBeenCalledWith("protect");
  });

  it("shows one flat icon column when compact", () => {
    render(<NavTree hubs={hubs()} screen="home" onPick={vi.fn()} showLabels={false} />);
    expect(screen.queryByRole("button", { name: "nav.pdfTools" })).toBeNull();
    expect(screen.getByTitle("nav.merge")).toBeInTheDocument();
  });

  it("creates a blank spreadsheet from the New menu", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    useOfficeTabs.setState({ tabs: [], activeId: null });
    render(<NewMenu onNavigate={navigate} />);
    await user.click(screen.getByRole("button", { name: /New/ }));
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: /Blank spreadsheet/ }));
    expect(useOfficeTabs.getState().tabs.map((tab) => tab.kind)).toEqual(["calc"]);
    expect(navigate).toHaveBeenCalledWith("office");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("Home create row", () => {
  it("opens a blank document in one click and the gallery for a template", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    useOfficeTabs.setState({ tabs: [], activeId: null });
    render(<CreateRow onNavigate={navigate} />);
    await user.click(screen.getByRole("button", { name: /Blank document/ }));
    expect(useOfficeTabs.getState().tabs.map((tab) => tab.kind)).toEqual(["writer"]);
    expect(navigate).toHaveBeenLastCalledWith("office");
    await user.click(screen.getByRole("button", { name: /From a template/ }));
    expect(navigate).toHaveBeenLastCalledWith("templates");
  });

  it("greets by the hour and dates in the interface language", () => {
    expect(greetingKey(8)).toBe("home.greetMorning");
    expect(greetingKey(14)).toBe("home.greetDay");
    expect(greetingKey(22)).toBe("home.greetEvening");
    expect(greetingKey(3)).toBe("home.greetEvening");
    expect(todayLabel("tr", new Date(2026, 10, 10))).toBe("10 Kasım Salı");
  });
});
