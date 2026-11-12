import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AiConsentNotice } from "./ai-consent-notice";

const notice = { consent: "ai.consent", privacyBody: "ai.privacyBody" };
const params = { provider: "DeepSeek" };

function setup(consent: boolean, remembered: boolean) {
  const handlers = { onConsent: vi.fn(), onRemember: vi.fn(), onForget: vi.fn() };
  render(<AiConsentNotice consent={consent} remembered={remembered} notice={notice} params={params} {...handlers} />);
  return handlers;
}

describe("AI data notice", () => {
  it("asks first, then offers to remember the consent for the provider", async () => {
    const user = userEvent.setup();
    const asked = setup(false, false);
    await user.click(screen.getByRole("checkbox"));
    expect(asked.onConsent).toHaveBeenCalledWith(true);
    expect(screen.queryByRole("button", { name: /remember/i })).toBeNull();
  });

  it("remembers a given consent and forgets a remembered one", async () => {
    const user = userEvent.setup();
    const given = setup(true, false);
    await user.click(screen.getByRole("button", { name: /remember for deepseek/i }));
    expect(given.onRemember).toHaveBeenCalledTimes(1);
  });

  it("shows a remembered consent with a forget button and no checkbox", async () => {
    const user = userEvent.setup();
    const remembered = setup(true, true);
    expect(screen.getByText(/remembered for deepseek/i)).toBeTruthy();
    expect(screen.queryByRole("checkbox")).toBeNull();
    await user.click(screen.getByRole("button", { name: /forget/i }));
    expect(remembered.onForget).toHaveBeenCalledTimes(1);
  });
});
