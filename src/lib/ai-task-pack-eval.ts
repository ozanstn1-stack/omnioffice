/**
 * The Turkish task-pack evaluation harness (D5).
 *
 * Each pack has golden answers that a local model is expected to produce, plus
 * the answers it must *not* get away with (broken arithmetic, missing fields,
 * wrong dates). Scoring them through the same verifier the screen uses turns
 * "the model seems fine" into a measured number: a pack that loses a golden
 * answer is a regression, and the suite fails when the overall score drops
 * below the floor.
 *
 * The harness is provider-agnostic: `runEvalSuite` takes the answers from a
 * provider function. The tests feed it the golden answers (a deterministic
 * stand-in for the local Ollama/LM Studio model) and a degraded provider to
 * prove that a regression is caught. Nothing here calls a network.
 */
import { TASK_PACKS, verifyTaskPackOutput, type TaskPack, type TaskPackId } from "./ai-task-packs";

export interface EvalCase {
  id: string;
  /** A recorded model answer (JSON text). */
  answer: string;
  /** True when the pack's verifier must accept it. */
  ok: boolean;
}

/** Minimum share of golden answers a pack may lose before the suite fails. */
export const SUITE_SCORE_FLOOR = 0.9;

export interface PackScore {
  pack: TaskPackId;
  passed: number;
  total: number;
  /** 0..1 */
  score: number;
}

export interface EvalReport {
  packs: PackScore[];
  passed: number;
  total: number;
  /** 0..1 */
  score: number;
  /** True when every pack and the suite itself stay above the floor. */
  ok: boolean;
}

const goodPetition = JSON.stringify({
  summary: "Dilekçe, hitap ve tarih dışında uygun.",
  issues: [
    { severity: "medium", title: "Hitap eksik", suggestion: "İlgili makama hitap ekleyin." },
    { severity: "low", title: "Noktalama", suggestion: "Cümle sonlarına nokta koyun." },
  ],
  corrected: "T.C. ... Sayın İlgili Makam ...",
});

const goodContract = JSON.stringify({
  summary: "İki taraflı hizmet sözleşmesi.",
  clauses: [
    { heading: "1. Konu", summary: "Hizmetin kapsamını tanımlar." },
    { heading: "3. Bedel", summary: "Ödeme koşullarını belirler.", risk: "Ödeme tarihi belirsiz." },
  ],
});

const goodVat = JSON.stringify({
  rows: [
    { rate: 20, base: 1000, vat: 200 },
    { rate: 10, base: 500, vat: 50 },
  ],
  totalBase: 1500,
  totalVat: 250,
});

const goodDeadline = JSON.stringify({
  serviceDate: "2026-10-01",
  periodDays: 14,
  lastDay: "2026-10-15",
  reasoning: "Tebliğden sonraki günden itibaren iki hafta.",
});

const goodInvoice = JSON.stringify({
  supplier: "Örnek Bilişim A.Ş.",
  customer: "Örnek Müşteri Ltd.",
  invoiceDate: "2026-10-05",
  rows: [
    { description: "Lisans", quantity: 2, unitPrice: 500, vatRate: 20, lineBase: 1000, lineVat: 200 },
    { description: "Destek", quantity: 1, unitPrice: 300, vatRate: 10, lineBase: 300, lineVat: 30 },
  ],
  totalBase: 1300,
  totalVat: 230,
  total: 1530,
});

const goodKep = JSON.stringify({
  sender: "Örnek Kurum",
  subject: "İdari para cezası tebliği",
  serviceDate: "2026-10-02",
  periodDays: 30,
  lastDay: "2026-11-01",
  sections: [
    { heading: "Tebliğ", summary: "Ceza 02.10.2026 tarihinde tebliğ edildi." },
    { heading: "Süre", summary: "Otuz gün içinde itiraz edilebilir." },
  ],
  reasoning: "Tebliğ tarihini izleyen günden itibaren otuz gün.",
});

const goodCourt = JSON.stringify({
  courtType: "asliye-hukuk",
  subject: "Kira alacağı tahsili",
  requests: ["Kira alacağının tahsili", "Yargılama giderleri"],
  evidence: ["Kira sözleşmesi", "Ödeme dekontları"],
  summary: "Kira alacağının tahsili için açılmış bir dava dilekçesi.",
});

