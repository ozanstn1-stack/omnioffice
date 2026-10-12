import { useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Camera, FileText, Save } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { isAndroid, pickOfficeFiles } from "../lib/mobile";
import { scanPages } from "../lib/office-api";

/**
 * Document scanner.
 *
 * The Android camera capture (ML Kit's document scanner, or CameraX with a
 * manual four-corner pick on devices without Google Play services) is not
 * implemented yet; this screen is its scaffold. It already performs the Rust
 * half — perspective correction and multi-page PDF assembly — on page photos
 * the user picks, so the flow can be exercised today and the capture plugs into
 * the same command.
 */
export function Scanner() {
  const t = useT();
  const [pages, setPages] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  const pick = async () => {
    let paths: string[] = [];
    if (isAndroid()) {
      paths = await pickOfficeFiles(true).catch(() => []);
    } else {
      const picked = await openDialog({
        multiple: true,
        filters: [{ name: t("scanner.filterName"), extensions: ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"] }],
      });
      paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    }
    setPages((current) => [...current, ...paths.filter((path) => !current.includes(path))]);
  };

  const build = async () => {
    if (!pages.length) return;
    const output = await saveDialog({
      defaultPath: "Tarama.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;
    setBusy(true);
    try {
      const result = await scanPages(
        pages.map((path) => ({ path })),
        output,
      );
      useToasts.getState().push({
        kind: "success",
        title: t("scanner.done", { pages: result.pages }),
        detail: output,
      });
      for (const warning of result.warnings) {
        useToasts.getState().push({ kind: "info", title: t("scanner.warnings"), detail: warning });
      }
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen title={t("scanner.title")} subtitle={t("scanner.subtitle")}>
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <Camera size={16} style={{ color: "var(--accent)" }} />
            <strong>{t("scanner.cameraTitle")}</strong>
            <Badge tone="warn">{t("scanner.pending")}</Badge>
          </div>
          <p className="text-sm muted">{t("scanner.cameraNote")}</p>
          <p className="text-xs muted">{t("scanner.permissionNote")}</p>
        </Card>

        <Card className="p-5 flex flex-col gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <Button variant="primary" disabled={busy} onClick={() => void pick()}>
              <FileText size={16} /> {t("scanner.pick")}
            </Button>
            <Button variant="ghost" disabled={busy || !pages.length} onClick={() => void build()}>
              <Save size={16} /> {t("scanner.build")}
            </Button>
            <span className="text-xs muted">{t("scanner.pickHint")}</span>
          </div>
          {pages.length ? (
            <ul className="flex flex-col gap-1 text-sm">
              {pages.map((path, index) => (
                <li key={path} className="flex items-center gap-2">
                  <span className="muted">{index + 1}.</span>
                  <span className="truncate">{path.split(/[\\/]/).pop()}</span>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setPages((current) => current.filter((item) => item !== path))}
                  >
                    ×
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted text-sm">{t("scanner.empty")}</p>
          )}
          <p className="text-xs muted">{t("scanner.ocrNote")}</p>
        </Card>
      </div>
    </Screen>
  );
}
