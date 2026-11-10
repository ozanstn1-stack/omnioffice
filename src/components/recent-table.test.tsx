import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RecentTable, recentKind, relativeTime } from "./recent-table";
import type { RecentEntry } from "../lib/types";

const NOW = Date.UTC(2026, 10, 10, 12, 0, 0);
const at = (secondsAgo: number) => Math.floor(NOW / 1000) - secondsAgo;

const entries: RecentEntry[] = [
  { token: "t1", fileName: "Kira sözleşmesi.docx", tool: "_office", timestamp: at(12 * 60) },
  { token: "t2", fileName: "Bütçe.xlsx", tool: "_office", timestamp: at(3 * 3600) },
  { token: "t3", fileName: "Tapu.pdf", tool: "merge", timestamp: at(26 * 3600) },
  { token: "t4", fileName: "Sunum.pptx", tool: "_office", timestamp: at(30 * 86_400) },
];

describe("recent files table (7.32)", () => {
  it("sorts each file into the kind it is filtered and coloured by", () => {
    expect(recentKind("a.docx")).toBe("doc");
    expect(recentKind("a.ODS")).toBe("sheet");
    expect(recentKind("a.pptx")).toBe("slides");
    expect(recentKind("a.pdf")).toBe("pdf");
    expect(recentKind("a.png")).toBe("other");
  });

  it("filters by kind and opens or shares the chosen row", async () => {
    const user = userEvent.setup();
    const open = vi.fn();
    const share = vi.fn();
    render(<RecentTable entries={entries} language="en" onOpen={open} onShare={share} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(4);
    await user.click(screen.getByRole("button", { name: "PDF" }));
    expect(screen.getByRole("button", { name: "PDF" })).toHaveAttribute("aria-pressed", "true");
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(1);
    // The tool the file was last used with, as the sidebar names it.
    expect(within(rows[0]).getByText("Merge")).toBeInTheDocument();

    await user.click(within(rows[0]).getByRole("button", { name: /Tapu\.pdf/ }));
    expect(open).toHaveBeenCalledWith(entries[2]);
    await user.click(within(rows[0]).getByRole("button", { name: "Open folder" }));
    expect(share).toHaveBeenCalledWith(entries[2]);
  });

  it("says when a kind has no recent files and when there are none at all", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <RecentTable entries={entries.slice(0, 1)} language="en" onOpen={vi.fn()} onShare={vi.fn()} />,
    );
    await user.click(screen.getByRole("button", { name: "Spreadsheets" }));
    expect(screen.getByText("No recent files of this kind")).toBeInTheDocument();
    rerender(<RecentTable entries={[]} language="en" onOpen={vi.fn()} onShare={vi.fn()} />);
    expect(screen.getByText("No documents yet")).toBeInTheDocument();
  });

  it("dates rows relative to now in the interface language", () => {
    expect(relativeTime(at(12 * 60), "en", NOW)).toBe("12 min. ago");
    expect(relativeTime(at(3 * 3600), "tr", NOW)).toBe("3 sa. önce");
    expect(relativeTime(at(26 * 3600), "en", NOW)).toBe("yesterday");
    expect(relativeTime(at(30 * 86_400), "en", NOW)).toBe("Oct 11");
  });
});

describe("jobs glance on Home", () => {
  it("shows the first running job with its progress and opens Jobs", async () => {
    const { JobsGlance } = await import("./jobs-glance");
    const { useJobs } = await import("../lib/jobs");
    const user = userEvent.setup();
    const navigate = vi.fn();
    const { container } = render(<JobsGlance onNavigate={navigate} />);
    expect(container).toBeEmptyDOMElement();

    useJobs.setState({
      jobs: [
        {
          id: "j1",
          kind: "ocr",
          title: "OCR · Tapu.pdf",
          status: "running",
          stage: "",
          current: 16,
          total: 25,
          startedAt: 0,
        },
        { id: "j2", kind: "merge", title: "Merge", status: "running", stage: "", current: 0, total: 0, startedAt: 0 },
      ],
    });
    const card = await screen.findByRole("button", { name: /OCR · Tapu\.pdf/ });
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "64");
    expect(card).toHaveTextContent("2 background jobs running");
    await user.click(card);
    expect(navigate).toHaveBeenCalledWith("jobs");
    useJobs.setState({ jobs: [] });
  });
});
