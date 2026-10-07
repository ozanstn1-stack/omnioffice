/**
 * Shared save/open/export/version-history logic for the office editors.
 * Keeps the editors focused on editing while this hook deals with files.
 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { fileBaseName, uid } from "../lib/format";
import {
  isAndroid,
  openAnyFile,
  pickAndroidSaveTarget,
  pickOfficeFiles,
  publishOutputs,
  updatePublishedOutput,
  type AndroidTarget,
} from "../lib/mobile";
import type { OfficeKind } from "../lib/office-types";
import { openOfficePath, rememberOfficePath, useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { useSettings, useToasts, reportError } from "../lib/store";
import { useT } from "../lib/i18n";
import * as api from "../lib/office-api";
import { historyKeyFor } from "./historyKey";
import { compatibilityReport, gatingLossItems, type CompatibilityReport } from "../components/compatibility";
import { useDataLossPrompt } from "../components/data-loss-dialog";
import { useFileConflictPrompt, type ConflictDetails } from "../components/file-conflict-dialog";

/** Shortens a hash for display without pretending it is the full digest. */
function shortHash(hash: string): string {
  return hash ? `${hash.slice(0, 12)}…` : "";
}

/**
 * Returns conflict details when the file on disk no longer matches the
 * fingerprint captured at open/save, or null when it is unchanged (or the
 * check itself failed, in which case the write proceeds rather than trapping
 * the user's data).
 */
async function detectExternalChange(path: string, openedFingerprint: string): Promise<ConflictDetails | null> {
  try {
    const current = await api.fileFingerprint(path);
    if (!current.exists) {
      return { name: fileBaseName(path), openedHash: shortHash(openedFingerprint), currentHash: "", missing: true };
    }
    if (current.sha256 === openedFingerprint) return null;
    return {
      name: fileBaseName(path),
      openedHash: shortHash(openedFingerprint),
      currentHash: shortHash(current.sha256),
      missing: false,
    };
  } catch {
    return null;
  }
}

const FILTERS: Record<OfficeKind, { name: string; extensions: string[] }[]> = {
  writer: [
    { name: "Word document", extensions: ["docx"] },
    { name: "OpenDocument text", extensions: ["odt"] },
    { name: "Rich text", extensions: ["rtf"] },
    { name: "Plain text", extensions: ["txt"] },
    { name: "Markdown", extensions: ["md"] },
    { name: "Web page", extensions: ["html"] },
    { name: "PDF", extensions: ["pdf"] },
    { name: "OmniOffice document", extensions: ["oswk"] },
  ],
  calc: [
    { name: "Excel workbook", extensions: ["xlsx"] },
    { name: "OpenDocument spreadsheet", extensions: ["ods"] },
    { name: "CSV", extensions: ["csv"] },
    { name: "PDF", extensions: ["pdf"] },
    { name: "OmniOffice spreadsheet", extensions: ["oswk"] },
  ],
  impress: [
    { name: "PowerPoint presentation", extensions: ["pptx"] },
    { name: "OpenDocument presentation", extensions: ["odp"] },
    { name: "PDF", extensions: ["pdf"] },
    { name: "OmniOffice presentation", extensions: ["oswk"] },
  ],
};

export function extensionOf(path: string): string {
  const match = /\.([a-z0-9]+)$/i.exec(path);
  return match ? match[1].toLowerCase() : "";
}

/** Replaces the last extension, so the lossless `.oswk` sibling can be offered. */
export function replaceExtension(path: string, extension: string): string {
  const match = /\.[a-z0-9]+$/i.exec(path);
  return match ? `${path.slice(0, match.index)}.${extension}` : `${path}.${extension}`;
}

const DEFAULT_EXTENSION: Record<OfficeKind, string> = { writer: "docx", calc: "xlsx", impress: "pptx" };

/**
 * The extension the save dialog suggests for a new document. The choice comes
 * from Settings ▸ Office defaults; the built-in pair stays the fallback.
 */
