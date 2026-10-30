import { useRef, useState } from "react";
import { call } from "../lib/typed-call";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { pickSaveToken } from "../lib/file-tokens";
import { writeFile } from "@tauri-apps/plugin-fs";
import { Camera, FileText, Save, ScanText } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { Screen } from "../components/layout";
import { uid } from "../lib/format";
import { pickDocumentTokens } from "../lib/file-tokens";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { isAndroid, pickAndroidFiles, pickAndroidSaveTarget, publishOutputs } from "../lib/mobile";
import { detectedCorners, scanDetect, scanPages, type ScanFilter } from "../lib/scan-detect";
import type { ScreenId } from "../lib/nav";
import { CornerPicker } from "./scanner-corners";
import { ScannerPageList, type ScannerPage } from "./scanner-page-list";

interface ScannerProps {
  onNavigate?: (screen: ScreenId, options?: { files?: string[] }) => void;
}

/**
 * Document scanner.
 *
 * Camera: the hidden `<input capture="environment">` opens the system camera
 * app (Android asks for the camera permission the first time); each photo is
 * one page. Gallery/file picking works on Android and desktop. Every page is
 * auto-cropped: `office_scan_detect` finds the four corners on the device
 * (Canny edges, Hough lines, scored quadrilateral) and the corner picker
 * (`scanner-corners.tsx`) stays for correcting a page it could not find. The
 * chosen filter (colour, grey or black and white) is applied while Rust turns
 * each page upright (EXIF), flattens it and assembles the PDF
 * (`office_scan_pages`), which can go straight to OCR. No Google Play services
 * are involved: ML Kit's scanner would bring its usage logging along.
 */
export function Scanner({ onNavigate }: ScannerProps = {}) {
  const t = useT();
  const [pages, setPages] = useState<ScannerPage[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [filter, setFilter] = useState<ScanFilter>("colour");
  const [lastOutput, setLastOutput] = useState<string | null>(null);
  const cameraInputRef = useRef<HTMLInputElement | null>(null);

  /** Adds pages and runs the on-device detector for each new photo. */
  const addPaths = async (paths: string[]) => {
    const unique = [...new Set(paths.filter(Boolean))];
    if (!unique.length) return;
    setPages((current) => [
      ...current,
      ...unique.filter((path) => !current.some((page) => page.path === path)).map((path) => ({ path })),
    ]);
    setDetecting(true);
    try {
      const found = await Promise.all(
        unique.map(async (path) => [path, await scanDetect(path).catch(() => null)] as const),
      );
      setPages((current) =>
        current.map((page) => {
          const match = found.find(([path]) => path === page.path);
          const corners = detectedCorners(match?.[1]);
          // A page the user already corrected keeps its corners.
          return corners && !page.corners ? { ...page, corners, auto: true } : page;
        }),
      );
    } finally {
      setDetecting(false);
    }
  };

  const pick = async () => {
    let paths: string[] = [];
    if (isAndroid()) {
      paths = await pickAndroidFiles({ multiple: true, accept: "image" }).catch(() => []);
    } else {
      paths = (
        await pickDocumentTokens(["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"], true).catch(() => [])
      ).map((file) => file.token);
    }
    await addPaths(paths);
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
      // Batch capture: each shot is one page; the detector crops it without
      // opening the picker, so the next shot can follow immediately.
      await addPaths(staged);
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
      const request = pages.map((page) => ({
        path: page.path,
        filter,
        ...(page.corners ? { corners: page.corners } : {}),
      }));
      const result = await scanPages(request, outputPath);
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
            <Button variant="ghost" disabled={busy || detecting || !pages.length} onClick={() => void build()}>
              <Save size={16} /> {t("scanner.build")}
            </Button>
            {lastOutput && onNavigate && (
              <Button variant="ghost" disabled={busy} onClick={() => onNavigate("ocr", { files: [lastOutput] })}>
                <ScanText size={16} /> {t("scanner.sendToOcr")}
              </Button>
            )}
            <span className="text-xs muted">{t("scanner.pickHint")}</span>
          </div>
          <ScannerPageList
            pages={pages}
            busy={busy}
            detecting={detecting}
            filter={filter}
            onFilter={setFilter}
            onMove={move}
            onRemove={(path) => setPages((current) => current.filter((page) => page.path !== path))}
            onAdjust={setEditing}
            onClear={() => setPages([])}
          />
          <p className="text-xs muted">{t("scanner.autoNote")}</p>
          <p className="text-xs muted">{t("scanner.ocrNote")}</p>
        </Card>
      </div>
      {editingPage ? (
        <CornerPicker
          key={editingPage.path}
          name={editingPage.path.split(/[\\/]/).pop() || editingPage.path}
          path={editingPage.path}
          initial={editingPage.corners}
          onSave={(corners) => setCorners(editingPage.path, corners)}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </Screen>
  );
}
