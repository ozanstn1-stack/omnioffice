import { useEffect } from "react";
import { Download, X } from "lucide-react";
import { Button, Card, IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { updateOpen } from "../lib/api";
import { isAndroid } from "../lib/mobile";
import { reportError, useSettings } from "../lib/store";
import { shouldAnnounce, useUpdate } from "../lib/update";

/**
 * "A new version is available" banner on Home. Runs the weekly automatic
 * check once settings are loaded and shows the result until the user closes
 * it for that version. The primary button installs in-app (Tauri updater on
 * desktop, the APK downloader on Android); when the build has no updater
 * configuration the store falls back to opening the release download in the
 * browser and the banner says so. The GitHub release page carries the notes.
 */
export function UpdateBanner() {
  const t = useT();
  const settings = useSettings((state) => state.settings);
  const loaded = useSettings((state) => state.loaded);
  const info = useUpdate((state) => state.info);
  const check = useUpdate((state) => state.check);
  const dismiss = useUpdate((state) => state.dismiss);
  const install = useUpdate((state) => state.install);
  const relaunch = useUpdate((state) => state.relaunch);
  const phase = useUpdate((state) => state.phase);
  const progress = useUpdate((state) => state.progress);
  const mode = useUpdate((state) => state.mode);

  useEffect(() => {
    if (loaded) void check(false);
  }, [loaded, check]);

  if (!shouldAnnounce(info, settings)) return null;

  const open = (url: string) => void updateOpen(url).catch((error: unknown) => reportError(error, t));
  const android = isAndroid();
  const canInstall = !android || Boolean(info.downloadUrl);
  const busy = phase === "downloading" || phase === "installing";
  const useFallback = mode === "fallback" || !canInstall;

  const primary = () => {
    if (phase === "ready") {
      return (
        <>
          <span className="text-[13.5px]">{t("update.installReady", { version: info.latest })}</span>
          {android ? null : (
            <Button
              variant="primary"
              size="sm"
              onClick={() => void relaunch().catch((error: unknown) => reportError(error, t))}
            >
              {t("update.restart")}
            </Button>
          )}
        </>
      );
    }
    if (busy) {
      return (
        <Button variant="primary" size="sm" disabled>
          {phase === "downloading" ? t("update.downloading", { percent: progress }) : t("update.installing")}
        </Button>
      );
    }
    if (useFallback) {
      return (
        <Button variant="primary" size="sm" onClick={() => open(info.downloadUrl ?? info.releaseUrl)}>
          {info.downloadUrl ? t("update.download") : t("update.openRelease")}
        </Button>
      );
    }
    return (
      <Button variant="primary" size="sm" onClick={() => void install()}>
        {t("update.install")}
      </Button>
    );
  };

  const hint =
    mode === "fallback"
      ? t("update.fallbackHint")
      : !busy && phase !== "ready" && !useFallback
        ? t("update.autoHint")
        : null;

  return (
    <Card className="p-4 fade-in">
      <div className="flex items-center gap-3 flex-wrap" role="status">
        <Download size={18} style={{ color: "var(--accent)" }} aria-hidden />
        <p className="text-[13.5px] flex-1 min-w-[12rem]">
          <strong>{t("update.available", { version: info.latest })}</strong>{" "}
          <span className="muted">{t("update.current", { version: info.current })}</span>
        </p>
        {primary()}
        {info.downloadUrl ? (
          <Button variant="ghost" size="sm" onClick={() => open(info.releaseUrl)}>
            {t("update.whatsNew")}
          </Button>
        ) : null}
        <IconButton label={t("common.close")} onClick={() => void dismiss()}>
          <X size={16} />
        </IconButton>
      </div>
      {hint ? <p className="text-xs muted mt-2">{hint}</p> : null}
    </Card>
  );
}
