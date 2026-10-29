import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { pickDocumentTokens, type FileToken } from "./lib/file-tokens";
import { extensionsFor } from "./lib/tool-files";
import { openTokenDocument } from "./lib/file-refs";
import { firstIncomingError, incomingErrorKey } from "./lib/incoming";
import { useDropTokens, useLaunchTokens } from "./lib/use-file-ref-events";
import {
  Archive,
  BookOpen,
  Bot,
  Combine,
  FileSearch,
  FolderClock,
  FileImage,
  FilePlus2,
  CalendarClock,
  FolderCheck,
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
  Receipt,
  Mail,
  Camera,
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
import { useDev, useDrop, useIsDarkTheme, useJobs, useRecent, useSettings, useToasts } from "./lib/store";
import packageJson from "../package.json";
import { devLaunchContext } from "./lib/api";
import type { Navigate, PageToolsTab, ScreenId } from "./lib/nav";
import { flagEnabled } from "./lib/flags";
import { enabledRoutes, navSections, routeEnabled } from "./lib/routes";
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
// Phones only; until it loads an empty bar of the same height holds its place.
const BottomNav = React.lazy(() => import("./components/bottom-nav").then((module) => ({ default: module.BottomNav })));
const PresendDialog = React.lazy(() =>
  import("./components/presend-dialog").then((module) => ({ default: module.PresendDialog })),
);
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
  import("./office/tools/cleaner").then((module) => ({ default: module.CleanerScreen })),
);
const ConverterScreen = React.lazy(() =>
  import("./office/tools/converter").then((module) => ({ default: module.ConverterScreen })),
);
const InvoicesScreen = React.lazy(() => import("./screens/Invoices").then((module) => ({ default: module.Invoices })));
const EmlScreen = React.lazy(() => import("./screens/Eml").then((module) => ({ default: module.Eml })));
const ScannerScreen = React.lazy(() => import("./screens/Scanner").then((module) => ({ default: module.Scanner })));
const RecipesScreen = React.lazy(() => import("./screens/Recipes").then((module) => ({ default: module.Recipes })));
const MailMergeScreen = React.lazy(() =>
  import("./screens/MailMerge").then((module) => ({ default: module.MailMerge })),
);
const DataScreen = React.lazy(() => import("./office/tools/data").then((module) => ({ default: module.DataScreen })));
const DrawScreen = React.lazy(() => import("./office/tools/draw").then((module) => ({ default: module.DrawScreen })));
const NotesScreen = React.lazy(() =>
  import("./office/tools/notes").then((module) => ({ default: module.NotesScreen })),
);
const PdfFormsScreen = React.lazy(() =>
  import("./office/tools/pdf-forms").then((module) => ({ default: module.PdfFormsScreen })),
);
const PlannerScreen = React.lazy(() =>
  import("./office/tools/planner").then((module) => ({ default: module.PlannerScreen })),
);
const TemplatesScreen = React.lazy(() =>
  import("./office/tools/templates").then((module) => ({ default: module.TemplatesScreen })),
);
const DeadlineCalculatorScreen = React.lazy(() =>
  import("./screens/DeadlineCalculator").then((module) => ({ default: module.DeadlineCalculatorScreen })),
);
const EvidenceBundleScreen = React.lazy(() =>
  import("./screens/EvidenceBundle").then((module) => ({ default: module.EvidenceBundleScreen })),
);
const PetitionWizardScreen = React.lazy(() =>
  import("./screens/PetitionWizard").then((module) => ({ default: module.PetitionWizard })),
);
const KvkkWizardScreen = React.lazy(() =>
  import("./screens/KvkkWizard").then((module) => ({ default: module.KvkkWizard })),
);
const DocumentHealthScreen = React.lazy(() =>
  import("./screens/DocumentHealthScreen").then((module) => ({ default: module.DocumentHealthScreen })),
);
import { matchKeybinding, registerCommand, runCommand, setCommandTranslator, unregisterCommand } from "./lib/commands";
import { resumeJobs, useJobs as useBackgroundJobs } from "./lib/jobs";
import { installJobRetryHandlers } from "./lib/job-retries";
import { CommandPalette, GlobalSearch } from "./components/command-palette";
import { OverwriteDialog, PasswordDialog, Toasts } from "./components/files";
import { usePresend } from "./lib/presend-gate";
import { DataLossDialogHost } from "./components/data-loss-dialog";
import { FileConflictDialogHost } from "./components/file-conflict-dialog";
import { Badge, IconButton, Spinner } from "./components/ui";
import { BrandMark } from "./components/brand";
import { ErrorBoundary } from "./components/ErrorBoundary";
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
import { useAndroidPendingOpen } from "./lib/android-open-with";
import { recordOpenWith } from "./lib/open-with-diagnostics";

