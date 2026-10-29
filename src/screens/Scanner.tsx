import { useRef, useState } from "react";
import { call } from "../lib/typed-call";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { pickSaveToken } from "../lib/file-tokens";
import { writeFile } from "@tauri-apps/plugin-fs";
import { Camera, Crop, FileText, Save, ScanText } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { Screen } from "../components/layout";
import { fileBaseName, uid } from "../lib/format";
import { pickDocumentTokens } from "../lib/file-tokens";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { isAndroid, pickAndroidFiles, pickAndroidSaveTarget, publishOutputs } from "../lib/mobile";
import { scanPages, type ScanPage } from "../lib/office-api";
import type { ScreenId } from "../lib/nav";
import { CornerPicker } from "./scanner-corners";

interface ScannerProps {
  onNavigate?: (screen: ScreenId, options?: { files?: string[] }) => void;
}

/**
 * Document scanner.
 *
 * Camera: the hidden `<input capture="environment">` opens the system camera
 * app (Android asks for the camera permission the first time); each photo is
 * one page. Gallery/file picking works on Android and desktop. A page's four
 * corners can be set in the corner picker (`scanner-corners.tsx`); Rust turns
 * every page upright (EXIF), flattens the cropped ones and assembles the PDF
 * (`office_scan_pages`), which can go straight to OCR. No Google Play services
 * are involved: ML Kit's scanner would bring its usage logging along.
 */
export function Scanner({ onNavigate }: ScannerProps = {}) {
  const t = useT();
  const [pages, setPages] = useState<ScanPage[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [lastOutput, setLastOutput] = useState<string | null>(null);
  const cameraInputRef = useRef<HTMLInputElement | null>(null);

  const addPaths = (paths: string[]) =>
    setPages((current) => [
      ...current,
      ...paths.filter((path) => !current.some((page) => page.path === path)).map((path) => ({ path })),
    ]);

  const pick = async () => {
    let paths: string[] = [];
    if (isAndroid()) {
      paths = await pickAndroidFiles({ multiple: true, accept: "image" }).catch(() => []);
    } else {
      paths = (
        await pickDocumentTokens(["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"], true).catch(() => [])
      ).map((file) => file.token);
    }
    addPaths(paths);
  };

  const handleCameraFiles = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    setBusy(true);
    try {
      const cache = await appCacheDir();
      const directory = await join(cache, "scans", uid("scan"));
      await call((c) => c.ensureDir(directory));
      const staged: string[] = [];
      for (const file of Array.from(fileList)) {
        const cleanName = (file.name || `page-${staged.length + 1}.jpg`).replace(/[\\/:*?"<>|]/g, "_");
        const dest = await join(directory, cleanName);
        const bytes = new Uint8Array(await file.arrayBuffer());
        await writeFile(dest, bytes);
        staged.push(dest);
      }
      addPaths(staged);
      // A fresh camera shot nearly always needs cropping: open the picker.
      if (staged.length === 1) setEditing(staged[0]);
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
      if (cameraInputRef.current) cameraInputRef.current.value = "";
    }
  };

  const move = (index: number, by: number) =>
    setPages((current) => {
      const next = [...current];
      const [page] = next.splice(index, 1);
      next.splice(index + by, 0, page);
      return next;
    });

  const setCorners = (path: string, corners: [number, number][] | undefined) => {
    setPages((current) =>
      current.map((page) => (page.path === path ? (corners ? { path, corners } : { path }) : page)),
    );
    setEditing(null);
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
      const target = await pickSaveToken("Tarama.pdf", ["pdf"]).catch(() => null);
      if (!target) return;
      outputPath = target.token;
    }

    setBusy(true);
    try {
      const result = await scanPages(pages, outputPath);
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

  const editingPage = editing ? pages.find((page) => page.path === editing) : undefined;

  return (
    <Screen
      title={
        <span className="flex items-center gap-2">
          {t("scanner.title")}
          <Badge tone="warn">{t("common.experimental")}</Badge>
        </span>
      }
      subtitle={t("scanner.subtitle")}
    >
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2 flex-wrap">
              <Camera size={16} style={{ color: "var(--accent)" }} />
              <strong>{t("scanner.cameraTitle")}</strong>
            </div>
            <input
              ref={cameraInputRef}
              type="file"
              accept="image/*"
              capture="environment"
              className="hidden"
              onChange={(e) => void handleCameraFiles(e.target.files)}
            />
            <Button variant="primary" size="sm" disabled={busy} onClick={() => cameraInputRef.current?.click()}>
              <Camera size={14} /> {t("scanner.cameraButton")}
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
                <ScanText size={16} /> {t("scanner.sendToOcr")}
              </Button>
            )}
            <span className="text-xs muted">{t("scanner.pickHint")}</span>
          </div>
          {pages.length ? (
            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between text-xs muted">
                <span>{t("scanner.pageCount", { count: pages.length })}</span>
                <Button size="sm" variant="ghost" onClick={() => setPages([])}>
                  {t("scanner.clearAll")}
                </Button>
              </div>
              <ul className="flex flex-col gap-1 text-sm">
                {pages.map((page, index) => (
                  <li
                    key={page.path}
                    className="flex items-center justify-between gap-2 p-1.5 rounded bg-surface/50 border border-border/40"
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="muted font-mono text-xs">{index + 1}.</span>
                      <span className="truncate">{fileBaseName(page.path)}</span>
                      {page.corners ? <Badge tone="accent">{t("scanner.cropped")}</Badge> : null}
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <Button size="sm" variant="ghost" disabled={busy} onClick={() => setEditing(page.path)}>
                        <Crop size={14} /> {t("scanner.adjust")}
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={t("common.moveUp")}
                        disabled={index === 0}
                        onClick={() => move(index, -1)}
                      >
                        ↑
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={t("common.moveDown")}
                        disabled={index === pages.length - 1}
                        onClick={() => move(index, 1)}
                      >
                        ↓
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={t("common.remove")}
                        onClick={() => setPages((current) => current.filter((item) => item.path !== page.path))}
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
      {editingPage ? (
        <CornerPicker
          key={editingPage.path}
          name={fileBaseName(editingPage.path)}
          path={editingPage.path}
          initial={editingPage.corners}
          onSave={(corners) => setCorners(editingPage.path, corners)}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </Screen>
  );
}
