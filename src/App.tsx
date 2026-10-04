import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  BookOpen,
  Bot,
  Combine,
  FileSearch,
  FolderClock,
  FileImage,
  FilePlus2,
  FolderOpen,
  FileSpreadsheet,
  FileText,
  Presentation,
  LayoutGrid,
  LayoutTemplate,
  NotebookPen,
  CalendarDays,
  Database,
  PenTool,
  Repeat,
  Sparkles,
  ClipboardList,
  Home,
  Images,
  Info,
  Layers,
  Library,
  Lock,
  LockOpen,
  Menu,
  Minimize2,
  Moon,
  Eraser,
  FileDiff,
  ListChecks,
  Puzzle,
  Scissors,
  ShieldCheck,
  Settings as SettingsIcon,
  Stamp,
  Sun,
  Type,
  Wand2,
} from "lucide-react";
import { useT } from "./lib/i18n";
import { errorMessage, useDev, useDrop, useJobs, useRecent, useSettings, useToasts } from "./lib/store";
import packageJson from "../package.json";
import { devLaunchContext, startupFiles } from "./lib/api";
import type { Navigate, ScreenId } from "./lib/nav";
// Route-level code splitting: every screen is loaded when it is first opened,
// so the startup bundle only carries the shell, the navigation and the shared
// UI primitives. The lazy import maps the named export to the default export
// `React.lazy` expects.
const HomeScreen = React.lazy(() => import("./screens/Home").then((module) => ({ default: module.Home })));
const Reader = React.lazy(() => import("./screens/Reader").then((module) => ({ default: module.Reader })));
const Ai = React.lazy(() => import("./screens/Ai").then((module) => ({ default: module.Ai })));
const AiLibrary = React.lazy(() => import("./screens/AiLibrary").then((module) => ({ default: module.AiLibrary })));
const Merge = React.lazy(() => import("./screens/Merge").then((module) => ({ default: module.Merge })));
const Organize = React.lazy(() => import("./screens/Organize").then((module) => ({ default: module.Organize })));
const Split = React.lazy(() => import("./screens/Split").then((module) => ({ default: module.Split })));
const Compress = React.lazy(() => import("./screens/Compress").then((module) => ({ default: module.Compress })));
const Ocr = React.lazy(() => import("./screens/Ocr").then((module) => ({ default: module.Ocr })));
const Convert = React.lazy(() => import("./screens/Convert").then((module) => ({ default: module.Convert })));
const Security = React.lazy(() => import("./screens/Security").then((module) => ({ default: module.Security })));
const Watermark = React.lazy(() => import("./screens/Watermark").then((module) => ({ default: module.Watermark })));
const PageTools = React.lazy(() => import("./screens/PageTools").then((module) => ({ default: module.PageTools })));
const Annotate = React.lazy(() => import("./screens/Annotate").then((module) => ({ default: module.Annotate })));
const Redact = React.lazy(() => import("./screens/Redact").then((module) => ({ default: module.Redact })));
const Compare = React.lazy(() => import("./screens/Compare").then((module) => ({ default: module.Compare })));
const Inspect = React.lazy(() => import("./screens/Inspect").then((module) => ({ default: module.Inspect })));
const Metadata = React.lazy(() => import("./screens/Metadata").then((module) => ({ default: module.Metadata })));
const Batch = React.lazy(() => import("./screens/Batch").then((module) => ({ default: module.Batch })));
const History = React.lazy(() => import("./screens/History").then((module) => ({ default: module.History })));
const Settings = React.lazy(() => import("./screens/Settings").then((module) => ({ default: module.Settings })));
const InfoScreen = React.lazy(() => import("./screens/Info").then((module) => ({ default: module.InfoScreen })));
const Vault = React.lazy(() => import("./screens/Vault").then((module) => ({ default: module.Vault })));
const PdfStudio = React.lazy(() => import("./screens/PdfStudio").then((module) => ({ default: module.PdfStudio })));
const CompatibilityScreen = React.lazy(() =>
  import("./screens/Compatibility").then((module) => ({ default: module.CompatibilityScreen })),
);
const JobsScreen = React.lazy(() => import("./screens/Jobs").then((module) => ({ default: module.JobsScreen })));
const Sync = React.lazy(() => import("./screens/Sync").then((module) => ({ default: module.Sync })));
const Plugins = React.lazy(() => import("./screens/Plugins").then((module) => ({ default: module.Plugins })));
const OfficeWorkspace = React.lazy(() =>
  import("./office/OfficeWorkspace").then((module) => ({ default: module.OfficeWorkspace })),
);
const CleanerScreen = React.lazy(() =>
  import("./office/ToolsScreens").then((module) => ({ default: module.CleanerScreen })),
);
const ConverterScreen = React.lazy(() =>
  import("./office/ToolsScreens").then((module) => ({ default: module.ConverterScreen })),
);
const DataScreen = React.lazy(() => import("./office/ToolsScreens").then((module) => ({ default: module.DataScreen })));
const DrawScreen = React.lazy(() => import("./office/ToolsScreens").then((module) => ({ default: module.DrawScreen })));
const NotesScreen = React.lazy(() =>
  import("./office/ToolsScreens").then((module) => ({ default: module.NotesScreen })),
);
const PdfFormsScreen = React.lazy(() =>
  import("./office/ToolsScreens").then((module) => ({ default: module.PdfFormsScreen })),
);
const PlannerScreen = React.lazy(() =>
  import("./office/ToolsScreens").then((module) => ({ default: module.PlannerScreen })),
);
const TemplatesScreen = React.lazy(() =>
  import("./office/ToolsScreens").then((module) => ({ default: module.TemplatesScreen })),
);
import { matchKeybinding, registerCommand, runCommand, setCommandTranslator, unregisterCommand } from "./lib/commands";
import { resumeJobs, useJobs as useBackgroundJobs } from "./lib/jobs";
import { installJobRetryHandlers } from "./lib/job-retries";
import { CommandPalette, GlobalSearch } from "./components/command-palette";
import { OverwriteDialog, PasswordDialog, Toasts } from "./components/files";
import { DataLossDialogHost } from "./components/data-loss-dialog";
import { FileConflictDialogHost } from "./components/file-conflict-dialog";
import { Badge, IconButton, Spinner } from "./components/ui";
import { isAndroid, openAnyFile, pickAndroidFiles } from "./lib/mobile";
import { isImage } from "./lib/format";
import {
  navigationActionFromState,
  overlayHistoryState,
  recordScreenVisit,
  type NavigationSnapshot,
} from "./lib/nav-history";
import { openIntoWorkspace } from "./office/useOfficeSession";
import { isOfficePath, openOfficePath, useOfficeTabs } from "./lib/office-store";
import { routeForPath } from "./lib/open-route";
import * as officeApi from "./lib/office-api";

