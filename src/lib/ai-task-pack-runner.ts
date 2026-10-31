/**
 * Runs a task pack against a provider with a strict JSON contract (D5).
 *
 * The first answer is verified by the pack itself. When it does not verify —
 * malformed JSON, a missing field, broken arithmetic — the runner sends one
 * repair turn with the concrete problems and the expected shape, so a local
 * model that "almost" answered gets a second chance instead of surfacing a raw
 * parse error. The runner never touches the network: `ask` is injected (the
 * screen passes `aiChat`), which is what the isolation test pins.
 */
import { buildTaskPackQuestion, verifyTaskPackOutput, type TaskPack, type TaskPackResult } from "./ai-task-packs";

/** Sends one question to the provider and returns its raw answer. */
export type TaskPackAsk = (question: string) => Promise<string>;

export interface TaskPackRun {
  result: TaskPackResult;
  /** 1 when the first answer verified, 2 after one repair turn. */
  attempts: number;
  /** The questions sent, in order. */
  questions: string[];
  /** The raw answers received, in order. */
  answers: string[];
}

/** One repair turn: a local model should not get more than that. */
export const MAX_TASK_PACK_ATTEMPTS = 2;

/** The second question: the pack rules, the problems and the rejected answer. */
export function repairQuestion(pack: TaskPack, previous: string, issues: TaskPackResult["issues"]): string {
  const problems = issues.map((issue) => (issue.field ? `${issue.field} (${issue.code})` : issue.code)).join(", ");
  const excerpt = previous.length > 1200 ? `${previous.slice(0, 1200)}…` : previous;
  return [
    buildTaskPackQuestion(pack),
    "",
    `ÖNCEKİ YANIT GEÇERSİZ. Sorunlar: ${problems}`,
    "Önceki yanıt:",
    excerpt,
    "Yalnızca geçerli JSON nesnesi döndür; sorunları düzelt ve tekrar dene.",
  ].join("\n");
}

/** Runs a pack with at most one repair turn and reports every attempt. */
export async function runTaskPack(pack: TaskPack, ask: TaskPackAsk): Promise<TaskPackRun> {
  const questions: string[] = [];
  const answers: string[] = [];
  let question = buildTaskPackQuestion(pack);
  let result: TaskPackResult = { ok: false, parsed: null, issues: [] };
  for (let attempt = 1; attempt <= MAX_TASK_PACK_ATTEMPTS; attempt += 1) {
    questions.push(question);
    const raw = await ask(question);
    answers.push(raw);
    result = verifyTaskPackOutput(pack, raw);
    if (result.ok) break;
    if (attempt < MAX_TASK_PACK_ATTEMPTS) {
      question = repairQuestion(pack, raw, result.issues);
    }
  }
  return { result, attempts: questions.length, questions, answers };
}
