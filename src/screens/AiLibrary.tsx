import { useCallback, useEffect, useMemo, useState } from "react";
import {
  BookOpenCheck,
  Clock,
  Download,
  FolderOpen,
  Languages,
  MessageCircleQuestion,
  RefreshCw,
  Search,
  Sparkles,
  Trash2,
  Wand2,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { isAndroid, openAnyFile, revealAnyFile, saveFileOnAndroid } from "../lib/mobile";
import { Badge, Button, Card, EmptyState, IconButton, Spinner, TextInput } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import {
  aiLibraryClear,
  aiLibraryDefaultDir,
  aiLibraryDelete,
  aiLibraryExport,
  aiLibraryList,
  aiLibraryText,
} from "../lib/api";
import { formatDate } from "../lib/format";
import { useSettings, useToasts } from "../lib/store";
import type { AiLibraryEntry } from "../lib/types";

const KIND_ICONS: Record<string, React.ReactNode> = {
  summary: <Sparkles size={14} />,
  translate: <Languages size={14} />,
  ask: <MessageCircleQuestion size={14} />,
  cleanup: <Wand2 size={14} />,
  metadata: <BookOpenCheck size={14} />,
};

export function AiLibrary({ onOpenAi }: { onOpenAi: () => void }) {
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const pushToast = useToasts((s) => s.push);
  const [entries, setEntries] = useState<AiLibraryEntry[] | null>(null);
  const [filter, setFilter] = useState<string>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<AiLibraryEntry | null>(null);
  const [preview, setPreview] = useState<string>("");
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [directory, setDirectory] = useState("");

  const refresh = useCallback(async () => {
    try {
      const list = await aiLibraryList();
      setEntries(list);
    } catch {
      setEntries([]);
    }
  }, []);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- async loaders own the list/directory state
    void refresh();
    void aiLibraryDefaultDir()
      .then((value) => setDirectory(settings.aiLibraryDir || value))
      .catch(() => undefined);
  }, [refresh, settings.aiLibraryDir]);

  const kinds = useMemo(() => {
    const set = new Set((entries ?? []).map((entry) => entry.kind));
    return ["all", ...Array.from(set)];
  }, [entries]);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (entries ?? []).filter((entry) => {
      if (filter !== "all" && entry.kind !== filter) return false;
      if (!needle) return true;
      return (
        entry.preview.toLowerCase().includes(needle) ||
        entry.sourceName.toLowerCase().includes(needle) ||
        entry.options.toLowerCase().includes(needle)
      );
    });
  }, [entries, filter, query]);

  const openEntry = async (entry: AiLibraryEntry) => {
    setSelected(entry);
    setLoadingPreview(true);
    try {
      setPreview(await aiLibraryText(entry.id));
    } catch {
      setPreview("");
    } finally {
      setLoadingPreview(false);
    }
  };

  const exportEntry = async (entry: AiLibraryEntry) => {
    const defaultName = entry.sourceName.replace(/\.pdf$/i, "") + `_${entry.kind}.md`;
    try {
      if (isAndroid()) {
        const saved = await saveFileOnAndroid(entry.filePath, defaultName);
        if (!saved) return;
        pushToast({ kind: "success", title: t("library.exported"), detail: saved });
        return;
      }
      const picked = await saveDialog({
        title: t("common.save"),
        defaultPath: defaultName,
        filters: [
          { name: "Markdown", extensions: ["md"] },
          { name: "Text", extensions: ["txt"] },
        ],
      });
      if (!picked) return;
      const path = await aiLibraryExport(entry.id, String(picked));
      pushToast({ kind: "success", title: t("library.exported"), detail: path });
    } catch (error) {
      pushToast({
        kind: "error",
        title: t("errors.title"),
        detail: String((error as { message?: string })?.message ?? error),
      });
    }
  };

  const removeEntry = async (entry: AiLibraryEntry) => {
    const list = await aiLibraryDelete(entry.id, true);
    setEntries(list);
    if (selected?.id === entry.id) {
      setSelected(null);
      setPreview("");
    }
  };

  const pickDirectory = async () => {
    if (isAndroid()) return;
    const picked = await openDialog({ directory: true, multiple: false, title: t("library.chooseFolder") });
    if (picked) setDirectory(String(picked));
  };

  return (
    <Screen
      title={t("library.title")}
      subtitle={t("library.subtitle")}
      actions={
        <div className="flex items-center gap-2">
          {!isAndroid() ? (
            <Button
              size="sm"
              variant="ghost"
              icon={<FolderOpen size={14} />}
              onClick={() => void openAnyFile(directory).catch(() => undefined)}
              disabled={!directory}
            >
              {t("common.openFolder")}
            </Button>
          ) : null}
          <Button size="sm" variant="ghost" icon={<Sparkles size={14} />} onClick={onOpenAi}>
            {t("nav.ai")}
          </Button>
          <IconButton label={t("common.retry")} onClick={() => void refresh()}>
            <RefreshCw size={15} />
          </IconButton>
        </div>
      }
    >
      <Card className="p-4 flex flex-wrap items-center gap-3">
        <span className="text-xs muted">
          {t("library.folder")}: <strong className="text-[var(--text)]">{directory || "—"}</strong>
        </span>
        {!isAndroid() ? (
          <Button size="sm" variant="ghost" onClick={() => void pickDirectory()}>
            {t("library.changeFolder")}
          </Button>
        ) : null}
        <span className="ml-auto flex items-center gap-2">
          <div className="relative">
            <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 muted" />
            <TextInput
              className="input-sm pl-8"
              style={{ width: 220 }}
              placeholder={t("library.searchPlaceholder")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <Button
            size="sm"
            variant="danger"
            icon={<Trash2 size={14} />}
            disabled={!entries?.length}
            onClick={() => {
              void aiLibraryClear(true).then(() => {
                setEntries([]);
                setSelected(null);
                setPreview("");
                pushToast({ kind: "success", title: t("library.cleared") });
              });
            }}
          >
            {t("library.clearAll")}
          </Button>
        </span>
      </Card>

      <div className="flex flex-wrap gap-1.5">
        {kinds.map((kind) => (
          <Button
            key={kind}
            size="sm"
            variant={filter === kind ? "primary" : "default"}
            onClick={() => setFilter(kind)}
          >
            {kind === "all" ? t("common.all") : t(`library.kind.${kind}`)}
            {kind !== "all" ? ` (${(entries ?? []).filter((entry) => entry.kind === kind).length})` : ""}
          </Button>
        ))}
      </div>

      <div className="grid gap-4" style={{ gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1.1fr)" }}>
        <div className="flex flex-col gap-2">
          {entries === null ? (
            <Card className="p-6 flex justify-center">
              <Spinner size={20} />
            </Card>
          ) : visible.length === 0 ? (
            <Card>
              <EmptyState
                icon={<Sparkles size={22} />}
                title={t("library.empty")}
                hint={t("library.emptyHint")}
                action={
                  <Button variant="primary" onClick={onOpenAi}>
                    {t("ai.run")}
                  </Button>
                }
              />
            </Card>
          ) : (
            visible.map((entry) => (
              <button
                key={entry.id}
                className={`card p-3 text-left flex flex-col gap-1.5 hover:border-[var(--accent)] ${selected?.id === entry.id ? "border-[var(--accent)]" : ""}`}
                onClick={() => void openEntry(entry)}
              >
                <span className="flex items-center gap-2">
                  <Badge tone="accent">
                    {KIND_ICONS[entry.kind] ?? <Sparkles size={12} />} {t(`library.kind.${entry.kind}`)}
                  </Badge>
                  <span className="text-[11.5px] muted flex items-center gap-1">
                    <Clock size={11} /> {formatDate(entry.createdAt)}
                  </span>
                  <span className="text-[11.5px] muted ml-auto">{entry.model}</span>
                </span>
                <span className="text-[13.5px] font-medium truncate" title={entry.sourcePath}>
                  {entry.sourceName}
                </span>
                <span
                  className="text-xs muted line-clamp-2"
                  style={{
                    display: "-webkit-box",
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: "vertical",
                    overflow: "hidden",
                  }}
                >
                  {entry.preview}
                </span>
                <span className="text-[11px] muted">
                  {entry.pages} {t("common.pages")} · {entry.characters} {t("library.chars")}
                  {entry.options ? ` · ${entry.options}` : ""}
                </span>
              </button>
            ))
          )}
        </div>

        <div>
          {selected ? (
            <Card className="p-4 flex flex-col gap-3 sticky top-2">
              <div className="flex items-center gap-2 flex-wrap">
                <Badge tone="accent">{t(`library.kind.${selected.kind}`)}</Badge>
                <span className="text-[13px] font-medium truncate max-w-[220px]" title={selected.sourcePath}>
                  {selected.sourceName}
                </span>
                <span className="ml-auto flex items-center gap-1">
                  <Button size="sm" icon={<Download size={14} />} onClick={() => void exportEntry(selected)}>
                    {t("library.exportAs")}
                  </Button>
                  <IconButton
                    label={t("common.openFile")}
                    onClick={() => void openAnyFile(selected.filePath).catch(() => undefined)}
                  >
                    <BookOpenCheck size={15} />
                  </IconButton>
                  <IconButton
                    label={isAndroid() ? t("common.share") : t("common.openFolder")}
                    onClick={() => void revealAnyFile(selected.filePath).catch(() => undefined)}
                  >
                    <FolderOpen size={15} />
                  </IconButton>
                  <IconButton label={t("common.delete")} onClick={() => void removeEntry(selected)}>
                    <Trash2 size={15} />
                  </IconButton>
                </span>
              </div>
              <div className="text-[11.5px] muted flex flex-wrap gap-x-4 gap-y-1">
                <span>
                  {t("library.savedAt")}: {formatDate(selected.createdAt)}
                </span>
                <span>{selected.model}</span>
                <span>
                  {selected.pages} {t("common.pages")} · {selected.characters} {t("library.chars")}
                </span>
                {selected.options ? <span>{selected.options}</span> : null}
                {selected.elapsedMs ? <span>{Math.round(selected.elapsedMs / 100) / 10}s</span> : null}
              </div>
              <div className="card-soft p-3 max-h-[520px] overflow-y-auto whitespace-pre-wrap text-[13px] leading-relaxed">
                {loadingPreview ? <Spinner size={16} /> : preview || t("library.emptyPreview")}
              </div>
              <p className="text-xs muted">
                {t("library.filePath")}: {selected.filePath}
              </p>
            </Card>
          ) : (
            <Card className="p-6 text-sm muted">{t("library.selectHint")}</Card>
          )}
        </div>
      </div>
    </Screen>
  );
}