type PageToolTab = "extract" | "delete" | "rotate" | "resize" | "crop" | "numbering";

/** Shown while a lazily imported screen chunk is being fetched. */
function ScreenLoading() {
  return (
    <div className="flex items-center justify-center h-full">
      <Spinner size={22} />
    </div>
  );
}

/** Reads and clears the Android open-with queue filled by MainActivity.kt. */
async function takePendingAndroidOpen(): Promise<string[]> {
  return invoke<string[]>("android_take_pending_open");
}

export default function App() {
  const t = useT();
  const init = useSettings((s) => s.init);
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const attachJobs = useJobs((s) => s.attach);
  const refreshRecent = useRecent((s) => s.refresh);
  const setDropHandler = useDrop((s) => s.setHandler);
  const pushToast = useToasts((s) => s.push);

  const [screen, setScreen] = useState<ScreenId>("home");
  const [files, setFiles] = useState<string[]>([]);
  const [pageToolTab, setPageToolTab] = useState<PageToolTab>("extract");
  const [convertTab, setConvertTab] = useState<"pdfToImages" | "imagesToPdf">("pdfToImages");
  const [securityTab, setSecurityTab] = useState<"protect" | "unlock">("protect");
  const [dragging, setDragging] = useState(false);
  const [sidebarCompact, setSidebarCompact] = useState(false);
  const [navOpen, setNavOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [narrow, setNarrow] = useState(() => typeof window !== "undefined" && window.innerWidth < 900);
  const backgroundJobs = useBackgroundJobs((state) => state.jobs.filter((job) => job.status === "running").length);
  // WebView history bridge for the Android back button (see lib/nav-history).
  const navSnapshotRef = useRef<NavigationSnapshot>({ screen: "home", files: [] });
  const overlayGuardRef = useRef(false);

  // Phones always use the drawer navigation; desktop windows switch to it
  // when they get narrow enough for the sidebar to waste space.
  const compactNav = isAndroid() || narrow;

  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < 900);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const navigate = useCallback<Navigate>((next, options) => {
    setFiles(options?.files ?? []);
    if (options?.pageToolsTab) setPageToolTab(options.pageToolsTab);
    if (next === "pdfToImages") {
      setConvertTab("pdfToImages");
      setScreen("pdfToImages");
      return;
    }
    if (next === "imagesToPdf") {
      setConvertTab("imagesToPdf");
      setScreen("imagesToPdf");
      return;
    }
    if (next === "protect" || next === "unlock") {
      setSecurityTab(next);
      setScreen(next);
      return;
    }
    setScreen(next);
  }, []);

  // Record one webview history entry per screen so the system back button can
  // walk the app's own stack. Same-screen file refreshes replace the entry
  // instead of stacking. This effect must stay before the overlay guard below:
  // a palette navigation has to push the new screen before the guard is popped.
  useEffect(() => {
    recordScreenVisit(history, navSnapshotRef.current, { screen, files });
    navSnapshotRef.current = { screen, files };
  }, [screen, files]);

  // Overlays push a guard entry, so back closes them first. When the user
  // closes an overlay through the UI the guard is popped here to stay in sync.
  const overlayOpen = navOpen || paletteOpen || searchOpen;
  useEffect(() => {
    if (overlayOpen && !overlayGuardRef.current) {
      overlayGuardRef.current = true;
      history.pushState(overlayHistoryState(), "");
    } else if (!overlayOpen && overlayGuardRef.current) {
      overlayGuardRef.current = false;
      history.back();
    }
  }, [overlayOpen]);

  // Back button / Alt+Left: turn history states back into navigation.
  useEffect(() => {
    const onPopState = (event: PopStateEvent) => {
      // While an overlay is open its guard entry sits on top of the current
      // screen, so this back press just closes the overlay.
      if (overlayGuardRef.current) {
        overlayGuardRef.current = false;
        setNavOpen(false);
        setPaletteOpen(false);
        setSearchOpen(false);
        return;
      }
      const action = navigationActionFromState(event.state);
      if (action.kind === "close-overlays") {
        // Landing on a guard entry happens when the UI already closed an
        // overlay and popped the guard itself; nothing left to do.
        return;
      }
      if (action.kind === "navigate") {
        // Update the snapshot first so the screen effect above replaces the
        // entry instead of pushing a duplicate while the webview is moving.
        navSnapshotRef.current = { screen: action.screen, files: action.files };
        setFiles(action.files);
        setScreen(action.screen);
        return;
      }
      // Walked past the first app entry: Home is the root, the next back press
      // leaves the app.
      navSnapshotRef.current = { screen: "home", files: [] };
      setFiles([]);
      setScreen("home");
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  // Global initialization
  useEffect(() => {
    void init();
    void refreshRecent();
    // Loads persisted job records (running ones come back as interrupted).
    void resumeJobs();
    let unlisten: (() => void) | undefined;
    void attachJobs().then((fn) => {
      unlisten = fn;
    });
    // Development/screenshot hook (no-op unless PDFSAK_* env vars are set).
    void devLaunchContext()
      .then((context) => {
        useDev.getState().set({
          startScreen: context.startScreen,
          newTab: context.newTab,
          files: context.files,
          autoRun: Boolean(context.autoRun),
          tab: context.tab,
        });
        void import("./lib/api").then(({ logFrontend }) =>
          logFrontend(
            "info",
            `dev-context: screen=${context.startScreen} files=${(context.files ?? []).length} autoRun=${context.autoRun}`,
          ),
        );
        if (context.startScreen) {
          navigate(context.startScreen as ScreenId, { files: context.files ?? [] });
        } else if (context.files?.length) {
          setFiles(context.files);
        }
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, [attachJobs, init, navigate, refreshRecent]);

  // Keep the native window chrome in sync with the selected theme.
  useEffect(() => {
    const resolved: "dark" | "light" =
      settings.theme === "system"
        ? document.documentElement.classList.contains("dark")
          ? "dark"
          : "light"
        : settings.theme === "paper"
          ? "light"
          : "dark";
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(resolved))
      .catch(() => undefined);
  }, [settings.theme]);

  // The document language drives spell check and assistive technology.
  useEffect(() => {
    document.documentElement.lang = settings.language;
  }, [settings.language]);

  // Opens one document on the screen that can actually edit/view it: office
  // documents in the workspace, PDFs in the reader and images in the
  // image-to-PDF tool. Unknown types go to the system viewer.
  const openSinglePath = useCallback((path: string) => {
    const route = routeForPath(path);
    if (!route) {
      void openAnyFile(path).catch(() => undefined);
      return;
    }
    if (route.office) {
      setScreen("office");
      void openOfficePath(path);
      return;
    }
    setFiles(route.files ?? []);
    setScreen(route.screen);
  }, []);

  // OS drag & drop from Explorer
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragging(true);
        } else if (event.payload.type === "drop") {
          setDragging(false);
          const paths = event.payload.paths ?? [];
          if (!paths.length) return;
          if (screen === "home") {
            // Dropping a single document opens it; several files stay on the
            // board as a quick-action suggestion.
            if (paths.length === 1) {
              openSinglePath(paths[0]);
              return;
            }
            setFiles(paths);
            pushToast({
              kind: "info",
              title: t("home.quickActions"),
              detail: `${paths.length} files`,
            });
            return;
          }
          const handler = useDrop.getState().handler;
          if (handler) handler(paths);
        } else {
          setDragging(false);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, [openSinglePath, pushToast, screen, t]);

  // Command platform: route palette titles through the active language and
  // register the real navigation commands (palette and global search).
  useEffect(() => {
    setCommandTranslator(t);
  }, [t]);

  // Files picked through the global open dialog (Ctrl+O / file.open command):
  // office documents open as workspace tabs, everything else goes through the
  // active screen's drop handler (or seeds the Home suggestion card).
  const openPickedFiles = useCallback(
    (paths: string[]) => {
      if (!paths.length) return;
      const officePaths = paths.filter((path) => isOfficePath(path));
      const documentPaths = paths.filter((path) => !isOfficePath(path));
      if (officePaths.length > 0) {
        setScreen("office");
        for (const path of officePaths) void openOfficePath(path);
      }
      if (documentPaths.length === 0) return;
      if (screen === "home" || officePaths.length > 0) {
        setFiles(documentPaths);
        return;
      }
      const handler = useDrop.getState().handler;
      if (handler) handler(documentPaths);
    },
    [screen],
  );

  const pickFiles = useCallback(() => {
    if (isAndroid()) {
      void pickAndroidFiles({ multiple: true, accept: "any" })
        .then(openPickedFiles)
        .catch(() => undefined);
      return;
    }
    void open({
      multiple: true,
      filters: [
        {
          name: "Documents",
          extensions: [
            "pdf",
            "jpg",
            "jpeg",
            "png",
            "webp",
            "bmp",
            "tif",
            "tiff",
            "docx",
            "odt",
            "rtf",
            "txt",
            "md",
            "html",
            "xlsx",
            "ods",
            "csv",
            "tsv",
            "pptx",
            "odp",
            "oswk",
          ],
        },
      ],
    }).then((picked) => {
      if (!picked) return;
      openPickedFiles((Array.isArray(picked) ? picked : [picked]).map(String));
    });
  }, [openPickedFiles]);

  // Retry routing for background jobs: one handler per tracked kind, so a
  // failed or interrupted job can be re-run from its persisted payload even
  // when the originating screen is not mounted.
  useEffect(() => installJobRetryHandlers(), []);

  // Command platform: every screen is a command, alongside the global file,
  // palette and search actions. The keybindings registered here are the single
  // source of truth for the former hard-coded shortcut handler.
  useEffect(() => {
    const views: { id: ScreenId; key: string; category: "view" | "settings"; keybinding?: string }[] = [
      { id: "home", key: "nav.home", category: "view" },
      { id: "office", key: "nav.office", category: "view" },
      { id: "reader", key: "nav.reader", category: "view" },
      { id: "ai", key: "nav.ai", category: "view" },
      { id: "aiLibrary", key: "nav.aiLibrary", category: "view" },
      { id: "merge", key: "nav.merge", category: "view" },
      { id: "organize", key: "nav.organize", category: "view" },
      { id: "split", key: "nav.split", category: "view" },
      { id: "compress", key: "nav.compress", category: "view" },
      { id: "ocr", key: "nav.ocr", category: "view" },
      { id: "pdfToImages", key: "nav.pdfToImages", category: "view" },
      { id: "imagesToPdf", key: "nav.imagesToPdf", category: "view" },
      { id: "protect", key: "nav.protect", category: "view" },
      { id: "unlock", key: "nav.unlock", category: "view" },
      { id: "watermark", key: "nav.watermark", category: "view" },
      { id: "annotate", key: "nav.annotate", category: "view" },
      { id: "redact", key: "nav.redact", category: "view" },
      { id: "compare", key: "nav.compare", category: "view" },
      { id: "inspect", key: "nav.inspect", category: "view" },
      { id: "metadata", key: "nav.metadata", category: "view" },
      { id: "pageTools", key: "nav.pageTools", category: "view" },
      { id: "batch", key: "nav.batch", category: "view" },
      { id: "pdfStudio", key: "nav.pdfStudio", category: "view" },
      { id: "pdfForms", key: "nav.pdfForms", category: "view" },
      { id: "vault", key: "nav.vault", category: "view" },
      { id: "documents", key: "nav.documents", category: "view" },
      { id: "spreadsheets", key: "nav.spreadsheets", category: "view" },
      { id: "presentations", key: "nav.presentations", category: "view" },
      { id: "templates", key: "nav.templates", category: "view" },
      { id: "notes", key: "nav.notes", category: "view" },
      { id: "planner", key: "nav.planner", category: "view" },
      { id: "data", key: "nav.data", category: "view" },
      { id: "draw", key: "nav.draw", category: "view" },
      { id: "converter", key: "nav.converter", category: "view" },
      { id: "cleaner", key: "nav.cleaner", category: "view" },
      { id: "history", key: "nav.history", category: "view" },
      { id: "jobs", key: "nav.jobs", category: "view" },
      { id: "sync", key: "nav.sync", category: "view" },
      { id: "plugins", key: "nav.plugins", category: "view" },
      { id: "compat", key: "nav.compat", category: "view" },
      { id: "info", key: "nav.info", category: "view" },
      { id: "settings", key: "nav.settings", category: "settings", keybinding: "Ctrl+," },
    ];
    for (const view of views) {
      registerCommand({
        id: `view.${view.id}`,
        titleKey: view.key,
        category: view.category,
        keybinding: view.keybinding,
        enabled: () => true,
        execute: () => setScreen(view.id),
      });
    }
    registerCommand({
      id: "file.open",
      titleKey: "common.openFile",
      category: "file",
      keybinding: "Ctrl+O",
      enabled: () => true,
      execute: () => pickFiles(),
    });
    registerCommand({
      id: "app.commandPalette",
      titleKey: "palette.open",
      category: "view",
      keybinding: "Ctrl+Shift+P",
      enabled: () => true,
      execute: () => setPaletteOpen(true),
    });
    registerCommand({
      id: "app.globalSearch",
      titleKey: "palette.search",
      category: "view",
      keybinding: "Ctrl+Shift+F",
      enabled: () => true,
      execute: () => setSearchOpen(true),
    });
    const ids = [...views.map((view) => `view.${view.id}`), "file.open", "app.commandPalette", "app.globalSearch"];
    return () => ids.forEach((id) => unregisterCommand(id));
  }, [pickFiles]);

  // One keyboard entry point: the command registry decides. Modified chords
  // run commands (palette, search, open, settings); unmodified keys are left
  // to the focused editor or control.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = !!target && (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable);
      const mod = event.ctrlKey || event.metaKey;
      if ((typing || event.key === "Escape") && !mod && !event.altKey) return;
      const id = matchKeybinding({
        key: event.key,
        ctrlKey: event.ctrlKey,
        metaKey: event.metaKey,
        shiftKey: event.shiftKey,
        altKey: event.altKey,
      });
      if (!id) return;
      event.preventDefault();
      runCommand(id, { screen });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [screen]);

  // Files handed to the process (Windows file association, "open with").
  useEffect(() => {
    let cancelled = false;
    void startupFiles()
      .then((paths) => {
        if (cancelled || paths.length === 0) return;
        const officePaths = paths.filter((path) => isOfficePath(path));
        if (officePaths.length > 0) {
          setScreen("office");
          for (const path of officePaths) void openOfficePath(path);
        }
        const pdfPaths = paths.filter((path) => path.toLowerCase().endsWith(".pdf"));
        if (pdfPaths.length > 0 && officePaths.length === 0) {
          setFiles(pdfPaths);
          setScreen("reader");
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  // Routes documents that arrived through an Android open-with intent to the
  // tool that can handle them. Unknown leftovers are handed to the system
  // viewer so nothing is silently dropped.
  const handleIncomingPaths = useCallback((paths: string[]) => {
    const officePaths = paths.filter((path) => isOfficePath(path));
    if (officePaths.length > 0) {
      setScreen("office");
      for (const path of officePaths) void openOfficePath(path);
      return;
    }
    const pdfPaths = paths.filter((path) => path.toLowerCase().endsWith(".pdf"));
    if (pdfPaths.length > 0) {
      setFiles(pdfPaths);
      setScreen("reader");
      return;
    }
    const imagePaths = paths.filter((path) => isImage(path));
    if (imagePaths.length > 0) {
      setFiles(imagePaths);
      setScreen("imagesToPdf");
      return;
    }
    for (const path of paths) void openAnyFile(path).catch(() => undefined);
  }, []);

  // Android open-with: MainActivity copies shared documents into the cache and
  // queues their paths. The copy runs on a background thread, so the queue is
  // polled once shortly after mount, when the app returns to the foreground,
  // and continuously while the activity is visible - a share sent to an app
  // that is already on screen (singleTask -> onNewIntent) never triggers a
  // visibility change, so the poll is what makes that case work. The command
  // is a small file read and returns nothing when the queue is empty.
  useEffect(() => {
    if (!isAndroid()) return;
    let cancelled = false;
    const seen = new Set<string>();
    const drain = () => {
      void takePendingAndroidOpen()
        .then((paths) => {
          if (cancelled || !paths.length) return;
          const fresh = paths.filter((path) => !seen.has(path));
          for (const path of fresh) seen.add(path);
          if (fresh.length > 0) handleIncomingPaths(fresh);
        })
        .catch(() => undefined);
    };
    drain();
    const retry = window.setTimeout(drain, 900);
    const poll = window.setInterval(() => {
      if (document.visibilityState === "visible") drain();
    }, 2500);
    const onVisibility = () => {
      if (document.visibilityState === "visible") drain();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelled = true;
      window.clearTimeout(retry);
      window.clearInterval(poll);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [handleIncomingPaths]);

  const homeDrop = useCallback(
    (paths: string[]) => {
      // A single picked document opens directly on the right screen; several
      // files stay on Home as a quick-action suggestion (merge, batch, ...).
      if (paths.length === 1) {
        openSinglePath(paths[0]);
        return;
      }
      setFiles(paths);
      setDropHandler(null);
    },
    [openSinglePath, setDropHandler],
  );

  // Global search opens any real file: office documents go to the workspace,
  // everything else to the PDF reader.
  const openAnyPath = useCallback((path: string) => {
    if (isOfficePath(path)) {
      setScreen("office");
      void openOfficePath(path);
      return;
    }
    setFiles([path]);
    setScreen("reader");
  }, []);

  const screens: Record<ScreenId, React.ReactElement> = useMemo(
    () => ({
      home: <HomeScreen onNavigate={navigate} onDropFiles={homeDrop} dragging={dragging} onFileList={setFiles} />,
      reader: <Reader initialFiles={files} dragging={dragging} />,
      ai: <Ai initialFiles={files} dragging={dragging} />,
      aiLibrary: <AiLibrary onOpenAi={() => navigate("ai")} />,
      merge: <Merge initialFiles={files} dragging={dragging} />,
      organize: <Organize initialFiles={files} dragging={dragging} />,
      split: <Split initialFiles={files} dragging={dragging} />,
      compress: <Compress initialFiles={files} dragging={dragging} />,
      ocr: <Ocr initialFiles={files} dragging={dragging} />,
      pdfToImages: <Convert tab={convertTab} initialFiles={files} dragging={dragging} />,
      imagesToPdf: <Convert tab={convertTab} initialFiles={files} dragging={dragging} />,
      protect: <Security tab={securityTab} initialFiles={files} dragging={dragging} />,
      unlock: <Security tab={securityTab} initialFiles={files} dragging={dragging} />,
      watermark: <Watermark initialFiles={files} dragging={dragging} />,
      annotate: <Annotate initialFiles={files} dragging={dragging} />,
      redact: <Redact initialFiles={files} dragging={dragging} />,
      compare: <Compare initialFiles={files} dragging={dragging} />,
      inspect: <Inspect initialFiles={files} dragging={dragging} />,
      metadata: <Metadata initialFiles={files} dragging={dragging} />,
      pageTools: <PageTools tab={pageToolTab} initialFiles={files} dragging={dragging} />,
      batch: <Batch initialFiles={files} dragging={dragging} />,
      history: <History onNavigate={navigate} />,
      settings: <Settings />,
      info: <InfoScreen initialFiles={files} />,
      office: <OfficeWorkspace />,
      documents: <OfficeLauncher kind="writer" onOpen={() => navigate("office")} />,
      spreadsheets: <OfficeLauncher kind="calc" onOpen={() => navigate("office")} />,
      presentations: <OfficeLauncher kind="impress" onOpen={() => navigate("office")} />,
      notes: <NotesScreen />,
      templates: <TemplatesScreen />,
      converter: <ConverterScreen />,
      cleaner: <CleanerScreen />,
      draw: <DrawScreen />,
      planner: <PlannerScreen />,
      data: <DataScreen />,
      pdfForms: <PdfFormsScreen />,
      vault: <Vault />,
      pdfStudio: <PdfStudio initialFiles={files} dragging={dragging} />,
      compat: <CompatibilityScreen />,
      jobs: <JobsScreen />,
      sync: <Sync />,
      plugins: <Plugins />,
    }),
    [convertTab, dragging, files, homeDrop, navigate, pageToolTab, securityTab],
  );

  const navGroups: { label?: string; items: { id: ScreenId; label: string; icon: React.ReactElement }[] }[] = [
    {
      items: [
        { id: "home", label: t("nav.home"), icon: <Home size={16} /> },
        { id: "office", label: t("nav.office"), icon: <LayoutGrid size={16} /> },
        { id: "reader", label: t("nav.reader"), icon: <BookOpen size={16} /> },
      ],
    },
    {
      label: t("nav.office"),
      items: [
        { id: "documents", label: t("nav.documents"), icon: <FileText size={16} /> },
        { id: "spreadsheets", label: t("nav.spreadsheets"), icon: <FileSpreadsheet size={16} /> },
        { id: "presentations", label: t("nav.presentations"), icon: <Presentation size={16} /> },
        { id: "templates", label: t("nav.templates"), icon: <LayoutTemplate size={16} /> },
      ],
    },
    {
      label: t("nav.office"),
      items: [
        { id: "notes", label: t("nav.notes"), icon: <NotebookPen size={16} /> },
        { id: "planner", label: t("nav.planner"), icon: <CalendarDays size={16} /> },
        { id: "data", label: t("nav.data"), icon: <Database size={16} /> },
        { id: "draw", label: t("nav.draw"), icon: <PenTool size={16} /> },
        { id: "vault", label: t("nav.vault"), icon: <FileSearch size={16} /> },
      ],
    },
    {
      label: t("nav.convert"),
      items: [
        { id: "converter", label: t("nav.converter"), icon: <Repeat size={16} /> },
        { id: "cleaner", label: t("nav.cleaner"), icon: <Sparkles size={16} /> },
        { id: "pdfForms", label: t("nav.pdfForms"), icon: <ClipboardList size={16} /> },
      ],
    },
    {
      label: t("nav.pdfTools"),
      items: [
        { id: "merge", label: t("nav.merge"), icon: <Combine size={16} /> },
        { id: "organize", label: t("nav.organize"), icon: <Layers size={16} /> },
        { id: "split", label: t("nav.split"), icon: <Scissors size={16} /> },
        { id: "compress", label: t("nav.compress"), icon: <Minimize2 size={16} /> },
        { id: "pageTools", label: t("nav.pageTools"), icon: <Wand2 size={16} /> },
        { id: "pdfStudio", label: t("nav.pdfStudio"), icon: <ShieldCheck size={16} /> },
        { id: "watermark", label: t("nav.watermark"), icon: <Stamp size={16} /> },
        { id: "annotate", label: t("nav.annotate"), icon: <Type size={16} /> },
        { id: "redact", label: t("nav.redact"), icon: <Eraser size={16} /> },
        { id: "compare", label: t("nav.compare"), icon: <FileDiff size={16} /> },
        { id: "inspect", label: t("nav.inspect"), icon: <FileSearch size={16} /> },
        { id: "metadata", label: t("nav.metadata"), icon: <Puzzle size={16} /> },
      ],
    },
    {
      label: t("nav.convert"),
      items: [
        { id: "pdfToImages", label: t("nav.pdfToImages"), icon: <FileImage size={16} /> },
        { id: "imagesToPdf", label: t("nav.imagesToPdf"), icon: <Images size={16} /> },
      ],
    },
    { items: [{ id: "ocr", label: t("nav.ocr"), icon: <FileSearch size={16} /> }] },
    {
      label: t("nav.security"),
      items: [
        { id: "protect", label: t("nav.protect"), icon: <Lock size={16} /> },
        { id: "unlock", label: t("nav.unlock"), icon: <LockOpen size={16} /> },
      ],
    },
    {
      label: t("nav.ai"),
      items: [
        { id: "ai", label: t("nav.ai"), icon: <Bot size={16} /> },
        { id: "aiLibrary", label: t("nav.aiLibrary"), icon: <Library size={16} /> },
      ],
    },
    {
      label: t("nav.batch"),
      items: [
        { id: "batch", label: t("nav.batch"), icon: <Archive size={16} /> },
        { id: "jobs", label: t("nav.jobs"), icon: <ListChecks size={16} /> },
        { id: "sync", label: t("nav.sync"), icon: <Repeat size={16} /> },
        { id: "plugins", label: t("nav.plugins"), icon: <Puzzle size={16} /> },
        { id: "compat", label: t("nav.compat"), icon: <ShieldCheck size={16} /> },
        { id: "info", label: t("nav.info"), icon: <Info size={16} /> },
        { id: "history", label: t("nav.history"), icon: <FolderClock size={16} /> },
        { id: "settings", label: t("nav.settings"), icon: <SettingsIcon size={16} /> },
      ],
    },
  ];

  const isDark =
    settings.theme === "dark" || (settings.theme === "system" && document.documentElement.classList.contains("dark"));

  const renderNav = (showLabels: boolean) => (
    <>
      {navGroups.map((group, index) => (
        <div key={index}>
          {group.label && showLabels ? <p className="nav-group-label">{group.label}</p> : null}
          {group.items.map((item) => {
            const active =
              screen === item.id ||
              (item.id === "pdfToImages" && screen === "pdfToImages") ||
              (item.id === "imagesToPdf" && screen === "imagesToPdf");
            return (
              <button
                key={item.id}
                className="nav-item"
                data-active={active}
                aria-current={active ? "page" : undefined}
                onClick={() => {
                  navigate(item.id);
                  setNavOpen(false);
                }}
                title={showLabels ? undefined : item.label}
              >
                <span className="shrink-0">{item.icon}</span>
                {showLabels ? <span className="truncate">{item.label}</span> : null}
              </button>
            );
          })}
        </div>
      ))}
    </>
  );

  const activeLabel =
    navGroups.flatMap((group) => group.items).find((item) => item.id === screen)?.label ?? t("app.name");

  // Phones (and narrow desktop windows): drawer navigation with a top bar.
  if (compactNav) {
    return (
      <div className="flex flex-col h-full" style={{ background: "var(--bg)" }}>
        <header className="mobile-bar">
          <IconButton label={t("app.name")} onClick={() => setNavOpen(true)} aria-expanded={navOpen}>
            <Menu size={18} />
          </IconButton>
          <p className="font-semibold text-[14px] truncate flex-1">{activeLabel}</p>
          <IconButton label={t("settings.theme")} onClick={() => void update({ theme: isDark ? "light" : "dark" })}>
            {isDark ? <Sun size={16} /> : <Moon size={16} />}
          </IconButton>
        </header>

        {navOpen ? (
          <div
            className="drawer-overlay"
            role="presentation"
            onClick={(event) => {
              if (event.target === event.currentTarget) setNavOpen(false);
            }}
          >
            <aside className="drawer">
              <div className="flex items-center gap-2.5 px-3.5 py-4">
                <div
                  className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
                  style={{ background: "var(--accent)", color: "var(--accent-text)" }}
                >
                  <Puzzle size={18} />
                </div>
                <div className="min-w-0">
                  <p className="font-bold text-[13.5px] leading-tight truncate">{t("app.name")}</p>
                  <p className="text-[11px] muted truncate">v{packageJson.version} · local</p>
                </div>
              </div>
              <nav className="flex-1 overflow-y-auto px-2 pb-4">{renderNav(true)}</nav>
              <div className="px-3 py-3 border-t" style={{ borderColor: "var(--border)" }}>
                <Badge tone="ok">
                  <Lock size={10} /> {t("nav.privacy")}
                </Badge>
              </div>
            </aside>
          </div>
        ) : null}

        <main className="flex-1 min-w-0 relative overflow-hidden">
          <div key={`${screen}-${files.join("|")}`} className="h-full">
            <React.Suspense fallback={<ScreenLoading />}>{screens[screen]}</React.Suspense>
          </div>
        </main>

        <Toasts />
        <div className="sr-only" role="status" aria-live="polite">
          {backgroundJobs > 0 ? t("jobs.runningCount", { count: backgroundJobs }) : ""}
        </div>
        <OverwriteDialog />
        <PasswordDialog />
        {/* One global host so the compatibility gate also covers the
            converter, which is not rendered inside the office workspace. */}
        <DataLossDialogHost />
        <FileConflictDialogHost />
        <CommandPalette
          open={paletteOpen}
          onClose={() => setPaletteOpen(false)}
          context={{ screen }}
          onNavigate={(next) => setScreen(next as ScreenId)}
        />
        <GlobalSearch
          open={searchOpen}
          onClose={() => setSearchOpen(false)}
          onOpenPath={openAnyPath}
          onNavigate={(next) => setScreen(next as ScreenId)}
        />
      </div>
    );
  }

  return (
    <div className="flex h-full" style={{ background: "var(--bg)" }}>
      {/* Sidebar */}
      <aside
        className="flex flex-col shrink-0 border-r"
        style={{
          width: sidebarCompact ? 64 : 232,
          borderColor: "var(--border)",
          background: "var(--bg-soft)",
          transition: "width 0.15s ease",
        }}
      >
        <div className="flex items-center gap-2.5 px-3.5 py-4">
          <div
            className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0"
            style={{ background: "var(--accent)", color: "var(--accent-text)" }}
          >
            <Puzzle size={18} />
          </div>
          {!sidebarCompact ? (
            <div className="min-w-0">
              <p className="font-bold text-[13.5px] leading-tight truncate">{t("app.name")}</p>
              <p className="text-[11px] muted truncate">v{packageJson.version} · local</p>
            </div>
          ) : null}
        </div>

        <nav className="flex-1 overflow-y-auto px-2 pb-3">{renderNav(!sidebarCompact)}</nav>

        <div className="px-3 py-3 border-t flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
          {!sidebarCompact ? (
            <Badge tone="ok">
              <Lock size={10} /> {t("nav.privacy")}
            </Badge>
          ) : null}
          <div className="flex items-center gap-1">
            <button
              type="button"
              className="icon-btn"
              title={t("nav.jobs")}
              onClick={() => setScreen("jobs")}
              style={{ position: "relative" }}
            >
              <ListChecks size={15} />
              {backgroundJobs > 0 ? <span className="jobs-dot">{backgroundJobs}</span> : null}
            </button>
            <IconButton label={t("settings.theme")} onClick={() => void update({ theme: isDark ? "light" : "dark" })}>
              {isDark ? <Sun size={15} /> : <Moon size={15} />}
            </IconButton>
            <IconButton label={t("nav.sidebar")} onClick={() => setSidebarCompact((previous) => !previous)}>
              <Layers size={15} />
            </IconButton>
          </div>
        </div>
      </aside>

      {/* Main area */}
      <main className="flex-1 min-w-0 h-full relative">
        {dragging ? (
          <div
            className="absolute inset-0 z-50 flex items-center justify-center pointer-events-none"
            style={{
              background: "color-mix(in srgb, var(--accent) 8%, transparent)",
              border: "2px dashed var(--accent)",
            }}
          >
            <p className="font-semibold" style={{ color: "var(--accent)" }}>
              {t("common.dropHere")}
            </p>
          </div>
        ) : null}
        <div key={`${screen}-${files.join("|")}`} className="h-full">
          <React.Suspense fallback={<ScreenLoading />}>{screens[screen]}</React.Suspense>
        </div>
      </main>

      <Toasts />
      <div className="sr-only" role="status" aria-live="polite">
        {backgroundJobs > 0 ? t("jobs.runningCount", { count: backgroundJobs }) : ""}
      </div>
      <OverwriteDialog />
      <PasswordDialog />
      {/* Global compatibility gate host (converter + office workspace). */}
      <DataLossDialogHost />
      <FileConflictDialogHost />
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        context={{ screen }}
        onNavigate={(next) => setScreen(next as ScreenId)}
      />
      <GlobalSearch
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onOpenPath={openAnyPath}
        onNavigate={(next) => setScreen(next as ScreenId)}
      />
    </div>
  );
}

interface OfficeLauncherProps {
  kind: "writer" | "calc" | "impress";
  onOpen: () => void;
}

const OFFICE_EXTENSIONS: Record<OfficeLauncherProps["kind"], string[]> = {
  writer: ["docx", "odt", "rtf", "txt", "md"],
  calc: ["xlsx", "xls", "ods", "csv", "tsv"],
  impress: ["pptx", "odp"],
};

function OfficeLauncher({ kind, onOpen }: OfficeLauncherProps) {
  const t = useT();
  const recent = useRecent((state) => state.entries);
  const refreshRecent = useRecent((state) => state.refresh);

  useEffect(() => {
    void refreshRecent();
  }, [refreshRecent]);

  const openPath = async (path: string) => {
    try {
      const result = await officeApi.openDocument(path);
      useOfficeTabs.getState().open({
        kind: result.kind,
        title: result.title,
        path: result.path,
        model: result.model as never,
        warnings: result.warnings,
      });
      onOpen();
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: errorMessage(error, t) });
    }
  };

  const related = recent.filter((entry) =>
    OFFICE_EXTENSIONS[kind].includes((entry.path.split(".").pop() ?? "").toLowerCase()),
  );

  return (
    <div className="screen">
      <div className="screen-head">
        <div>
          <h1>{t(kind === "writer" ? "nav.documents" : kind === "calc" ? "nav.spreadsheets" : "nav.presentations")}</h1>
          <p className="muted">{t("office.noTabsHint")}</p>
        </div>
        <button
          type="button"
          className="btn btn-soft"
          onClick={async () => {
            const opened = await openIntoWorkspace();
            if (opened) onOpen();
          }}
        >
          <FolderOpen size={16} /> {t("common.open")}
        </button>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => {
            useOfficeTabs.getState().create(kind);
            onOpen();
          }}
        >
          <FilePlus2 size={16} />{" "}
          {t(
            kind === "writer"
              ? "office.newDocument"
              : kind === "calc"
                ? "office.newSpreadsheet"
                : "office.newPresentation",
          )}
        </button>
      </div>
      <div className="card">
        <h3>{t("home.recent")}</h3>
        {related.length === 0 ? <p className="muted">{t("converter.noFiles")}</p> : null}
        <div className="stack">
          {related.map((entry) => (
            <button key={entry.path} type="button" className="row recent-row" onClick={() => void openPath(entry.path)}>
              <span className="grow">{entry.path.split(/[\\/]/).pop()}</span>
              <span className="muted">{entry.path}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