type PageToolTab = PageToolsTab["tab"];

/** Shown while a lazily imported screen chunk is being fetched. */
function ScreenLoading() {
  return (
    <div className="flex items-center justify-center h-full">
      <Spinner size={22} />
    </div>
  );
}

/**
 * One routed screen. A render crash stays inside this frame: the sidebar, the
 * toasts and the other screens keep working (the wrapper is keyed per screen,
 * so navigating away resets it).
 */
function ScreenFrame({ screen, children }: { screen: ScreenId; children: React.ReactNode }) {
  return (
    <ErrorBoundary scope={`screen:${screen}`}>
      <React.Suspense fallback={<ScreenLoading />}>{children}</React.Suspense>
    </ErrorBoundary>
  );
}

export default function App() {
  const t = useT();
  const init = useSettings((s) => s.init);
  const settings = useSettings((s) => s.settings);
  const appliedDark = useIsDarkTheme();
  const update = useSettings((s) => s.update);
  const attachJobs = useJobs((s) => s.attach);
  const refreshRecent = useRecent((s) => s.refresh);
  const setDropHandler = useDrop((s) => s.setHandler);
  const pushToast = useToasts((s) => s.push);
  const presendOpen = usePresend((s) => s.request !== null);

  const [screen, setScreen] = useState<ScreenId>("home");
  const [files, setFiles] = useState<string[]>([]);
  const [readerPage, setReaderPage] = useState<number | undefined>(undefined);
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
    if (options?.targetPage) setReaderPage(options.targetPage);
    else setReaderPage(undefined);
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

  // Keep the native window chrome in sync with the selected theme (the
  // "system" theme follows the OS live, see applyTheme in lib/store).
  useEffect(() => {
    const resolved: "dark" | "light" =
      settings.theme === "system" ? (appliedDark ? "dark" : "light") : settings.theme === "paper" ? "light" : "dark";
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(resolved))
      .catch(() => undefined);
  }, [settings.theme, appliedDark]);

  // The document language drives spell check and assistive technology.
  useEffect(() => {
    document.documentElement.lang = settings.language;
  }, [settings.language]);

  // Opens one document on the screen that can actually edit/view it: office
  // documents in the workspace, PDFs in the reader and images in the
  // image-to-PDF tool. Unknown types go to the system viewer.
  const openSinglePath = useCallback(
    (path: string) => {
      // UYAP `.udf` is closed until real UYAP files verify the round trip
      // (RELEASE_READINESS.md). A leftover file association, a share intent or
      // drag & drop can still deliver one; say so instead of failing silently.
      if (path.toLowerCase().endsWith(".udf") && !flagEnabled("experimentalUdf")) {
        recordOpenWith("route:udf-closed");
        pushToast({
          kind: "info",
          title: t("office.udfClosed"),
          detail: t("office.udfClosedHint"),
        });
        return;
      }
      const route = routeForPath(path);
      if (!route) {
        recordOpenWith("route:external");
        void openAnyFile(path).catch(() => undefined);
        return;
      }
      if (route.office) {
        recordOpenWith("route:office");
        setScreen("office");
        // The open request is asynchronous and used to be swallowed here: a
        // failed open left an empty workspace with no message at all.
        void openOfficePath(path).then((result) => {
          if (!result.ok) pushToast({ kind: "error", title: t("errors.openFailed"), detail: result.error });
        });
        return;
      }
      recordOpenWith(`route:${route.screen}`);
      setFiles(route.files ?? []);
      setScreen(route.screen);
    },
    [pushToast, t],
  );

  const openDocumentRef = useCallback(
    (ref: FileToken) =>
      openTokenDocument(ref, {
        showClosedUdf: () =>
          pushToast({ kind: "info", title: t("office.udfClosed"), detail: t("office.udfClosedHint") }),
        openOffice: (token) => {
          setScreen("office");
          void openOfficePath(token);
        },
        openScreen: (next, files) => {
          setFiles(files);
          setScreen(next);
        },
        openExternal: (token) => void openAnyFile(token).catch(() => undefined),
      }),
    [pushToast, t],
  );

  // OS drag & drop from Explorer. The highlight still comes from the webview
  // event (it carries no paths), but the drop itself is handled in Rust: the
  // window event registers the paths and emits `file-drop` with tokens only.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        setDragging(event.payload.type === "enter" || event.payload.type === "over");
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, []);

  useDropTokens({
    screen,
    openDocument: openDocumentRef,
    setFiles,
    notify: (detail) => pushToast({ kind: "info", title: t("home.quickActions"), detail }),
  });

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

  const openPickedRefs = useCallback(
    (refs: FileToken[]) => {
      if (!refs.length) return;
      const office = refs.filter((ref) => isOfficePath(ref.name));
      const documents = refs.filter((ref) => !isOfficePath(ref.name));
      if (office.length > 0) {
        setScreen("office");
        for (const ref of office) void openOfficePath(ref.token);
      }
      if (!documents.length) return;
      if (screen === "home" || office.length > 0) {
        setFiles(documents.map((ref) => ref.token));
        return;
      }
      const handler = useDrop.getState().handler;
      if (handler) handler(documents.map((ref) => ref.token));
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
    void pickDocumentTokens(extensionsFor("document"), true)
      .then(openPickedRefs)
      .catch(() => undefined);
  }, [openPickedFiles, openPickedRefs]);

  // Retry routing for background jobs: one handler per tracked kind, so a
  // failed or interrupted job can be re-run from its persisted payload even
  // when the originating screen is not mounted.
  useEffect(() => installJobRetryHandlers(), []);

  // Command platform: every screen is a command, alongside the global file,
  // palette and search actions. The keybindings registered here are the single
  // source of truth for the former hard-coded shortcut handler.
  useEffect(() => {
    // The command palette mirrors the route manifest (lib/routes.ts): one
    // list, and a flag-gated route is not registered at all.

    for (const view of enabledRoutes()) {
      registerCommand({
        id: `view.${view.id}`,
        titleKey: view.labelKey,
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
    const ids = [
      ...enabledRoutes().map((view) => `view.${view.id}`),
      "file.open",
      "app.commandPalette",
      "app.globalSearch",
    ];
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

  // Files handed to the app: the command line of this launch (Windows file
  // association, "open with") and the files later launches forward to this
  // window, because the single-instance plugin keeps one app per user.
  useLaunchTokens({
    openOffice: (tokens) => {
      setScreen("office");
      for (const token of tokens) void openOfficePath(token);
    },
    openPdfs: (tokens) => {
      setFiles(tokens);
      setScreen("reader");
    },
  });

  // Routes documents that arrived through an Android open-with intent to the
  // tool that can handle them. Unknown leftovers are handed to the system
  // viewer so nothing is silently dropped.
  const handleIncomingPaths = useCallback(
    (paths: string[]) => {
      recordOpenWith(`incoming:${paths.length}`);
      const actionToken = paths.find((p) => p.startsWith("__action:"));
      if (actionToken === "__action:scan") {
        recordOpenWith("route:scanner");
        setScreen("scanner");
        return;
      }
      if (actionToken === "__action:new_document") {
        recordOpenWith("route:office-new");
        useOfficeTabs.getState().create("writer");
        setScreen("office");
        return;
      }
      if (actionToken === "__action:recent_files") {
        recordOpenWith("route:home");
        setScreen("home");
        return;
      }
      // The Android shell queues `__error:<reason>:<name>` for a refused file;
      // say why instead of doing nothing (any real paths still open).
      const incomingError = firstIncomingError(paths);
      if (incomingError) pushToast({ kind: "error", title: t(incomingErrorKey(incomingError.reason)) });
      const realPaths = paths.filter((p) => !p.startsWith("__action:") && !p.startsWith("__error:"));
      if (realPaths.length === 0) {
        recordOpenWith("route:none");
        return;
      }
      if (realPaths.length === 1) {
        openSinglePath(realPaths[0]);
        return;
      }
      const officePaths = realPaths.filter((path) => isOfficePath(path));
      if (officePaths.length > 0) {
        recordOpenWith(`route:office:${officePaths.length}`);
        setScreen("office");
        for (const path of officePaths) {
          void openOfficePath(path).then((result) => {
            if (!result.ok) pushToast({ kind: "error", title: t("errors.openFailed"), detail: result.error });
          });
        }
        return;
      }
      const pdfPaths = realPaths.filter((path) => path.toLowerCase().endsWith(".pdf"));
      if (pdfPaths.length > 0) {
        recordOpenWith(`route:reader:${pdfPaths.length}`);
        setFiles(pdfPaths);
        setScreen("reader");
        return;
      }
      const imagePaths = realPaths.filter((path) => isImage(path));
      if (imagePaths.length > 0) {
        recordOpenWith(`route:imagesToPdf:${imagePaths.length}`);
        setFiles(imagePaths);
        setScreen("imagesToPdf");
        return;
      }
      recordOpenWith(`route:external:${realPaths.length}`);
      for (const path of realPaths) void openAnyFile(path).catch(() => undefined);
    },
    [openSinglePath, pushToast, t],
  );

  // Android open-with: the poll and its trace live in lib/android-open-with.ts,
  // which also records the drained counts and refusal tokens.
  useAndroidPendingOpen(handleIncomingPaths);

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
      reader: <Reader initialFiles={files} dragging={dragging} initialPage={readerPage} />,
      ai: <Ai initialFiles={files} dragging={dragging} onNavigate={navigate} />,
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
      templates: <TemplatesScreen onOpen={() => navigate("office")} />,
      converter: <ConverterScreen />,
      invoices: <InvoicesScreen initialFiles={files} />,
      eml: <EmlScreen initialFiles={files} onNavigate={navigate} />,
      scanner: <ScannerScreen onNavigate={navigate} />,
      recipes: <RecipesScreen />,
      mailMerge: <MailMergeScreen />,
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
      petition: <PetitionWizardScreen onNavigate={navigate} />,
      deadline: <DeadlineCalculatorScreen initialFiles={files} dragging={dragging} />,
      evidence: <EvidenceBundleScreen initialFiles={files} dragging={dragging} />,
      kvkk: <KvkkWizardScreen onNavigate={navigate} />,
      documentHealth: <DocumentHealthScreen onNavigate={navigate} />,
    }),
    [convertTab, dragging, files, homeDrop, navigate, pageToolTab, readerPage, securityTab],
  );

  /** Icons are presentation, not routing: the manifest owns the ids. */
  const NAV_ICONS: Record<ScreenId, React.ReactElement> = {
    home: <Home size={16} />,
    office: <LayoutGrid size={16} />,
    reader: <BookOpen size={16} />,
    documents: <FileText size={16} />,
    spreadsheets: <FileSpreadsheet size={16} />,
    presentations: <Presentation size={16} />,
    templates: <LayoutTemplate size={16} />,
    petition: <FilePlus2 size={16} />,
    deadline: <CalendarClock size={16} />,
    evidence: <FolderCheck size={16} />,
    kvkk: <ShieldCheck size={16} />,
    notes: <NotebookPen size={16} />,
    planner: <CalendarDays size={16} />,
    data: <Database size={16} />,
    draw: <PenTool size={16} />,
    vault: <FileSearch size={16} />,
    converter: <Repeat size={16} />,
    invoices: <Receipt size={16} />,
    eml: <Mail size={16} />,
    scanner: <Camera size={16} />,
    recipes: <ListChecks size={16} />,
    mailMerge: <Mail size={16} />,
    cleaner: <Sparkles size={16} />,
    pdfForms: <ClipboardList size={16} />,
    merge: <Combine size={16} />,
    organize: <Layers size={16} />,
    split: <Scissors size={16} />,
    compress: <Minimize2 size={16} />,
    pageTools: <Wand2 size={16} />,
    pdfStudio: <ShieldCheck size={16} />,
    documentHealth: <ShieldCheck size={16} />,
    watermark: <Stamp size={16} />,
    annotate: <Type size={16} />,
    redact: <Eraser size={16} />,
    compare: <FileDiff size={16} />,
    inspect: <FileSearch size={16} />,
    metadata: <Puzzle size={16} />,
    pdfToImages: <FileImage size={16} />,
    imagesToPdf: <Images size={16} />,
    ocr: <FileSearch size={16} />,
    protect: <Lock size={16} />,
    unlock: <LockOpen size={16} />,
    ai: <Bot size={16} />,
    aiLibrary: <Library size={16} />,
    batch: <Archive size={16} />,
    jobs: <ListChecks size={16} />,
    sync: <Repeat size={16} />,
    plugins: <Puzzle size={16} />,
    compat: <ShieldCheck size={16} />,
    info: <Info size={16} />,
    history: <FolderClock size={16} />,
    settings: <SettingsIcon size={16} />,
  };

  // Sidebar entries come from the same manifest, grouped by section.
  const navGroups = navSections().map((section) => ({
    label: section.labelKey ? t(section.labelKey) : undefined,
    items: section.routes.map((item) => ({
      id: item.id,
      label: t(item.labelKey),
      icon: NAV_ICONS[item.id],
    })),
  }));

  // Flag-gated routes are closed, not just hidden: a deep link to a disabled
  // route renders Home instead of the unfinished screen.
  const effectiveScreen = routeEnabled(screen) ? screen : "home";

  const isDark = settings.theme === "dark" || (settings.theme === "system" && appliedDark);

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
                <BrandMark size={36} className="shrink-0" />
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
            <ScreenFrame screen={effectiveScreen}>{screens[effectiveScreen]}</ScreenFrame>
          </div>
        </main>

        {/* Thumb-reach navigation on phones (C11); More opens the full drawer. */}
        <React.Suspense fallback={<div className="bottom-nav bottom-nav-placeholder" aria-hidden />}>
          <BottomNav screen={screen} onNavigate={navigate} onMore={() => setNavOpen(true)} />
        </React.Suspense>

        <Toasts />
        <div className="sr-only" role="status" aria-live="polite">
          {backgroundJobs > 0 ? t("jobs.runningCount", { count: backgroundJobs }) : ""}
        </div>
        <OverwriteDialog />
        {presendOpen ? (
          <React.Suspense fallback={null}>
            <PresendDialog onNavigate={navigate} />
          </React.Suspense>
        ) : null}
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
          onOpenRecent={(entry) => openDocumentRef({ token: entry.token, name: entry.fileName })}
          onNavigate={(next) => setScreen(next as ScreenId)}
        />
      </div>
    );
  }

  return (
    <div className="flex h-full" style={{ background: "var(--bg)" }}>
      {/* Sidebar */}
      <aside
        className="sidebar flex flex-col shrink-0 border-r"
        style={{
          width: sidebarCompact ? 64 : 232,
          borderColor: "var(--border)",
          transition: "width 0.15s ease",
        }}
      >
        <div className="flex items-center gap-2.5 px-3.5 py-4">
          <BrandMark size={36} className="shrink-0" />
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
          <ScreenFrame screen={effectiveScreen}>{screens[effectiveScreen]}</ScreenFrame>
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
        onOpenRecent={(entry) => openDocumentRef({ token: entry.token, name: entry.fileName })}
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
  // UYAP `.udf` is experimental and only listed with the flag on.
  writer: ["docx", "odt", "rtf", "txt", "md", ...(flagEnabled("experimentalUdf") ? ["udf"] : [])],
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
    // The shared opener also handles legacy imports, the conflict fingerprint
    // and the recent list, which a direct openDocument call skipped.
    const result = await openOfficePath(path);
    if (result.ok) {
      onOpen();
    } else {
      useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: result.error });
    }
  };

  const related = recent.filter((entry) =>
    OFFICE_EXTENSIONS[kind].includes((entry.fileName.split(".").pop() ?? "").toLowerCase()),
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
            <button
              key={entry.token}
              type="button"
              className="row recent-row"
              onClick={() => void openPath(entry.token)}
            >
              <span className="grow">{entry.fileName}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
