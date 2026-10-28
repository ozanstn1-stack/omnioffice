import { describe, expect, it } from "vitest";
import { DESKS, PROFESSIONS, deskTasks, isProfession } from "./desks";
import { SCREEN_IDS, type ScreenId } from "./routes";
import en from "./i18n/en.json";
import tr from "./i18n/tr.json";

const allEnabled = () => true;

describe("profession desks", () => {
  it("gives every profession six to eight tasks", () => {
    for (const profession of PROFESSIONS) {
      const tasks = deskTasks(profession);
      expect(tasks.length, profession).toBeGreaterThanOrEqual(6);
      expect(tasks.length, profession).toBeLessThanOrEqual(8);
    }
  });

  it("points every task at a real screen, with its own key", () => {
    for (const profession of PROFESSIONS) {
      const keys = DESKS[profession].map((task) => task.key);
      expect(new Set(keys).size, profession).toBe(keys.length);
      for (const task of DESKS[profession]) {
        expect(SCREEN_IDS as readonly string[]).toContain(task.screen);
      }
    }
  });

  it("leaves out a task whose screen is behind an off flag", () => {
    const hidden: ScreenId = "kvkk";
    const tasks = deskTasks("lawyer", (id) => id !== hidden);
    expect(tasks.map((task) => task.screen)).not.toContain(hidden);
    expect(deskTasks("lawyer", allEnabled).length).toBeGreaterThan(tasks.length);
  });

  it("has no desk for the general choice or before a choice", () => {
    expect(deskTasks("general")).toEqual([]);
    expect(deskTasks("")).toEqual([]);
    expect(isProfession("general")).toBe(false);
    expect(isProfession("")).toBe(false);
    expect(isProfession("lawyer")).toBe(true);
  });

  it("names every profession and task in English and Turkish", () => {
    const dictionaries: Record<string, Record<string, string>> = { en, tr };
    for (const [language, dict] of Object.entries(dictionaries)) {
      for (const profession of PROFESSIONS) {
        expect(dict[`desk.profession.${profession}`], `${language} ${profession}`).toBeTruthy();
        expect(dict[`desk.title.${profession}`], `${language} ${profession}`).toBeTruthy();
        for (const task of DESKS[profession]) {
          expect(dict[`desk.task.${task.key}Title`], `${language} ${task.key}`).toBeTruthy();
          expect(dict[`desk.task.${task.key}Body`], `${language} ${task.key}`).toBeTruthy();
        }
      }
    }
  });
});
