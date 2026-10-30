/**
 * Impress diff (C13): slides compared by position. A slide is compared by the
 * text of its text frames (paragraph text, falling back to the paragraph's
 * runs), which is what a reader notices without rendering the slide.
 */
import type { Deck, Slide } from "./office-types";
import { diffSegments } from "./text-diff";
import {
  countKind,
  emptySummary,
  type DiffKind,
  type DiffSummary,
  type ImpressDiff,
  type SlideChange,
} from "./office-diff-types";

/** Bound so a pathological pair cannot exhaust memory in the webview. */
export const MAX_SLIDE_CHANGES = 1_000;

function slideText(slide: Slide): string {
  const parts: string[] = [];
  for (const object of slide.objects) {
    if (!object.text) continue;
    for (const paragraph of object.text.paragraphs) {
      const text = paragraph.text || paragraph.runs.map((run) => run.text).join("");
      if (text.trim() !== "") parts.push(text);
    }
  }
  return parts.join("\n");
}

function slideTitle(slide: Slide): string {
  return slideText(slide).split("\n")[0]?.trim() || "—";
}

/** Slide-level diff: added/removed slides and the text of changed ones. */
export function diffDecks(base: Deck, current: Deck): ImpressDiff {
  const slides: SlideChange[] = [];
  const summary: DiffSummary = emptySummary();
  let truncated = false;
  const total = Math.max(base.slides.length, current.slides.length);
  for (let index = 0; index < total; index += 1) {
    if (slides.length >= MAX_SLIDE_CHANGES) {
      truncated = true;
      break;
    }
    const beforeSlide = base.slides[index];
    const afterSlide = current.slides[index];
    const before = beforeSlide ? slideText(beforeSlide) : "";
    const after = afterSlide ? slideText(afterSlide) : "";
    if (beforeSlide && afterSlide && before === after) continue;
    const kind: DiffKind = !beforeSlide ? "added" : !afterSlide ? "removed" : "changed";
    const change: SlideChange = {
      kind,
      index: index + 1,
      title: afterSlide ? slideTitle(afterSlide) : beforeSlide ? slideTitle(beforeSlide) : "—",
      before,
      after,
    };
    if (kind === "changed") change.segments = diffSegments(before, after);
    slides.push(change);
    countKind(summary, kind);
  }
  return { kind: "impress", summary, slides, truncated };
}
