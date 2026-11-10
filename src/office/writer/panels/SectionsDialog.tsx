import { Trash2 } from "lucide-react";
import { Dialog } from "../../office-ui";
import { useT } from "../../../lib/i18n";
import { defaultPageSetup, type SectionProps } from "../../../lib/office-types";

const SIZES = ["a4", "a5", "letter", "legal", "a3"];

/** The page size in points for a named paper size and orientation. */
export function sizeDimensions(size: string, orientation: string): { widthPt: number; heightPt: number } {
  const setup = defaultPageSetup(size, orientation);
  return { widthPt: setup.widthPt, heightPt: setup.heightPt };
}

/** Writer's sections dialog: paper size, orientation and start of each section. */
export function SectionsDialog({
  sections,
  onUpdate,
  onRemoveBreak,
  onClose,
}: {
  sections: SectionProps[];
  onUpdate: (index: number, patch: Partial<SectionProps>) => void;
  onRemoveBreak: (index: number) => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <Dialog title={t("writer.sections")} onClose={onClose}>
      <div className="stack">
        {sections.map((section, sectionIndex) => (
          <div key={sectionIndex} className="section-card">
            <div className="row">
              <strong>
                {t("writer.section")} {sectionIndex + 1}
              </strong>
              <span className="spacer" />
              {sectionIndex > 0 ? (
                <button
                  type="button"
                  className="icon-btn"
                  title={t("common.delete")}
                  onClick={() => onRemoveBreak(sectionIndex)}
                >
                  <Trash2 size={13} />
                </button>
              ) : null}
            </div>
            <div className="row">
              <select
                value={section.page.size}
                onChange={(event) =>
                  onUpdate(sectionIndex, {
                    page: {
                      ...section.page,
                      ...sizeDimensions(event.target.value, section.page.orientation),
                      size: event.target.value,
                    },
                  })
                }
              >
                {SIZES.map((size) => (
                  <option key={size} value={size}>
                    {size.toUpperCase()}
                  </option>
                ))}
              </select>
              <select
                value={section.page.orientation}
                onChange={(event) => {
                  const landscape = event.target.value === "landscape";
                  const currently = section.page.widthPt > section.page.heightPt;
                  const page = { ...section.page, orientation: event.target.value };
                  if (landscape !== currently) {
                    const width = page.widthPt;
                    page.widthPt = page.heightPt;
                    page.heightPt = width;
                  }
                  onUpdate(sectionIndex, { page });
                }}
              >
                <option value="portrait">{t("writer.portrait")}</option>
                <option value="landscape">{t("writer.landscape")}</option>
              </select>
              <select
                value={section.start}
                onChange={(event) =>
                  sectionIndex === sections.length - 1
                    ? undefined
                    : onUpdate(sectionIndex, { start: event.target.value })
                }
              >
                <option value="newPage">{t("writer.startNewPage")}</option>
                <option value="continuous">{t("writer.startContinuous")}</option>
                <option value="oddPage">{t("writer.startOdd")}</option>
                <option value="evenPage">{t("writer.startEven")}</option>
              </select>
            </div>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
