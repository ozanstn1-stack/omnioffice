import { describe, expect, it, vi } from "vitest";

import { EVAL_CASES, SUITE_SCORE_FLOOR, formatEvalReport, runEvalSuite, scorePack } from "./ai-task-pack-eval";
import { TASK_PACKS, getTaskPack } from "./ai-task-packs";
import { MAX_TASK_PACK_ATTEMPTS, runTaskPack, type TaskPackAsk } from "./ai-task-pack-runner";

describe("task-pack eval harness (D5)", () => {
  it("keeps golden and negative cases for every pack", () => {
    for (const pack of TASK_PACKS) {
      const cases = EVAL_CASES[pack.id];
      expect(cases.length, pack.id).toBeGreaterThanOrEqual(3);
      expect(
        cases.some((testCase) => testCase.ok),
        `${pack.id} needs a golden answer`,
      ).toBe(true);
      expect(
        cases.some((testCase) => !testCase.ok),
        `${pack.id} needs a rejected answer`,
      ).toBe(true);
      expect(new Set(cases.map((testCase) => testCase.id)).size).toBe(cases.length);
    }
  });

  it("scores the golden answers at 100 % and reports every pack", () => {
    const report = runEvalSuite();
    expect(report.ok).toBe(true);
    expect(report.score).toBe(1);
    expect(report.packs.map((entry) => entry.pack)).toEqual(TASK_PACKS.map((pack) => pack.id));
    const table = formatEvalReport(report);
    expect(table).toContain("suite");
    expect(table).toContain("duzelt-dilekce");
    expect(table).toContain("mahkeme-dilekce");
  });

  it("catches a regression when the local model starts answering badly", () => {
    // Every second golden answer is replaced by an empty object: a model that
    // lost its JSON discipline must pull the score under the floor.
    const degraded = Object.fromEntries(
      Object.entries(EVAL_CASES).map(([id, cases]) => [
        id,
        cases.map((testCase, index) => (testCase.ok && index % 2 === 0 ? { ...testCase, answer: "{}\n" } : testCase)),
      ]),
    );
    const report = runEvalSuite(degraded);
    expect(report.score).toBeLessThan(SUITE_SCORE_FLOOR);
    expect(report.ok).toBe(false);
    expect(report.packs.some((entry) => entry.score < SUITE_SCORE_FLOOR)).toBe(true);
  });

  it("scores one pack in isolation", () => {
    const pack = getTaskPack("kdv-ozeti");
    const score = scorePack(pack, EVAL_CASES["kdv-ozeti"]);
    expect(score.passed).toBe(EVAL_CASES["kdv-ozeti"].length);
    expect(scorePack(pack, [])).toMatchObject({ passed: 0, total: 0, score: 0 });
  });
});

describe("strict JSON runner (D5)", () => {
  it("accepts a valid first answer without a repair turn", async () => {
    const run = await runTaskPack(getTaskPack("tebligat-sure"), async () => EVAL_CASES["tebligat-sure"][0].answer);
    expect(run.attempts).toBe(1);
    expect(run.result.ok).toBe(true);
    expect(run.answers).toHaveLength(1);
  });

  it("sends one repair turn with the concrete problems and then accepts", async () => {
    const golden = EVAL_CASES["tebligat-sure"][0].answer;
    const ask = vi.fn<TaskPackAsk>(async (question) =>
      question.includes("ÖNCEKİ YANIT GEÇERSİZ") ? golden : "Tebliğ tarihi 01.10.2026, süre iki hafta.",
    );
    const run = await runTaskPack(getTaskPack("tebligat-sure"), ask);
    expect(ask).toHaveBeenCalledTimes(2);
    const repair = ask.mock.calls[1][0];
    expect(repair).toContain("ÖNCEKİ YANIT GEÇERSİZ");
    expect(repair).toContain("serviceDate");
    expect(run.attempts).toBe(2);
    expect(run.result.ok).toBe(true);
    expect(run.questions).toHaveLength(2);
    expect(run.answers[0]).not.toBe(run.answers[1]);
  });

  it("stops after the repair budget and reports the failures", async () => {
    const run = await runTaskPack(getTaskPack("kdv-ozeti"), async () => "bu bir JSON değil");
    expect(run.attempts).toBe(MAX_TASK_PACK_ATTEMPTS);
    expect(run.result.ok).toBe(false);
    expect(run.result.issues.some((issue) => issue.code === "json")).toBe(true);
  });

  it("never calls the network or telemetry itself", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network access not allowed"));
    const beaconSpy = vi.fn(() => true);
    Object.defineProperty(navigator, "sendBeacon", { configurable: true, value: beaconSpy });

    // The harness and the runner are provider-agnostic: they only see answers.
    runEvalSuite();
    await runTaskPack(getTaskPack("mahkeme-dilekce"), async () => EVAL_CASES["mahkeme-dilekce"][0].answer);

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(beaconSpy).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });
});