function defaultExtensionFor(kind: OfficeKind): string {
  const settings = useSettings.getState().settings;
  const configured =
    kind === "writer"
      ? settings.defaultWriterFormat
      : kind === "calc"
        ? settings.defaultCalcFormat
        : settings.defaultImpressFormat;
  return configured || DEFAULT_EXTENSION[kind];
}

/** Suggests a file name for the save dialog when the tab has never been saved. */
function suggestedName(tab: OfficeTab, extension: string): string {
  const stem = (tab.title || "Untitled").replace(/[\\/:*?"<>|]/g, "-").trim() || "Untitled";
  return `${stem}.${extension}`;
}

/**
 * Android only: real path inside the app cache where the engine can write
 * before the result is published through the system save dialog.
 */
async function scratchPath(extension: string): Promise<string> {
  const cache = await appCacheDir();
  const directory = await join(cache, "office");
  await invoke("ensure_dir", { path: directory });
  return join(directory, `${uid("office")}.${extension}`);
}

export function useOfficeSession(tab: OfficeTab) {
  const t = useT();
  const markSaved = useOfficeTabs((state) => state.markSaved);
  const [busy, setBusy] = useState(false);

  const notify = useCallback((title: string, detail?: string) => {
    useToasts.getState().push({ kind: "success", title, detail });
  }, []);

  /**
   * Runs the backend feature report for `target`. Returns null when the check
   * itself fails: a broken compatibility check must never trap the user's
   * data, so the caller fails open and the write continues with a note.
   */
  const loadCompatibility = useCallback(
    async (target: string): Promise<CompatibilityReport | null> => {
      try {
        return await compatibilityReport(tab.kind, tab.model, target);
      } catch {
        useToasts.getState().push({
          kind: "info",
          title: t("office.compatCheckFailed"),
          detail: t("office.compatCheckFailedHint"),
        });
        return null;
      }
    },
    [t, tab.kind, tab.model],
  );

  /** Copies the saved document to its Android destination, or explains why not. */
  const publishAndroidResult = useCallback(
    async (sourcePath: string, target: AndroidTarget | null, name: string): Promise<void> => {
      if (target) {
        const failures = await publishOutputs([sourcePath], { file: target });
        if (failures.length) {
          useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: failures[0].error });
        } else {
          notify(t("office.saved"), name);
        }
        return;
      }
      // Later saves reuse the destination picked the first time; if this install
      // no longer remembers it (fresh process), fall back to a new export.
      if (await updatePublishedOutput(sourcePath)) {
        notify(t("office.saved"), fileBaseName(sourcePath));
        return;
      }
      const failures = await publishOutputs([sourcePath]);
      if (failures.length) {
        useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: failures[0].error });
      } else {
        notify(t("office.saved"), fileBaseName(sourcePath));
      }
    },
    [notify, t],
  );

  const save = useCallback(
    async function saveImpl(
      targetPath?: string,
      options?: { forceDestination?: boolean; defaultPath?: string; extension?: string },
    ): Promise<string | null> {
      // Ctrl+S keeps the current path; `saveAs` forces a fresh destination.
      const forceDestination = options?.forceDestination ?? false;
      let path = targetPath ?? (forceDestination ? undefined : tab.path) ?? undefined;
      let androidTarget: AndroidTarget | null = null;
      if (!path) {
        if (isAndroid()) {
          // Android never hands out a writable path for the user's documents:
          // pick the destination first, let the engine write into the app
          // cache, then copy the finished file over.
          const extension = options?.extension ?? defaultExtensionFor(tab.kind);
          androidTarget = await pickAndroidSaveTarget(suggestedName(tab, extension));
          if (!androidTarget) return null;
          path = await scratchPath(extension);
        } else {
          path =
            (await saveDialog({
              title: `Save ${tab.title}`,
              defaultPath:
                options?.defaultPath ?? suggestedName(tab, options?.extension ?? defaultExtensionFor(tab.kind)),
              filters: FILTERS[tab.kind],
            })) ?? undefined;
          if (!path) return null;
        }
      }
      setBusy(true);
      try {
        const extension = extensionOf(path);
        // External-change guard: only for a path that already existed and was
        // fingerprinted at open/save. A brand-new target (fingerprint null) or
        // an unchanged file continues silently.
        if (!androidTarget && tab.path && tab.path === path && tab.fingerprint) {
          const conflict = await detectExternalChange(path, tab.fingerprint);
          if (conflict) {
            setBusy(false);
            const choice = await useFileConflictPrompt.getState().ask(conflict);
            if (choice === "cancel") return null;
            if (choice === "saveAs") {
              // Re-open the save dialog seeded with a new name beside the file.
              return saveImpl(undefined, { forceDestination: true, extension });
            }
            // "reload": adopt the on-disk file, discarding the in-app edits.
            const reloaded = await openOfficePath(path);
            if (reloaded.ok) {
              useToasts.getState().push({ kind: "info", title: t("conflict.reload"), detail: fileBaseName(path) });
            }
            return path;
          }
          setBusy(true);
        }
        // The native unit format carries everything the suite understands, so it
        // is the one target where a save is a full snapshot. Every other format
        // can drop features, which the engine reports through `warnings`.
        const lossless = extension === "oswk";
        if (!lossless) {
          // Data Loss Protection: ask the backend what this document would lose
          // before any byte is written, and never silently drop data. A report
          // that cannot be loaded fails open (the write continues with a note).
          const report = await loadCompatibility(extension);
          if (report && gatingLossItems(extension, report).length > 0) {
            const choice = await useDataLossPrompt.getState().ask(extension, report);
            if (choice === "cancel") return null;
            if (choice === "oswk") {
              // Close the warning and run the real lossless flow: a fresh save
              // dialog seeded with the lossless sibling of the chosen target.
              return saveImpl(undefined, {
                forceDestination: true,
                extension: "oswk",
                defaultPath: replaceExtension(path, "oswk"),
              });
            }
          }
        }
        const result = lossless
          ? await api.saveUnit(tab.kind, tab.title, tab.model, path)
          : await api.saveDocument(tab.kind, tab.model, path);
        // Re-fingerprint the file we just wrote so the next save compares
        // against our own bytes, not the pre-save version.
        let savedFingerprint: string | null = null;
        try {
          if (!androidTarget) {
            const fingerprint = await api.fileFingerprint(result.path);
            savedFingerprint = fingerprint.exists ? fingerprint.sha256 : null;
          }
        } catch {
          savedFingerprint = null;
        }
        markSaved(tab.id, result.path, savedFingerprint);
        rememberOfficePath(result.path);
        if (isAndroid()) {
          await publishAndroidResult(result.path, androidTarget, androidTarget?.name ?? fileBaseName(path));
          if (result.warnings.length > 0) {
            useToasts
              .getState()
              .push({ kind: "info", title: t("office.savedWithNotes"), detail: result.warnings.join(" ") });
          }
        } else if (result.warnings.length > 0) {
          useToasts
            .getState()
            .push({ kind: "info", title: t("office.savedWithNotes"), detail: result.warnings.join(" ") });
        } else {
          notify(t("office.saved"), result.path);
        }
        if (useSettings.getState().settings.versionHistory) {
          // Keyed by the path just written, so the history survives reopening
          // the file (the tab id changes every time it is opened).
          void api
            .historyPush(historyKeyFor({ id: tab.id, path: result.path }), tab.kind, tab.title, tab.model)
            .catch(() => undefined);
        }
        if (!lossless && result.warnings.length > 0) {
          // A lossy export may have dropped something the user cares about, so
          // the recovery snapshot stays on disk until the next clean save.
          useToasts
            .getState()
            .push({ kind: "info", title: t("office.recoveryKept"), detail: t("office.recoveryKeptHint") });
          return result.path;
        }
        void api.recoveryDiscard(tab.id).catch(() => undefined);
        return result.path;
      } catch (error) {
        reportError(error, t);
        return null;
      } finally {
        setBusy(false);
      }
    },
    [loadCompatibility, markSaved, notify, publishAndroidResult, t, tab],
  );

  /** Always asks for a destination, even when the tab already has a path. */
  const saveAs = useCallback(async (): Promise<string | null> => {
    if (isAndroid()) {
      // Android picks its destination inside `save` (the engine writes a cache
      // copy first, then the file is published through the system dialog).
      return save(undefined, { forceDestination: true });
    }
    const chosen = (await saveDialog({
      title: `Save ${tab.title} as`,
      defaultPath: tab.path ?? suggestedName(tab, extensionOf(tab.path ?? "") || defaultExtensionFor(tab.kind)),
      filters: FILTERS[tab.kind],
    })) as string | null;
    if (!chosen) return null;
    return save(chosen);
  }, [save, tab]);

  const exportPdf = useCallback(async (): Promise<string | null> => {
    let path: string | undefined;
    let androidTarget: AndroidTarget | null = null;
    if (isAndroid()) {
      androidTarget = await pickAndroidSaveTarget(`${tab.title}.pdf`, "application/pdf");
      if (!androidTarget) return null;
      path = await scratchPath("pdf");
    } else {
      path =
        ((await saveDialog({
          title: `Export ${tab.title} as PDF`,
          defaultPath: `${tab.title}.pdf`,
          filters: [{ name: "PDF", extensions: ["pdf"] }],
        })) as string | null) ?? undefined;
      if (!path) return null;
    }
    setBusy(true);
    try {
      // PDF is a rendering target, not a document container: the backend's pdf
      // report describes how features render (e.g. slide animations do not
      // apply), never container loss, because the export writes a side file and
      // never replaces the model. Only the actual rows it returns gate here,
      // and a container-loss `format` row would be ignored (gatingLossItems).
      const report = await loadCompatibility("pdf");
      if (report && gatingLossItems("pdf", report).length > 0) {
        const choice = await useDataLossPrompt.getState().ask("pdf", report);
        if (choice === "cancel") return null;
        if (choice === "oswk") {
          return save(undefined, {
            forceDestination: true,
            extension: "oswk",
            defaultPath: replaceExtension(path, "oswk"),
          });
        }
      }
      const result = await api.exportPdf(tab.kind, tab.model, path);
      if (isAndroid()) {
        const failures = await publishOutputs([result.path], androidTarget ? { file: androidTarget } : undefined);
        if (failures.length) {
          useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: failures[0].error });
        } else {
          useToasts.getState().push({
            kind: "success",
            title: t("office.pdfExported"),
            detail: androidTarget?.name ?? fileBaseName(result.path),
          });
        }
      } else {
        useToasts.getState().push({ kind: "success", title: t("office.pdfExported"), detail: result.path });
      }
      return result.path;
    } catch (error) {
      reportError(error, t);
      return null;
    } finally {
      setBusy(false);
    }
  }, [loadCompatibility, save, t, tab]);

  /**
   * Print. Desktop WebViews implement window.print(); the Android WebView
   * ignores it, so there the document is rendered to a PDF in the cache and
   * opened in the system viewer, whose menu prints (or shares) it.
   */
  const print = useCallback(async (): Promise<void> => {
    if (!isAndroid()) {
      window.print();
      return;
    }
    setBusy(true);
    try {
      const result = await api.exportPdf(tab.kind, tab.model, await scratchPath("pdf"));
      await openAnyFile(result.path);
      useToasts.getState().push({ kind: "info", title: t("office.printAndroid") });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  }, [t, tab]);

  const openRecentVersion = useCallback(
    async (version: number) => {
      try {
        const model = await api.historyLoad(historyKeyFor({ id: tab.id, path: tab.path }), version);
        if (model) {
          useOfficeTabs.getState().edit(tab.id, () => model as never);
          useToasts.getState().push({ kind: "info", title: t("office.versionRestored"), detail: `v${version}` });
        }
      } catch (error) {
        reportError(error, t);
      }
    },
    [t, tab.id, tab.path],
  );

  const choosePath = useCallback(async (): Promise<string | null> => {
    // Android has no writable paths to hand out; callers use `save` instead.
    if (isAndroid()) return null;
    const path = (await saveDialog({
      title: `Save ${tab.title}`,
      defaultPath: `${tab.title}.${tab.kind === "writer" ? "docx" : tab.kind === "calc" ? "xlsx" : "pptx"}`,
      filters: FILTERS[tab.kind],
    })) as string | null;
    return path;
  }, [tab]);

  const openFile = useCallback(async (): Promise<string | null> => {
    if (isAndroid()) {
      // SAF picks are copied into the app cache; the engine only sees the path.
      const [picked] = await pickOfficeFiles(false);
      return picked ?? null;
    }
    const selection = await openDialog({
      multiple: false,
      filters: [
        {
          name: "Office documents",
          extensions: ["docx", "odt", "rtf", "txt", "md", "html", "xlsx", "ods", "csv", "pptx", "odp", "oswk"],
        },
      ],
    });
    return typeof selection === "string" ? selection : null;
  }, []);

  const autosaveInterval = useSettings((state) => state.settings).autosaveSeconds ?? 30;

  return { save, saveAs, exportPdf, print, busy, openRecentVersion, choosePath, openFile, autosaveInterval };
}

