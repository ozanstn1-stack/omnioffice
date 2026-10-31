/**
 * Command palette (Ctrl+Shift+P) and global search (Ctrl+Shift+F).
 *
 * Both are thin shells over the command registry (`src/lib/commands.ts`) and
 * the local stores; they never invent actions - every row runs a real command
 * or opens a real file.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import type { VaultSearchHit, VaultSearchRequest_Deserialize } from "../lib/generated/specta";
import { call } from "../lib/typed-call";
import { Command as CommandIcon, FileText, Search } from "lucide-react";
import {
  isCommandEnabled,
  runCommand,
  searchCommands,
  type CommandDefinition,
  type CommandContext,
} from "../lib/commands";
import { errorMessage, useRecent } from "../lib/store";
import { useT } from "../lib/i18n";
import type { RecentEntry } from "../lib/types";

/** A vault search hit, as the Rust command returns it (generated, #316). */
type VaultHit = VaultSearchHit;

export function CommandPalette({
  open,
  onClose,
  context,
  onNavigate,
}: {
  open: boolean;
  onClose: () => void;
  context?: CommandContext;
  onNavigate?: (screen: string) => void;
}) {
  if (!open) return null;
  return <CommandPaletteBody onClose={onClose} context={context} onNavigate={onNavigate} />;
}

