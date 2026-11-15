import { InfoStrip } from "./files";
import { useT } from "../lib/i18n";

/** A document that failed to load says why, instead of an empty page and "1 / 0". */
export function LoadError({ loaded, error }: { loaded: boolean; error?: string | null }) {
  const t = useT();
  if (loaded || !error) return null;
  return (
    <div role="alert" data-testid="reader-load-error" className="card-soft p-4">
      <InfoStrip info={null} error={`${t("errors.openFailed")} ${error}`} />
    </div>
  );
}
