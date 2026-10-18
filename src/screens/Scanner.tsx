import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";
import { Camera, FileText, Save, ScanText } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { Screen } from "../components/layout";
import { uid } from "../lib/format";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { isAndroid, pickAndroidFiles, pickAndroidSaveTarget, publishOutputs } from "../lib/mobile";
import { scanPages } from "../lib/office-api";
import type { ScreenId } from "../lib/nav";

interface ScannerProps {
  onNavigate?: (screen: ScreenId, options?: { files?: string[] }) => void;
}

/**
 * Document scanner.
 *
 * Supports direct camera capture (`capture="environment"`) and gallery/file
 * picking on both Android and desktop, perspective correction + multi-page PDF
 * assembly in Rust (`office_scan_pages`), Android SAF save publishing, and
 * one-click handoff to the OCR screen.
 */
export function Scanner({ onNavigate }: ScannerProps = {}) {
  const t = useT();
  const [pages, setPages] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [lastOutput, setLastOutput] = useState<string | null>(null);
  const cameraInputRef = useRef<HTMLInputElement | null>(null);

  const pick = async () => {
    let paths: string[] = [];
    if (isAndroid()) {
      paths = await pickAndroidFiles({ multiple: true, accept: "image" }).catch(() => []);
    } else {
      const picked = await openDialog({
        multiple: true,
        filters: [{ name: t("scanner.filterName"), extensions: ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"] }],
      });
      paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    }
    setPages((current) => [...current, ...paths.filter((path) => !current.includes(path))]);
  };

  const handleCameraFiles = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    setBusy(true);
    try {
      const cache = await appCacheDir();
      const directory = await join(cache, "scans", uid("scan"));
      await invoke("ensure_dir", { path: directory });
      const staged: string[] = [];
      for (const file of Array.from(fileList)) {
        const cleanName = (file.name || `sayfa-${staged.length + 1}.jpg`).replace(/[\\/:*?"<>|]/g, "_");
        const dest = await join(directory, cleanName);
        const bytes = new Uint8Array(await file.arrayBuffer());
        await writeFile(dest, bytes);
        staged.push(dest);
      }
      setPages((current) => [...current, ...staged]);
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
      if (cameraInputRef.current) cameraInputRef.current.value = "";
    }
  };

  const build = async () => {
    if (!pages.length) return;
    let outputPath: string | null = null;
    let androidTarget: Awaited<ReturnType<typeof pickAndroidSaveTarget>> = null;

    if (isAndroid()) {
      androidTarget = await pickAndroidSaveTarget("Tarama.pdf", "application/pdf");
      if (!androidTarget) return;
      const cache = await appCacheDir();
      outputPath = await join(cache, androidTarget.name || "Tarama.pdf");
    } else {
      const output = await saveDialog({
        defaultPath: "Tarama.pdf",
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (typeof output !== "string") return;
      outputPath = output;
    }

    setBusy(true);
    try {
      const result = await scanPages(
        pages.map((path) => ({ path })),
        outputPath,
      );
      if (isAndroid() && androidTarget) {
        await publishOutputs([outputPath], { file: androidTarget });
      }
      setLastOutput(outputPath);
      useToasts.getState().push({
        kind: "success",
        title: t("scanner.done", { pages: result.pages }),
        detail: androidTarget ? androidTarget.name : outputPath,
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
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2 flex-wrap">
              <Camera size={16} style={{ color: "var(--accent)" }} />
              <strong>{t("scanner.cameraTitle")}</strong>
              <Badge tone="accent">Hazır</Badge>
            </div>
            <input
              ref={cameraInputRef}
              type="file"
              accept="image/*"
              capture="environment"
              multiple
              className="hidden"
              onChange={(e) => void handleCameraFiles(e.target.files)}
            />
            <Button variant="primary" size="sm" disabled={busy} onClick={() => cameraInputRef.current?.click()}>
              <Camera size={14} /> Kameradan Fotoğraf Çek
            </Button>
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
            {lastOutput && onNavigate && (
              <Button variant="ghost" disabled={busy} onClick={() => onNavigate("ocr", { files: [lastOutput] })}>
                <ScanText size={16} /> OCR&apos;a Gönder
              </Button>
            )}
            <span className="text-xs muted">{t("scanner.pickHint")}</span>
          </div>
          {pages.length ? (
            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between text-xs muted">
                <span>{pages.length} sayfa eklendi</span>
                <Button size="sm" variant="ghost" onClick={() => setPages([])}>
                  Tümünü Temizle
                </Button>
              </div>
              <ul className="flex flex-col gap-1 text-sm">
                {pages.map((path, index) => (
                  <li
                    key={path}
                    className="flex items-center justify-between gap-2 p-1.5 rounded bg-surface/50 border border-border/40"
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="muted font-mono text-xs">{index + 1}.</span>
                      <span className="truncate">{path.split(/[\\/]/).pop()}</span>
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={index === 0}
                        onClick={() => {
                          const next = [...pages];
                          const temp = next[index - 1];
                          next[index - 1] = next[index];
                          next[index] = temp;
                          setPages(next);
                        }}
                      >
                        ↑
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={index === pages.length - 1}
                        onClick={() => {
                          const next = [...pages];
                          const temp = next[index + 1];
                          next[index + 1] = next[index];
                          next[index] = temp;
                          setPages(next);
                        }}
                      >
                        ↓
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setPages((current) => current.filter((item) => item !== path))}
                      >
                        ×
                      </Button>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          ) : (
            <p className="muted text-sm">{t("scanner.empty")}</p>
          )}
          <p className="text-xs muted">{t("scanner.ocrNote")}</p>
        </Card>
      </div>
    </Screen>
  );
}
