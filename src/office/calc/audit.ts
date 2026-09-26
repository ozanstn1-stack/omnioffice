/**
 * Formula auditing for the Calc editor.
 *
 * The dependency graph in `cells.ts` already knows which formula reads which
 * cells. These helpers turn that graph into the answers an auditing UI needs:
 * the transitive precedents/dependents of one cell, the cycles in the
 * workbook, and the formulas whose references cannot resolve.
 *
 * Everything here is a pure function over the `Workbook` model (the graph is a
 * cache owned by `cells.ts`, exactly like the value cache), so it can be unit
 * tested without mounting the editor.
 */
import type { Workbook } from "../../lib/office-types";
import { computeWorkbookValues, dependencyGraph } from "./cells";
import { collectReferences, isError } from "./formula";

/** One cell reached by a trace, with its distance from the traced cell. */
export interface AuditNode {
  sheet: string;
  address: string;
  /** 1 for a direct neighbour, 2 for a neighbour of a neighbour, ... */
  depth: number;
}

/** A formula cell that points at something that does not exist. */
export interface InvalidReference {
  sheet: string;
  address: string;
  /** The offending reference text, or the whole formula for an error value. */
  reference: string;
  reason: "missing-sheet" | "error";
}

function keyOf(sheet: string, address: string): string {
  return `${sheet}!${address}`;
}

function splitKey(key: string): { sheet: string; address: string } {
  const at = key.lastIndexOf("!");
  return { sheet: key.slice(0, at), address: key.slice(at + 1) };
}

/** Breadth-first walk over one edge map, starting at `start` (excluded). */
function walk(edges: Map<string, Set<string>>, start: string): AuditNode[] {
  const visited = new Set([start]);
  const queue: Array<{ key: string; depth: number }> = [{ key: start, depth: 0 }];
  const nodes: AuditNode[] = [];
  while (queue.length > 0) {
    const { key, depth } = queue.shift()!;
    for (const next of edges.get(key) ?? []) {
      if (visited.has(next)) continue;
      visited.add(next);
      nodes.push({ ...splitKey(next), depth: depth + 1 });
      queue.push({ key: next, depth: depth + 1 });
    }
  }
  return nodes;
}

/** Every cell the formula at `sheet!address` reads, directly or through a chain. */
export function tracePrecedents(workbook: Workbook, sheet: string, address: string): AuditNode[] {
  return walk(dependencyGraph(workbook).precedents, keyOf(sheet, address));
}

/** Every formula that reads `sheet!address`, directly or through a chain. */
export function traceDependents(workbook: Workbook, sheet: string, address: string): AuditNode[] {
  return walk(dependencyGraph(workbook).dependents, keyOf(sheet, address));
}

/** Rotation-independent identity for a cycle, used to list each cycle once. */
function canonicalCycle(cycle: string[]): string {
  const body = cycle.slice(0, -1);
  let best = body;
  for (let offset = 1; offset < body.length; offset += 1) {
    const rotated = [...body.slice(offset), ...body.slice(0, offset)];
    if (rotated.join(">") < best.join(">")) best = rotated;
  }
  return best.join(">");
}

/**
 * Every reference cycle in the workbook, as sheet-qualified cell keys.
 *
 * A cycle repeats its first key at the end (`["Sheet1!A1", "Sheet1!A2",
 * "Sheet1!A1"]`) so the UI can print the path without guessing where it
 * starts. Rotations of the same cycle are reported once.
 */
export function findCircularReferences(workbook: Workbook): string[][] {
  const graph = dependencyGraph(workbook);
  const formulaCells = new Set(graph.precedents.keys());
  const cycles: string[][] = [];
  const seen = new Set<string>();
  const state = new Map<string, "open" | "done">();
  const path: string[] = [];

  const visit = (key: string): void => {
    state.set(key, "open");
    path.push(key);
    for (const next of graph.precedents.get(key) ?? []) {
      if (!formulaCells.has(next)) continue;
      if (state.get(next) === "open") {
        const cycle = [...path.slice(path.indexOf(next)), next];
        const canonical = canonicalCycle(cycle);
        if (!seen.has(canonical)) {
          seen.add(canonical);
          cycles.push(cycle);
        }
        continue;
      }
      if (state.get(next) !== "done") visit(next);
    }
    path.pop();
    state.set(key, "done");
  };

  for (const key of formulaCells) {
    if (state.get(key) !== "done") visit(key);
  }
  return cycles;
}

/**
 * Formulas that cannot resolve: a reference to a sheet that does not exist,
 * and cells whose computed value is `#REF!` (including circular references).
 */
export function invalidReferences(workbook: Workbook): InvalidReference[] {
  const sheetNames = workbook.sheets.map((sheet) => sheet.name);
  const hasSheet = (name: string) => sheetNames.some((candidate) => candidate.toLowerCase() === name.toLowerCase());
  const values = computeWorkbookValues(workbook);
  const issues: InvalidReference[] = [];
  const seen = new Set<string>();
  const push = (issue: InvalidReference): void => {
    const id = `${issue.sheet}!${issue.address}|${issue.reference}|${issue.reason}`;
    if (seen.has(id)) return;
    seen.add(id);
    issues.push(issue);
  };

  for (const sheet of workbook.sheets) {
    for (const [address, cell] of Object.entries(sheet.cells)) {
      if (!cell.formula) continue;
      // A reference to a missing sheet already explains the #REF! that the
      // evaluator produces; reporting both would just duplicate the cell.
      let explained = false;
      const summary = collectReferences(cell.formula);
      if (summary) {
        for (const ref of summary.refs) {
          if (ref.sheet && !hasSheet(ref.sheet)) {
            push({ sheet: sheet.name, address, reference: `${ref.sheet}!${ref.address}`, reason: "missing-sheet" });
            explained = true;
          }
        }
        for (const range of summary.ranges) {
          if (range.sheet && !hasSheet(range.sheet)) {
            push({ sheet: sheet.name, address, reference: `${range.sheet}!${range.range}`, reason: "missing-sheet" });
            explained = true;
          }
        }
      }
      const value = values.get(keyOf(sheet.name, address));
      if (!explained && isError(value) && value.code === "#REF!") {
        push({ sheet: sheet.name, address, reference: cell.formula, reason: "error" });
      }
    }
  }

  return issues.sort(
    (a, b) => a.sheet.localeCompare(b.sheet) || a.address.localeCompare(b.address) || a.reference.localeCompare(b.reference),
  );
}
