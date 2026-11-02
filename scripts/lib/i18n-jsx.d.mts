// Types for the JSX scanner, so the vitest suite can import the ESM module
// directly and TypeScript still checks the test's expectations.

export interface FoundText {
  line: number;
  text: string;
}

export interface FoundNamed extends FoundText {
  name: string;
}

export interface ScanResult {
  texts: FoundText[];
  attributes: FoundNamed[];
  props: FoundNamed[];
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

export function literalText(initializer: unknown, source: unknown): string | null;
export function scanSource(name: string, body: string): ScanResult;
export function countByFile(entries: [string, ScanResult][]): Record<string, number>;
export function scanObjects(name: string, body: string): ObjectScanResult;
