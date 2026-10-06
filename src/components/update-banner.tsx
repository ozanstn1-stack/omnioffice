import { useEffect } from "react";
import { Download, X } from "lucide-react";
import { Button, Card, IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { updateOpen } from "../lib/api";
import { reportError, useSettings } from "../lib/store";
import { shouldAnnounce, useUpdate } from "../lib/update";

/**
 * "A new version is available" banner on Home. Runs the weekly automatic
 * check once settings are loaded and shows the result until the user closes
 * it for that version. Download opens the installer (Windows) or APK
 * (Android) for this device in the browser; the GitHub release page carries
 * the notes.
 */
export function UpdateBanner() {
  const t = useT();
  const settings = useSettings((state) => state.settings);
  const loaded = useSettings((state) => state.loaded);
  const info = useUpdate((state) => state.info);
  const check = useUpdate((state) => state.check);
  const dismiss = useUpdate((state) => state.dismiss);

  useEffect(() => {
    if (loaded) void check(false);
  }, [loaded, check]);

  if (!shouldAnnounce(info, settings)) return null;

  const open = (url: string) => void updateOpen(url).catch((error: unknown) => reportError(error, t));

  return (
    <Card className="p-4 fade-in">
      <div className="flex items-center gap-3 flex-wrap" role="status">
        <Download size={18} style={{ color: "var(--accent)" }} aria-hidden />
        <p className="text-[13.5px] flex-1 min-w-[12rem]">
          <strong>{t("update.available", { version: info.latest })}</strong>{" "}
          <span className="muted">{t("update.current", { version: info.current })}</span>
        </p>
        <Button variant="primary" size="sm" onClick={() => open(info.downloadUrl ?? info.releaseUrl)}>
          {info.downloadUrl ? t("update.download") : t("update.openRelease")}
        </Button>
        {info.downloadUrl ? (
          <Button variant="ghost" size="sm" onClick={() => open(info.releaseUrl)}>
            {t("update.whatsNew")}
          </Button>
        ) : null}
        <IconButton label={t("common.close")} onClick={() => void dismiss()}>
          <X size={16} />
        </IconButton>
      </div>
    </Card>
  );
}