export const EVAL_CASES: Record<TaskPackId, EvalCase[]> = {
  "duzelt-dilekce": [
    { id: "golden", answer: goodPetition, ok: true },
    { id: "code-fence", answer: "```json\n" + goodPetition + "\n```", ok: true },
    { id: "missing-corrected", answer: JSON.stringify({ summary: "Uygun.", issues: [] }), ok: false },
    { id: "bad-severity", answer: goodPetition.replace('"medium"', '"urgent"'), ok: false },
  ],
  "ozetle-sozlesme": [
    { id: "golden", answer: goodContract, ok: true },
    { id: "no-clauses", answer: JSON.stringify({ summary: "Sözleşme.", clauses: [] }), ok: false },
    { id: "wrong-type", answer: JSON.stringify({ summary: "Sözleşme.", clauses: "yok" }), ok: false },
  ],
  "kdv-ozeti": [
    { id: "golden", answer: goodVat, ok: true },
    {
      id: "bad-vat-math",
      answer: JSON.stringify({ rows: [{ rate: 20, base: 1000, vat: 100 }], totalBase: 1000, totalVat: 100 }),
      ok: false,
    },
    { id: "totals-mismatch", answer: goodVat.replace('"totalBase":1500', '"totalBase":1400'), ok: false },
  ],
  "tebligat-sure": [
    { id: "golden", answer: goodDeadline, ok: true },
    { id: "date-order", answer: goodDeadline.replace("2026-10-15", "2026-09-15"), ok: false },
    { id: "bad-format", answer: goodDeadline.replace("2026-10-01", "01.10.2026"), ok: false },
  ],
  "efatura-analiz": [
    { id: "golden", answer: goodInvoice, ok: true },
    { id: "line-math", answer: goodInvoice.replace('"lineVat":30', '"lineVat":100'), ok: false },
    {
      id: "total-mismatch",
      answer: goodInvoice.replace('"total":1530', '"total":1600'),
      ok: false,
    },
  ],
  "kep-tebligat": [
    { id: "golden", answer: goodKep, ok: true },
    { id: "last-before-service", answer: goodKep.replace("2026-11-01", "2026-09-30"), ok: false },
    { id: "no-sections", answer: goodKep.replace(/"sections":\[[\s\S]*?\]/, '"sections":[]'), ok: false },
  ],
  "mahkeme-dilekce": [
    { id: "golden", answer: goodCourt, ok: true },
    { id: "unknown-court", answer: goodCourt.replace('"asliye-hukuk"', '"bölge-adliyesi"'), ok: false },
    { id: "no-evidence", answer: goodCourt.replace(/"evidence":\[[^\]]*\]/, '"evidence":[]'), ok: false },
  ],
};

/** Scores one pack's cases with the verifier the screen uses. */
export function scorePack(pack: TaskPack, cases: EvalCase[]): PackScore {
  let passed = 0;
  for (const testCase of cases) {
    const result = verifyTaskPackOutput(pack, testCase.answer);
    if (result.ok === testCase.ok) passed += 1;
  }
  return { pack: pack.id, passed, total: cases.length, score: cases.length === 0 ? 0 : passed / cases.length };
}

/** Scores every pack from recorded answers (the default local-model stand-in). */
export function runEvalSuite(cases: Partial<Record<TaskPackId, EvalCase[]>> = EVAL_CASES): EvalReport {
  const packs = TASK_PACKS.map((pack) => scorePack(pack, cases[pack.id] ?? []));
  const passed = packs.reduce((sum, entry) => sum + entry.passed, 0);
  const total = packs.reduce((sum, entry) => sum + entry.total, 0);
  const score = total === 0 ? 0 : passed / total;
  const ok =
    total > 0 &&
    score >= SUITE_SCORE_FLOOR &&
    packs.every((entry) => entry.total === 0 || entry.score >= SUITE_SCORE_FLOOR);
  return { packs, passed, total, score, ok };
}

/** One line per pack, for the test output and the release note. */
export function formatEvalReport(report: EvalReport): string {
  const rows = report.packs.map(
    (entry) => `${entry.pack.padEnd(16)} ${entry.passed}/${entry.total} (${Math.round(entry.score * 100)} %)`,
  );
  rows.push(`suite            ${report.passed}/${report.total} (${Math.round(report.score * 100)} %)`);
  return rows.join("\n");
}