function CommandPaletteBody({
  onClose,
  context,
  onNavigate,
}: {
  onClose: () => void;
  context?: CommandContext;
  onNavigate?: (screen: string) => void;
}) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // The body mounts fresh on every open: reset is its initial state.
  useEffect(() => {
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, []);

  // D13: a query also shows the desk-task commands that are disabled right now,
  // with the reason, so a missing file selection is explained instead of silent.
  const results = useMemo(
    () => searchCommands(query, context, { includeDisabled: query.trim().length > 0 }),
    [query, context],
  );

  const run = (command: CommandDefinition) => {
    if (!isCommandEnabled(command)) return;
    onClose();
    void runCommand(command.id, context);
    if (command.id.startsWith("view.") && onNavigate) {
      onNavigate(command.id.slice("view.".length));
    }
  };

  return (
    <div
      className="palette-overlay"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="palette" role="dialog" aria-modal="true">
        <div className="palette-input">
          <CommandIcon size={15} />
          <input
            ref={inputRef}
            value={query}
            placeholder={t("palette.placeholder")}
            onChange={(event) => {
              setQuery(event.target.value);
              setIndex(0);
            }}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setIndex((current) => Math.min(results.length - 1, current + 1));
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setIndex((current) => Math.max(0, current - 1));
              } else if (event.key === "Enter" && results[index]) {
                event.preventDefault();
                run(results[index]);
              } else if (event.key === "Escape") {
                onClose();
              }
            }}
          />
          <span className="muted small">{t("palette.hint")}</span>
        </div>
        <div className="palette-list">
          {results.length === 0 ? <p className="muted small">{t("palette.empty")}</p> : null}
          {results.slice(0, 60).map((command, position) => (
            <button
              key={command.id}
              type="button"
              className={`palette-row${isCommandEnabled(command) ? "" : " is-disabled"}`}
              data-active={position === index}
              aria-disabled={isCommandEnabled(command) ? undefined : true}
              onMouseEnter={() => setIndex(position)}
              onClick={() => run(command)}
            >
              <span className="palette-row-text">
                <span className="palette-title">{t(command.titleKey)}</span>
                {!isCommandEnabled(command) && command.disabledReasonKey ? (
                  <span className="muted small">{t(command.disabledReasonKey)}</span>
                ) : null}
              </span>
              <span className="spacer" />
              {command.keybinding ? <kbd>{command.keybinding}</kbd> : null}
              <span className="muted small">{t(`palette.category.${command.category}`)}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

export function GlobalSearch({
  open,
  onClose,
  onOpenPath,
  onOpenRecent,
  onNavigate,
}: {
  open: boolean;
  onClose: () => void;
  onOpenPath: (path: string) => void;
  onOpenRecent: (entry: RecentEntry) => void;
  onNavigate?: (screen: string) => void;
}) {
  if (!open) return null;
  return (
    <GlobalSearchBody onClose={onClose} onOpenPath={onOpenPath} onOpenRecent={onOpenRecent} onNavigate={onNavigate} />
  );
}

function GlobalSearchBody({
  onClose,
  onOpenPath,
  onOpenRecent,
  onNavigate,
}: {
  onClose: () => void;
  onOpenPath: (path: string) => void;
  onOpenRecent: (entry: RecentEntry) => void;
  onNavigate?: (screen: string) => void;
}) {
  const t = useT();
  const recent = useRecent((state) => state.entries);
  const [query, setQuery] = useState("");
  const [vault, setVault] = useState<VaultHit[]>([]);
  const [vaultState, setVaultState] = useState<"idle" | "searching" | "missing" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // The body mounts fresh on every open; only autofocus needs an effect.
  useEffect(() => {
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, []);

  useEffect(() => {
    if (query.trim().length < 2) {
      // Nothing to search: the derived view already shows no hits.
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setVaultState("searching");
      // The palette is in the entry chunk; typed-call reaches the generated
      // wrappers (#316) without carrying them in that chunk.
      const request: VaultSearchRequest_Deserialize = { query: query.trim(), limit: 20, offset: 0 };
      void call((c) => c.vaultSearch(request))
        .then((response) => {
          if (cancelled) return;
          setVault(response.hits ?? []);
          setVaultState("idle");
        })
        .catch((reason) => {
          if (cancelled) return;
          const message = errorMessage(reason, t);
          if (message.toLowerCase().includes("index") || message.toLowerCase().includes("bulunamadı")) {
            setVaultState("missing");
          } else {
            setVaultState("error");
            setError(message);
          }
        });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, t]);

  const commands = useMemo(() => searchCommands(query), [query]);
  const effectiveVault = query.trim().length < 2 ? [] : vault;
  const effectiveVaultState = query.trim().length < 2 ? "idle" : vaultState;
  const fileHits = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return recent.slice(0, 8);
    return recent.filter((entry) => entry.fileName.toLowerCase().includes(needle)).slice(0, 10);
  }, [query, recent]);

  return (
    <div
      className="palette-overlay"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="palette" role="dialog" aria-modal="true">
        <div className="palette-input">
          <Search size={15} />
          <input
            ref={inputRef}
            value={query}
            placeholder={t("search.placeholder")}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
            }}
          />
          <span className="muted small">{t("search.hint")}</span>
        </div>
        <div className="palette-list">
          {commands.length > 0 ? <p className="palette-section">{t("search.commands")}</p> : null}
          {commands.slice(0, 8).map((command) => (
            <button
              key={command.id}
              type="button"
              className="palette-row"
              onClick={() => {
                onClose();
                void runCommand(command.id);
                if (command.id.startsWith("view.") && onNavigate) onNavigate(command.id.slice("view.".length));
              }}
            >
              <CommandIcon size={13} />
              <span className="palette-title">{t(command.titleKey)}</span>
            </button>
          ))}

          {fileHits.length > 0 ? <p className="palette-section">{t("search.files")}</p> : null}
          {fileHits.map((entry) => (
            <button
              key={entry.token}
              type="button"
              className="palette-row"
              onClick={() => {
                onClose();
                onOpenRecent(entry);
              }}
            >
              <FileText size={13} />
              <span className="palette-title">{entry.fileName}</span>
            </button>
          ))}

          {effectiveVaultState === "searching" ? <p className="muted small">{t("search.searching")}</p> : null}
          {effectiveVaultState === "missing" ? <p className="muted small">{t("search.vaultMissing")}</p> : null}
          {error ? <p className="muted small">{error}</p> : null}
          {effectiveVault.length > 0 ? <p className="palette-section">{t("search.indexed")}</p> : null}
          {effectiveVault.map((hit) => (
            <button
              key={`${hit.documentId}-${hit.matchLabel}`}
              type="button"
              className="palette-row"
              onClick={() => {
                onClose();
                onOpenPath(hit.path);
              }}
            >
              <FileText size={13} />
              <span className="palette-title">{hit.fileName}</span>
              <span className="muted small">{hit.matchLabel}</span>
              <span className="spacer" />
              <span className="muted small truncate">{hit.snippet.replaceAll("<<", "").replaceAll(">>", "")}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
