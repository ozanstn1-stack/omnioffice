import { useEffect, useMemo, useState } from "react";
import {
  BookOpen,
  Combine,
  Download,
  Files,
  FolderClock,
  Hash,
  Home,
  Images,
  Layers,
  Minimize2,
  Moon,
  Puzzle,
  Scissors,
  Stamp,
  Sun,
  Wand2,
} from "lucide-react";
import { Badge, Button, IconButton, Modal, Spinner } from "./components/ui";
import { ScreenId } from "./lib/nav";
import { HomeScreen } from "./screens/Home";
import { ReaderScreen } from "./screens/Reader";
import { MergeScreen } from "./screens/Merge";
import { OrganizeScreen } from "./screens/Organize";
import { SplitScreen } from "./screens/Split";
import { PageOpsScreen } from "./screens/PageOps";
import { CompressScreen } from "./screens/Compress";
import { WatermarkScreen } from "./screens/Watermark";
import { NumberingScreen } from "./screens/Numbering";
import { MetadataScreen } from "./screens/Metadata";
import { ConvertScreen } from "./screens/Convert";

import { setPendingFiles } from "./lib/pending";
import { openRemoteLink, parseRemoteLink, type RemoteLink } from "./lib/remotelink";

export default function App() {
  const [screen, setScreen] = useState<ScreenId>("home");
  const [dark, setDark] = useState(() => localStorage.getItem("pdfsak-theme") !== "light");
  const [version, setVersion] = useState("1.0.1");
  const [remoteLink, setRemoteLink] = useState<RemoteLink | null>(null);
  const [remoteBusy, setRemoteBusy] = useState(false);
  const [remoteError, setRemoteError] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    localStorage.setItem("pdfsak-theme", dark ? "dark" : "light");
  }, [dark]);

  useEffect(() => {
    const manifest = typeof chrome !== "undefined" && chrome.runtime?.getManifest ? chrome.runtime.getManifest() : null;
    if (manifest?.version) setVersion(manifest.version);
  }, []);

  const navigate = (next: ScreenId, files?: File[]) => {
    if (files?.length) setPendingFiles(files);
    setScreen(next);
  };

  // Context-menu deep link (`app.html#url=...`, written by background.js).
  // The tab is often reused, so the hash can change without a reload: listen
  // for both the initial value and later hashchange events.
  useEffect(() => {
    const read = () => {
      const link = parseRemoteLink(location.hash);
      // A changed hash replaces the prompt: an invalid/removed link must never
      // leave a stale consent dialog for a different URL on screen.
      setRemoteLink(link);
      if (link) setRemoteError(null);
    };
    read();
    window.addEventListener("hashchange", read);
    return () => window.removeEventListener("hashchange", read);
  }, []);

  const requestOriginPermission = (origin: string): Promise<boolean> => {
    const permissions = typeof chrome !== "undefined" ? chrome.permissions : undefined;
    if (!permissions?.request) return Promise.resolve(true);
    return new Promise((resolve) => {
      try {
        permissions.request({ origins: [origin] }, (granted) => resolve(Boolean(granted)));
      } catch {
        resolve(false);
      }
    });
  };

  const openRemote = async () => {
    if (!remoteLink) return;
    setRemoteBusy(true);
    setRemoteError(null);
    try {
      await openRemoteLink(remoteLink, { requestPermission: requestOriginPermission });
      setRemoteLink(null);
      setScreen("reader");
    } catch (error) {
      setRemoteError(error instanceof Error ? error.message : String(error));
    } finally {
      setRemoteBusy(false);
    }
  };

  // Development hook (used by the automated browser validation):
  //   app.html?demo=1&screen=reader   generates a sample PDF and opens it.
  useEffect(() => {
    const params = new URLSearchParams(location.search);
    if (!params.has("demo")) return;
    const target = (params.get("screen") as ScreenId | null) ?? "reader";
    void import("./lib/demo").then(async ({ makeDemoPdf }) => {
      const file = await makeDemoPdf(5, params.get("label") ?? "Demo document");
      setPendingFiles([file]);
      setScreen(target);
    });
  }, []);

  const screens: Record<ScreenId, React.ReactElement> = useMemo(
    () => ({
      home: <HomeScreen onNavigate={navigate} />,
      reader: <ReaderScreen />,
      merge: <MergeScreen />,
      organize: <OrganizeScreen />,
      split: <SplitScreen />,
      compress: <CompressScreen />,
      convert: <ConvertScreen />,
      pageops: <PageOpsScreen />,
      watermark: <WatermarkScreen />,
      numbering: <NumberingScreen />,
      metadata: <MetadataScreen />,
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const groups: { label?: string; items: { id: ScreenId; label: string; icon: React.ReactElement }[] }[] = [
    {
      items: [
        { id: "home", label: "Home", icon: <Home size={16} /> },
        { id: "reader", label: "Read", icon: <BookOpen size={16} /> },
      ],
    },
    {
      label: "PDF tools",
      items: [
        { id: "merge", label: "Merge", icon: <Combine size={16} /> },
        { id: "organize", label: "Organize Pages", icon: <Layers size={16} /> },
        { id: "split", label: "Split", icon: <Scissors size={16} /> },
        { id: "pageops", label: "Page Tools", icon: <Wand2 size={16} /> },
        { id: "compress", label: "Compress", icon: <Minimize2 size={16} /> },
        { id: "watermark", label: "Watermark", icon: <Stamp size={16} /> },
        { id: "numbering", label: "Page Numbers", icon: <Hash size={16} /> },
        { id: "metadata", label: "Metadata", icon: <Files size={16} /> },
      ],
    },
    {
      label: "Convert",
      items: [{ id: "convert", label: "PDF ↔ Images", icon: <Images size={16} /> }],
    },
  ];

  return (
    <div className="flex h-full" style={{ background: "var(--bg)" }}>
      <aside className="flex flex-col shrink-0 border-r" style={{ width: 224, borderColor: "var(--border)", background: "var(--bg-soft)" }}>
        <div className="flex items-center gap-2.5 px-3.5 py-4">
          <div className="w-9 h-9 rounded-xl flex items-center justify-center shrink-0" style={{ background: "var(--accent)", color: "var(--accent-text)" }}>
            <Puzzle size={18} />
          </div>
          <div className="min-w-0">
            <p className="font-bold text-[13.5px] leading-tight truncate">PDF Swiss Army Knife</p>
            <p className="text-[11px] muted truncate">v{version} · browser</p>
          </div>
        </div>

        <nav className="flex-1 overflow-y-auto px-2 pb-3">
          {groups.map((group, index) => (
            <div key={index}>
              {group.label ? <p className="nav-group-label">{group.label}</p> : null}
              {group.items.map((item) => (
                <button key={item.id} className="nav-item" data-active={screen === item.id} onClick={() => navigate(item.id)}>
                  <span className="shrink-0">{item.icon}</span>
                  <span className="truncate">{item.label}</span>
                </button>
              ))}
            </div>
          ))}
        </nav>

        <div className="px-3 py-3 border-t flex items-center justify-between" style={{ borderColor: "var(--border)" }}>
          <Badge tone="ok">offline</Badge>
          <div className="flex items-center gap-1">
            <IconButton label="Toggle theme" onClick={() => setDark((previous) => !previous)}>
              {dark ? <Sun size={15} /> : <Moon size={15} />}
            </IconButton>
            <IconButton label="Extensions" onClick={() => window.open("chrome://extensions", "_blank")}>
              <FolderClock size={15} />
            </IconButton>
          </div>
        </div>
      </aside>

      <main className="flex-1 min-w-0 h-full relative">
        <div key={screen} className="h-full">
          {screens[screen]}
        </div>
      </main>

      {remoteLink ? (
        <Modal
          title="Open linked PDF?"
          onClose={() => setRemoteLink(null)}
          footer={
            <>
              <Button onClick={() => setRemoteLink(null)}>Cancel</Button>
              <Button
                variant="primary"
                onClick={() => void openRemote()}
                disabled={remoteBusy}
                icon={remoteBusy ? <Spinner size={14} /> : <Download size={14} />}
                data-testid="remote-link-open"
              >
                Download and open
              </Button>
            </>
          }
        >
          <div data-testid="remote-link-banner">
            <p className="text-[13px]">
              The browser passed a document from the context menu. It is downloaded once, opened locally and never uploaded anywhere.
            </p>
            <p className="text-xs muted mt-2 break-all" data-testid="remote-link-host">
              {remoteLink.host}
            </p>
            <p className="text-xs muted break-all">{remoteLink.url}</p>
            {remoteError ? (
              <p className="text-xs mt-2" style={{ color: "var(--danger, #b91c1c)" }} data-testid="remote-link-error">
                {remoteError}
              </p>
            ) : null}
          </div>
        </Modal>
      ) : null}
    </div>
  );
}
