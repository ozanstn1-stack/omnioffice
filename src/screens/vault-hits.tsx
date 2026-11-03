/**
 * One search hit and its highlighted snippet.
 *
 * Extracted from the screen so the screen file stays inside its size budget;
 * the markup and the `data-*` hooks the E2E driver reads are unchanged.
 */
import type { VaultSearchHit } from "./Vault";
import { Badge } from "../components/ui";
import { formatBytes, formatDate } from "../lib/format";
import { useT } from "../lib/i18n";

function Snippet({ text }: { text: string }) {
  if (!text) return null;
  const parts = text.split(/(<<[^<>]*>>)/g).filter(Boolean);
  return (
    <span className="block text-[13px] leading-snug" style={{ color: "var(--text-1)" }}>
      {parts.map((part, index) =>
        part.startsWith("<<") && part.endsWith(">>") ? (
          <mark
            key={index}
            style={{ background: "var(--accent-weak)", color: "var(--accent)", borderRadius: 3, padding: "0 2px" }}
          >
            {part.slice(2, -2)}
          </mark>
        ) : (
          <span key={index}>{part}</span>
        ),
      )}
    </span>
  );
}

export function HitRow({
  hit,
  active,
  onSelect,
}: {
  hit: VaultSearchHit;
  active: boolean;
  onSelect: (hit: VaultSearchHit) => void;
}) {
  const t = useT();
  return (
    <button
      type="button"
      data-testid="vault-hit"
      data-file-name={hit.fileName}
      onClick={() => onSelect(hit)}
      className="card-soft text-left flex flex-col gap-1 px-3 py-2.5"
      style={active ? { outline: "2px solid var(--accent)" } : undefined}
    >
      <span className="flex w-full items-center gap-2 flex-wrap">
        <span className="font-medium text-[13px] truncate" style={{ color: "var(--text-1)" }}>
          {hit.fileName}
        </span>
        <Badge>{hit.extension.toUpperCase()}</Badge>
        <Badge tone="accent">{hit.matchLabel}</Badge>
        <span className="text-xs muted ml-auto">{t("vault.score", { score: Math.round(hit.score ?? 0) })}</span>
      </span>
      <span className="block w-full text-xs muted truncate" title={hit.path}>
        {hit.path}
      </span>
      <span className="block text-xs muted">
        {formatBytes(hit.size)} · {formatDate(hit.modified)}
      </span>
      <Snippet text={hit.snippet} />
    </button>
  );
}
