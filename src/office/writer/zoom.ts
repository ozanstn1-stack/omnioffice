/**
 * Writer zoom helpers.
 *
 * A4 is 794 CSS px wide at 100 %, about twice a phone's width (~412 px), so a
 * document opened on a phone at 100 % showed a page wider than the screen. The
 * editor now starts at "fit width" on narrow screens; desktop keeps 100 %.
 */

/** Horizontal room the editor chrome takes around the page (scroll padding, sheet border). */
const GUTTER_PX = 32;

/** Below this viewport width the editor starts fitted to the width. */
export const NARROW_VIEWPORT_PX = 700;

/** The zoom at which a page of `pageWidthPt` fills `viewportWidthPx`, clamped to 25-100 %. */
export function fitWidthZoom(pageWidthPt: number, viewportWidthPx: number): number {
  const pageWidthPx = pageWidthPt * (96 / 72);
  if (!(pageWidthPx > 0) || !(viewportWidthPx > 0)) return 1;
  const fitted = (viewportWidthPx - GUTTER_PX) / pageWidthPx;
  return Math.min(1, Math.max(0.25, Math.floor(fitted * 100) / 100));
}

/** The zoom a Writer tab opens with: fit width on a phone-sized viewport, 100 % otherwise. */
export function initialWriterZoom(pageWidthPt: number, viewportWidthPx: number): number {
  return viewportWidthPx < NARROW_VIEWPORT_PX ? fitWidthZoom(pageWidthPt, viewportWidthPx) : 1;
}
