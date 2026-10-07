/**
 * Quick style gallery for the Writer's Home ribbon.
 *
 * One button per everyday paragraph style, drawn in a scaled-down copy of that
 * style (size, weight, italics from the style chain) so the choice is visible
 * before it is applied. The full list stays in the style dropdown.
 */
import { useT } from "../../lib/i18n";
import { defaultParaProps, effectiveStyle, type TextDocument } from "../../lib/office-types";

export const GALLERY_STYLES = [
  { id: "Normal", label: "writer.styleNormal" },
  { id: "Title", label: "writer.styleTitle" },
  { id: "Heading1", label: "writer.styleHeading1" },
  { id: "Heading2", label: "writer.styleHeading2" },
  { id: "Heading3", label: "writer.styleHeading3" },
  { id: "Quote", label: "writer.styleQuote" },
] as const;

/** Preview text size in px: far enough apart to tell styles apart, small enough for one ribbon row. */
export function previewSizePx(sizePt: number): number {
  return Math.round(Math.min(18, Math.max(11, 9 + sizePt * 0.33)) * 10) / 10;
}

export function StyleGallery({
  document,
  active,
  onApply,
}: {
  document: TextDocument;
  /** Style id of the paragraph with the caret. */
  active: string;
  onApply: (styleId: string) => void;
}) {
  const t = useT();
  // A document without one of these styles (some imports) does not get a
  // button that would apply an undefined style.
  const items = GALLERY_STYLES.filter((entry) => document.styles.some((style) => style.id === entry.id));
  return (
    <div className="style-gallery" role="group" aria-label={t("writer.styleGallery")}>
      {items.map((entry) => {
        const look = effectiveStyle(document, defaultParaProps(entry.id));
        const isActive = active === entry.id;
        return (
          <button
            key={entry.id}
            type="button"
            className={`style-gallery-item${isActive ? " is-active" : ""}`}
            aria-pressed={isActive}
            style={{
              fontSize: `${previewSizePx(look.sizePt ?? 11)}px`,
              fontWeight: look.bold ? 700 : 400,
              fontStyle: look.italic ? "italic" : "normal",
            }}
            // Keep the caret in the paragraph: the style applies to the
            // focused block, and a focused button would take that away.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onApply(entry.id)}
          >
            {t(entry.label)}
          </button>
        );
      })}
    </div>
  );
}
