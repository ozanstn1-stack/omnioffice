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

export const ATTRIBUTES: string[];
export const PROPS: string[];

export function literalText(initializer: unknown, source: unknown): string | null;
export function scanSource(name: string, body: string): ScanResult;
export function countByFile(entries: [string, ScanResult][]): Record<string, number>;
