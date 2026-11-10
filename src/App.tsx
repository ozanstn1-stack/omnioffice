import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { pickDocumentTokens, type FileToken } from "./lib/file-tokens";
import { extensionsFor } from "./lib/tool-files";
import { openTokenDocument } from "./lib/file-refs";
import { firstIncomingError, incomingErrorKey } from "./lib/incoming";
import { useDropTokens, useLaunchTokens } from "./lib/use-file-ref-events";
import { Lock, Menu, Moon, Sun } from "lucide-react";
import { useT } from "./lib/i18n";
import { NAV_ICONS } from "./lib/nav-icons";
import { GlobalOverlays } from "./components/global-overlays";
import type { RecentEntry } from "./lib/types";
import { useDev, useDrop, useIsDarkTheme, useJobs, useRecent, useSettings, useToasts } from "./lib/store";
import packageJson from "../package.json";
import { devLaunchContext } from "./lib/api";
import type { Navigate, PageToolsTab, ScreenId } from "./lib/nav";
import { flagEnabled } from "./lib/flags";
import { useFocusScrollIntoView, useKeyboardInset, useNarrowLayout } from "./lib/viewport";
import { enabledRoutes, routeEnabled, routeFor } from "./lib/routes";
import type { TaskPackId } from "./lib/ai-task-packs";
import * as Screens from "./screens/registry";
import { matchKeybinding, registerCommand, runCommand, setCommandTranslator, unregisterCommand } from "./lib/commands";
import { resumeJobs, useJobs as useBackgroundJobs } from "./lib/jobs";
import { installJobRetryHandlers } from "./lib/job-retries";
import { useDeskTaskCommands } from "./lib/use-desk-task-commands";
import { usePresend } from "./lib/presend-gate";
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
  const [aiPack, setAiPack] = useState<TaskPackId | null>(null);
  const [readerPage, setReaderPage] = useState<number | undefined>(undefined);
  const [pageToolTab, setPageToolTab] = useState<PageToolTab>("extract");
  const [convertTab, setConvertTab] = useState<"pdfToImages" | "imagesToPdf">("pdfToImages");
  const [securityTab, setSecurityTab] = useState<"protect" | "unlock">("protect");
  const [dragging, setDragging] = useState(false);
  const [sidebarCompact, setSidebarCompact] = useState(false);
  const [navOpen, setNavOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const narrow = useNarrowLayout();
  const backgroundJobs = useBackgroundJobs((state) => state.jobs.filter((job) => job.status === "running").length);
  // WebView history bridge for the Android back button (see lib/nav-history).
  const navSnapshotRef = useRef<NavigationSnapshot>({ screen: "home", files: [] });
  const overlayGuardRef = useRef(false);

  // Phones always use the drawer navigation; desktop windows switch to it
  // when they get narrow enough for the sidebar to waste space.
  const compactNav = isAndroid() || narrow;

  // Phone ergonomics (D8): publish the keyboard inset for the bottom action
  // bar and scroll a focused field clear of the keyboard.
  useKeyboardInset();
  useFocusScrollIntoView();

  const navigate = useCallback<Navigate>((next, options) => {
    setFiles(options?.files ?? []);
    setAiPack(options?.pack ?? null);
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

  // Desk tasks (D13): the palette runs a task with the current file selection.
  useDeskTaskCommands(files, navigate);

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
      home: (
        <Screens.HomeScreen onNavigate={navigate} onDropFiles={homeDrop} dragging={dragging} onFileList={setFiles} />
      ),
      reader: <Screens.Reader initialFiles={files} dragging={dragging} initialPage={readerPage} />,
      ai: <Screens.Ai initialFiles={files} dragging={dragging} onNavigate={navigate} />,
      aiLibrary: <Screens.AiLibrary onOpenAi={() => navigate("ai")} />,
      // A desk task (D13) navigates with its pack; the key remounts the screen so the
      // preselection applies even when the user is already on it.
      aiPacks: <Screens.AiPacks key={aiPack ?? "ai-packs"} initialPack={aiPack ?? undefined} initialFiles={files} />,
      merge: <Screens.Merge initialFiles={files} dragging={dragging} />,
      organize: <Screens.Organize initialFiles={files} dragging={dragging} />,
      split: <Screens.Split initialFiles={files} dragging={dragging} />,
      compress: <Screens.Compress initialFiles={files} dragging={dragging} />,
      ocr: <Screens.Ocr initialFiles={files} dragging={dragging} />,
      pdfToImages: <Screens.Convert tab={convertTab} initialFiles={files} dragging={dragging} />,
      imagesToPdf: <Screens.Convert tab={convertTab} initialFiles={files} dragging={dragging} />,
      protect: <Screens.Security tab={securityTab} initialFiles={files} dragging={dragging} />,
      unlock: <Screens.Security tab={securityTab} initialFiles={files} dragging={dragging} />,
      watermark: <Screens.Watermark initialFiles={files} dragging={dragging} />,
      annotate: <Screens.Annotate initialFiles={files} dragging={dragging} />,
      redact: <Screens.Redact initialFiles={files} dragging={dragging} />,
      compare: <Screens.Compare initialFiles={files} dragging={dragging} />,
      inspect: <Screens.Inspect initialFiles={files} dragging={dragging} />,
      metadata: <Screens.Metadata initialFiles={files} dragging={dragging} />,
      pageTools: <Screens.PageTools tab={pageToolTab} initialFiles={files} dragging={dragging} />,
      batch: <Screens.Batch initialFiles={files} dragging={dragging} />,
      history: <Screens.History onNavigate={navigate} />,
      settings: <Screens.Settings />,
      info: <Screens.InfoScreen initialFiles={files} />,
      office: <Screens.OfficeWorkspace />,
      documents: (
        <Screens.OfficeLauncher
          kind="writer"
          onOpen={() => navigate("office")}
          onTemplates={() => navigate("templates")}
        />
      ),
      spreadsheets: (
        <Screens.OfficeLauncher
          kind="calc"
          onOpen={() => navigate("office")}
          onTemplates={() => navigate("templates")}
        />
      ),
      presentations: (
        <Screens.OfficeLauncher
          kind="impress"
          onOpen={() => navigate("office")}
          onTemplates={() => navigate("templates")}
        />
      ),
      notes: <Screens.NotesScreen />,
      templates: <Screens.TemplatesScreen onOpen={() => navigate("office")} />,
      converter: <Screens.ConverterScreen />,
      invoices: <Screens.InvoicesScreen initialFiles={files} />,
      eml: <Screens.EmlScreen initialFiles={files} onNavigate={navigate} />,
      scanner: <Screens.ScannerScreen onNavigate={navigate} initialFiles={files} />,
      recipes: <Screens.RecipesScreen />,
      mailMerge: <Screens.MailMergeScreen />,
      cleaner: <Screens.CleanerScreen />,
      draw: <Screens.DrawScreen />,
      planner: <Screens.PlannerScreen />,
      data: <Screens.DataScreen />,
      pdfForms: <Screens.PdfFormsScreen />,
      vault: <Screens.Vault initialFiles={files} />,
      pdfStudio: <Screens.PdfStudio initialFiles={files} dragging={dragging} />,
      compat: <Screens.CompatibilityScreen />,
      jobs: <Screens.JobsScreen />,
      sync: <Screens.Sync />,
      plugins: <Screens.Plugins />,
      petition: <Screens.PetitionWizardScreen onNavigate={navigate} />,
      officialLetter: <Screens.OfficialLetterScreen onNavigate={navigate} />,
      deadline: <Screens.DeadlineCalculatorScreen initialFiles={files} dragging={dragging} />,
      evidence: <Screens.EvidenceBundleScreen initialFiles={files} dragging={dragging} />,
      kvkk: <Screens.KvkkWizardScreen onNavigate={navigate} />,
      documentHealth: <Screens.DocumentHealthScreen onNavigate={navigate} />,
      verify: <Screens.VerifyScreen initialFiles={files} dragging={dragging} />,
      tools: <Screens.ToolsScreen onNavigate={navigate} />,
    }),
    [aiPack, convertTab, dragging, files, homeDrop, navigate, pageToolTab, readerPage, securityTab],
  );

  // Flag-gated routes are closed, not just hidden: a deep link to a disabled
  // route renders Home instead of the unfinished screen.
  const effectiveScreen = routeEnabled(screen) ? screen : "home";

  const isDark = settings.theme === "dark" || (settings.theme === "system" && appliedDark);

  const pickNav = (id: ScreenId) => {
    navigate(id);
    setNavOpen(false);
  };

  const activeLabel = routeEnabled(screen) ? t(routeFor(screen).labelKey) : t("app.name");

  const overlayProps = {
    screen,
    backgroundJobs,
    paletteOpen,
    onClosePalette: () => setPaletteOpen(false),
    searchOpen,
    onCloseSearch: () => setSearchOpen(false),
    onNavigate: (next: string) => setScreen(next as ScreenId),
    onOpenPath: openAnyPath,
    onOpenRecent: (entry: RecentEntry) => openDocumentRef({ token: entry.token, name: entry.fileName }),
  };

  // Phones (and narrow desktop windows): drawer navigation with a top bar.
  if (compactNav) {
    return (
      <div className="flex flex-col h-full" style={{ background: "var(--bg)" }}>
        <header className="mobile-bar">
          <IconButton label={t("app.name")} onClick={() => setNavOpen(true)} aria-expanded={navOpen}>
            <Menu size={18} />
          </IconButton>
          <p className="font-bold text-[17px] truncate flex-1">{activeLabel}</p>
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
              <React.Suspense fallback={<div className="flex-1" />}>
                <Screens.DrawerNav icons={NAV_ICONS} screen={screen} onPick={pickNav} />
              </React.Suspense>
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
          <Screens.BottomNav screen={screen} onNavigate={navigate} onMore={() => setNavOpen(true)} />
        </React.Suspense>

        <GlobalOverlays
          {...overlayProps}
          presend={
            presendOpen ? (
              <React.Suspense fallback={null}>
                <Screens.PresendDialog onNavigate={navigate} />
              </React.Suspense>
            ) : null
          }
        />
      </div>
    );
  }

  return (
    <div className="flex h-full" style={{ background: "var(--bg)" }}>
      <React.Suspense fallback={<aside className="sidebar shrink-0" style={{ width: sidebarCompact ? 68 : 252 }} />}>
        <Screens.DesktopSidebar
          icons={NAV_ICONS}
          screen={screen}
          compact={sidebarCompact}
          version={packageJson.version}
          backgroundJobs={backgroundJobs}
          isDark={isDark}
          onNavigate={pickNav}
          onSearch={() => setSearchOpen(true)}
          onToggleTheme={() => void update({ theme: isDark ? "light" : "dark" })}
          onToggleCompact={() => setSidebarCompact((previous) => !previous)}
        />
      </React.Suspense>

      {/* Main area */}
      <main className="app-main flex-1 min-w-0 relative">
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

      <GlobalOverlays {...overlayProps} />
    </div>
  );
}
