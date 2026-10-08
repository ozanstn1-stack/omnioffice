import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { describeCrash, ErrorBoundary } from "./ErrorBoundary";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

/** Throws while `shouldThrow.current` is set, like a screen with a render bug. */
const shouldThrow = { current: true };

function Bomb({ message = "boom" }: { message?: string }) {
  if (shouldThrow.current) throw new Error(message);
  return <p>recovered content</p>;
}

function logCalls() {
  return invokeMock.mock.calls.filter(([command]) => command === "log_frontend").map(([, args]) => args);
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    shouldThrow.current = true;
    invokeMock.mockResolvedValue(undefined);
    useSettings.setState({ settings: DEFAULT_SETTINGS });
    // React logs every caught render error; the test output stays readable.
    vi.spyOn(console, "error").mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the children untouched when nothing throws", () => {
    shouldThrow.current = false;
    render(
      <ErrorBoundary scope="screen:test">
        <Bomb />
      </ErrorBoundary>,
    );
    expect(screen.getByText("recovered content")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows a fallback with the actions and keeps the siblings alive", () => {
    render(
      <div>
        <p>shell stays</p>
        <ErrorBoundary scope="screen:test">
          <Bomb message="kaboom" />
        </ErrorBoundary>
      </div>,
    );
    expect(screen.getByText("shell stays")).toBeInTheDocument();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Something went wrong");
    expect(alert).toHaveTextContent("kaboom");
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy details" })).toBeInTheDocument();
  });

  it("logs the crash with its scope through the frontend logger", async () => {
    render(
      <ErrorBoundary scope="office:writer">
        <Bomb message="render exploded" />
      </ErrorBoundary>,
    );
    await waitFor(() => expect(logCalls()).toHaveLength(1));
    const [entry] = logCalls() as { level: string; message: string }[];
    expect(entry.level).toBe("error");
    expect(entry.message).toContain("render crash");
    expect(entry.message).toContain("scope: office:writer");
    expect(entry.message).toContain("render exploded");
  });

  it("remounts the children when Try again is pressed", async () => {
    const user = userEvent.setup();
    render(
      <ErrorBoundary scope="screen:test">
        <Bomb />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();

    // The bug is gone by the time the user retries.
    shouldThrow.current = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByText("recovered content")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("falls back again when the retry throws again", async () => {
    const user = userEvent.setup();
    render(
      <ErrorBoundary scope="screen:test">
        <Bomb />
      </ErrorBoundary>,
    );
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByRole("alert")).toBeInTheDocument();
    await waitFor(() => expect(logCalls().length).toBeGreaterThanOrEqual(2));
  });

  it("copies the details to the clipboard", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(
      <ErrorBoundary scope="screen:ocr">
        <Bomb message="clipboard check" />
      </ErrorBoundary>,
    );
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    expect(writeText).toHaveBeenCalledTimes(1);
    const text = String((writeText.mock.calls[0] as unknown[])[0]);
    expect(text).toContain("scope: screen:ocr");
    expect(text).toContain("clipboard check");
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("survives an unavailable clipboard", async () => {
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    render(
      <ErrorBoundary scope="screen:test">
        <Bomb />
      </ErrorBoundary>,
    );
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    expect(screen.getByRole("button", { name: "Copy details" })).toBeInTheDocument();
  });

  it("calls onError once and ignores a throwing handler", () => {
    const onError = vi.fn(() => {
      throw new Error("handler failed");
    });
    render(
      <ErrorBoundary scope="office:calc" onError={onError}>
        <Bomb message="first" />
      </ErrorBoundary>,
    );
    expect(onError).toHaveBeenCalledTimes(1);
    expect((onError.mock.calls[0] as unknown as [Error])[0].message).toBe("first");
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("shows the extra note and translates the fallback", () => {
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "tr" } });
    render(
      <ErrorBoundary scope="office:writer" note="Ek not.">
        <Bomb />
      </ErrorBoundary>,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Bir şeyler ters gitti");
    expect(alert).toHaveTextContent("Ek not.");
    expect(screen.getByRole("button", { name: "Yeniden dene" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ayrıntıları kopyala" })).toBeInTheDocument();
  });
});

describe("describeCrash", () => {
  it("starts with the scope and includes the message, stack and component stack", () => {
    const error = new Error("bad state");
    error.stack = "Error: bad state\n    at Thing (file.js:1:1)";
    const text = describeCrash("screen:home", error, "\n    at Home\n    at App");
    expect(text.split("\n")[0]).toBe("scope: screen:home");
    expect(text).toContain("Error: bad state");
    expect(text).toContain("at Thing (file.js:1:1)");
    expect(text).toContain("component stack:");
    expect(text).toContain("at Home");
  });

  it("adds the headline when the engine stack does not carry it", () => {
    const error = new TypeError("not a function");
    error.stack = "run@file.js:2:3";
    expect(describeCrash("x", error)).toContain("TypeError: not a function\nrun@file.js:2:3");
  });

  it("wraps non-Error throws and caps the length", () => {
    expect(describeCrash("x", "plain string")).toContain("Error: plain string");
    const huge = new Error("y");
    huge.stack = "Error: y\n" + "at frame\n".repeat(2000);
    expect(describeCrash("x", huge).length).toBeLessThanOrEqual(4000);
  });
});
