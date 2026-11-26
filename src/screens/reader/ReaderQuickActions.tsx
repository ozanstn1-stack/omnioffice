import { PenLine, SlidersHorizontal } from "lucide-react";
import { IconButton } from "../../components/ui";
import { useT } from "../../lib/i18n";
import type { Navigate } from "../../lib/nav";

/** One-click handoff buttons from the PDF Reader toolbar to Annotate and PDF Studio. */
export function ReaderQuickActions({ path, onNavigate }: { path: string; onNavigate?: Navigate }) {
  const t = useT();
  if (!onNavigate) return null;
  return (
    <>
      <IconButton label={t("reader.openInAnnotate")} onClick={() => onNavigate("annotate", { files: [path] })}>
        <PenLine size={15} />
      </IconButton>
      <IconButton label={t("reader.openInStudio")} onClick={() => onNavigate("pdfStudio", { files: [path] })}>
        <SlidersHorizontal size={15} />
      </IconButton>
    </>
  );
}

export default ReaderQuickActions;
