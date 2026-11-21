/** Font choices for the Writer ribbon. */

/** Fonts offered in the ribbon: the common Windows and Office faces. */
export const FONT_FAMILIES = [
  "Calibri",
  "Arial",
  "Times New Roman",
  "Cambria",
  "Georgia",
  "Garamond",
  "Verdana",
  "Tahoma",
  "Segoe UI",
  "Trebuchet MS",
  "Courier New",
  "Consolas",
];

/** The font list, plus the current font when a document uses another one. */
export function fontOptions(current: string): { value: string; label: string }[] {
  const names = FONT_FAMILIES.includes(current) || !current ? FONT_FAMILIES : [current, ...FONT_FAMILIES];
  return names.map((name) => ({ value: name, label: name }));
}
