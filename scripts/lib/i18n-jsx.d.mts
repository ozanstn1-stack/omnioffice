// Types for the JSX scanner, so the vitest suite can import the ESM module
// directly and TypeScript still checks the test's expectations.

export interface FoundText {
  line: number;
  text: string;
}

export interface FoundNamed extends FoundText {
  name: string;
}

export interface FoundIgnored extends FoundText {
  kind: "text" | "attribute" | "prop";
  name?: string;
  via?: string;
  /** The justification the marker carries, verbatim. */
  reason: string;
  /** The line the marker itself sits on. */
  markerLine: number;
}

export interface InvalidMarker {
  line: number;
  problem: "missing-reason" | "weak-reason" | "orphan" | "ambiguous" | "in-jsx-text";
}

export interface ScanResult {
  texts: FoundText[];
  attributes: FoundNamed[];
  props: FoundNamed[];
  /** Findings a `i18n-literal:` marker silenced, with the reason. */
  ignored: FoundIgnored[];
  /** Markers that cannot be used; any of them fails the audit. */
  invalidMarkers: InvalidMarker[];
}

export interface FoundObject extends FoundText {
  key: string;
}

export interface FoundExcluded extends FoundObject {
  reason: string;
}

export interface ContentPath {
  path: string;
  reason: string;
}

export interface ObjectScanResult {
  contentFile: ContentPath | null;
  /** Strings the baseline measures. */
  counted: FoundObject[];
  /** Strings a `i18n-content:` pragma keeps out, with its reason. */
  excluded: FoundExcluded[];
  /** Matching keys whose value carries no Turkish letter - info only. */
  unmarked: FoundObject[];
}

export const ATTRIBUTES: string[];
export const PROPS: string[];
export const OBJECT_KEYS: string[];
export const CONTENT_PATHS: ContentPath[];

export const LITERAL_PRAGMA: string;
export const LITERAL_HARD_CAP: number;
export function literalReasonProblem(reason: unknown): string | null;
export function findLiteralMarkers(body: string): {
  line: number;
  boundLine: number;
  reason: string;
  standalone: boolean;
}[];
export function bindLiteralMarkers(
  findings: FoundIgnored[],
  markers: { line: number; boundLine: number; reason: string; standalone: boolean }[],
): { kept: FoundIgnored[]; ignored: FoundIgnored[]; invalid: InvalidMarker[] };
export function literalText(initializer: unknown, source: unknown): string | null;
export function scanSource(name: string, body: string): ScanResult;
export function countByFile(entries: [string, ScanResult][]): Record<string, number>;
export function scanObjects(name: string, body: string): ObjectScanResult;
