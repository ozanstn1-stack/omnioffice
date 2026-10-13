import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "recipe_list":
      return [];
    case "watched_folder_get":
      return { folder: "", outputDir: "", recipeId: "" };
    case "recipe_save":
    case "watched_folder_set":
    case "watched_folder_start":
    case "watched_folder_stop":
      return null;
    case "recipe_run":
      return { output: "C:/out.pdf", applied: ["compress"] };
    case "watched_folder_scan":
      return [];
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => "C:/docs/in.pdf"),
  save: vi.fn(async () => "C:/docs/out.pdf"),
}));

import { Recipes } from "./Recipes";

describe("batch recipes screen", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("builds and saves a recipe from the step palette", async () => {
    const user = userEvent.setup();
    render(<Recipes />);
    await screen.findByText(/Build a recipe/i);
    await user.type(screen.getByPlaceholderText(/Arşiv/i), "Arşiv");
    await user.click(screen.getByRole("button", { name: /compress/i }));
    await user.click(screen.getByRole("button", { name: /^stamp$/i }));
    await user.click(screen.getByRole("button", { name: /Save recipe/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "recipe_save",
        expect.objectContaining({
          recipe: expect.objectContaining({
            id: "Arşiv",
            steps: [expect.objectContaining({ kind: "compress" }), expect.objectContaining({ kind: "stamp" })],
          }),
        }),
      ),
    );
  });
});
