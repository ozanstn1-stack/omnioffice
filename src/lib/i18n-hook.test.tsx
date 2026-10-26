import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

// A Turkish dictionary that is not loaded yet, the way a phone starts: English
// is awaited before the first render, Turkish is fetched once the settings say
// "tr". The fetch resolves when the test says so.
const dictionaries: Record<string, Record<string, string>> = { en: { "nav.home": "Home" } };
let resolveTurkish: () => void = () => undefined;

vi.mock("./i18n-runtime", () => ({
  loadedDictionary: (language: string) => dictionaries[language] ?? null,
  ensureLanguage: (language: string) =>
    dictionaries[language]
      ? Promise.resolve()
      : new Promise<void>((resolve) => {
          resolveTurkish = () => {
            dictionaries[language] = { "nav.home": "Ana sayfa" };
            resolve();
          };
        }),
}));

import { useT } from "./i18n";
import { useSettings } from "./store";
import { DEFAULT_SETTINGS } from "./types";

function Label() {
  const t = useT();
  return <span>{t("nav.home")}</span>;
}

describe("useT", () => {
  it("switches a mounted component to a language that finishes loading later", async () => {
    // Mounted while the settings still say English (App and the drawer do this).
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" } });
    render(<Label />);
    expect(screen.getByText("Home")).toBeInTheDocument();

    // The settings load and pick Turkish; its dictionary is still in flight.
    act(() => useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "tr" } }));
    await act(async () => resolveTurkish());

    expect(screen.getByText("Ana sayfa")).toBeInTheDocument();
  });
});
