import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Screen } from "./layout";

// D8: on a 360 dp phone the screen actions must sit in the bottom action bar
// (thumb reach), not in the header. jsdom reports a desktop window by default,
// so the width is set before the render.
function setWindowWidth(width: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, writable: true, value: width });
}

afterEach(() => setWindowWidth(1024));

describe("phone screen action bar (D8)", () => {
  it("moves the header actions into the bottom bar at 360 dp", () => {
    setWindowWidth(360);
    render(
      <Screen title="Tarayıcı" actions={<button type="button">Kaydet</button>}>
        <p>İçerik</p>
      </Screen>,
    );
    const bar = screen.getByRole("toolbar");
    expect(bar).toHaveClass("screen-action-bar");
    expect(bar).toContainElement(screen.getByRole("button", { name: "Kaydet" }));
    // The header no longer carries the action on a phone.
    expect(document.querySelector(".screen-content header")).not.toContainElement(
      screen.getByRole("button", { name: "Kaydet" }),
    );
    expect(document.querySelector(".screen-shell")).not.toBeNull();
  });

  it("keeps the actions in the header on a desktop window", () => {
    setWindowWidth(1280);
    render(
      <Screen title="Tarayıcı" actions={<button type="button">Kaydet</button>}>
        <p>İçerik</p>
      </Screen>,
    );
    expect(screen.queryByRole("toolbar")).toBeNull();
    expect(document.querySelector(".screen-content header")).toContainElement(
      screen.getByRole("button", { name: "Kaydet" }),
    );
  });

  it("shows no bar on a phone when the screen has no actions", () => {
    setWindowWidth(360);
    render(
      <Screen title="Ayarlar">
        <p>İçerik</p>
      </Screen>,
    );
    expect(screen.queryByRole("toolbar")).toBeNull();
  });
});
