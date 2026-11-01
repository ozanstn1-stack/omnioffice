/**
 * Working copies of the checked-in samples, so a flow never edits the repository's own files.
 */
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { root, work } from "../lib/driver.mjs";

export function prepareWorkFile(source) {
  mkdirSync(work, { recursive: true });
  const target = join(work, source);
  copyFileSync(join(root, "samples", source), target);
  return target;
}

export const elementKey = (element) => element["element-6066-11e4-a52e-4f735466cecf"] ?? element.ELEMENT;
