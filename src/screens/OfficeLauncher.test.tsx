import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { OfficeLauncher } from "./OfficeLauncher";
import { useRecent } from "../lib/store";
import { useOfficeTabs } from "../lib/office-store";

describe("office launchers (7.32.1)", () => {
  it("lists only this kind's recent files, without kind chips, and starts a blank one", async () => {
    const user = userEvent.setup();
    useRecent.setState({
      entries: [
        { token: "a", fileName: "Bütçe.xlsx", tool: "_office", timestamp: 1_700_000_000 },
        { token: "b", fileName: "Kira.docx", tool: "_office", timestamp: 1_700_000_100 },
        { token: "c", fileName: "Envanter.ods", tool: "_office", timestamp: 1_700_000_200 },
      ],
      refresh: async () => undefined,
    });
    useOfficeTabs.setState({ tabs: [], activeId: null });
    const onOpen = vi.fn();
    const onTemplates = vi.fn();
    render(<OfficeLauncher kind="calc" onOpen={onOpen} onTemplates={onTemplates} />);

    expect(screen.getByRole("heading", { name: "Spreadsheets" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Bütçe\.xlsx/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Envanter\.ods/ })).toBeInTheDocument();
    expect(screen.queryByText("Kira.docx")).toBeNull();
    expect(screen.queryByRole("group", { name: "Filter by kind" })).toBeNull();

    await user.click(screen.getByRole("button", { name: /Blank spreadsheet/ }));
    expect(useOfficeTabs.getState().tabs.map((tab) => tab.kind)).toEqual(["calc"]);
    expect(onOpen).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: /From a template/ }));
    expect(onTemplates).toHaveBeenCalled();
  });
});