/**
 * Standard office keyboard shortcuts (Ctrl+S / Ctrl+Shift+S / Ctrl+O / Ctrl+P,
 * plus optional Ctrl+F / Ctrl+H). Uses capture phase so the editor wins over
 * the global PDF shortcuts.
 */
export function useEditorShortcuts(
  session: ReturnType<typeof useOfficeSession>,
  handlers?: { onFind?: () => void; onReplace?: () => void },
) {
  const onFind = handlers?.onFind;
  const onReplace = handlers?.onReplace;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const key = event.key.toLowerCase();
      if (key === "s") {
        event.preventDefault();
        event.stopImmediatePropagation();
        void (event.shiftKey ? session.saveAs() : session.save());
        return;
      }
      if (key === "o") {
        event.preventDefault();
        event.stopImmediatePropagation();
        void openIntoWorkspace();
        return;
      }
      if (key === "p") {
        event.preventDefault();
        event.stopImmediatePropagation();
        window.print();
        return;
      }
      if (key === "f" && onFind) {
        event.preventDefault();
        event.stopImmediatePropagation();
        onFind();
        return;
      }
      if (key === "h" && onReplace) {
        event.preventDefault();
        event.stopImmediatePropagation();
        onReplace();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onFind, onReplace, session]);
}

/** Opens a file dialog and adds the chosen document as a workspace tab. */
export async function openIntoWorkspace(): Promise<string | null> {
  let selection: string | null = null;
  if (isAndroid()) {
    const [picked] = await pickOfficeFiles(false);
    selection = picked ?? null;
  } else {
    const picked = await openDialog({
      multiple: false,
      filters: [
        {
          name: "Documents",
          extensions: ["docx", "odt", "rtf", "txt", "md", "html", "xlsx", "ods", "csv", "pptx", "odp", "oswk"],
        },
        { name: "All files", extensions: ["*"] },
      ],
    });
    selection = typeof picked === "string" ? picked : null;
  }
  if (!selection) return null;
  const result = await openOfficePath(selection);
  if (!result.ok) {
    useToasts.getState().push({ kind: "error", title: "Unable to open this document.", detail: result.error });
    return null;
  }
  return selection;
}
