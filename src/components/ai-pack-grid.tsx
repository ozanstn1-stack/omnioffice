/**
 * The task-pack chooser grid (C7, D5). Kept as its own component: the pack
 * screen owns running, verifying and the provider notes, the grid only picks.
 */
import { Bot } from "lucide-react";
import { useT } from "../lib/i18n";
import type { TaskPack, TaskPackId } from "../lib/ai-task-packs";

export function AiPackGrid({
  packs,
  activeId,
  onPick,
}: {
  packs: TaskPack[];
  activeId: TaskPackId;
  onPick: (id: TaskPackId) => void;
}) {
  const t = useT();
  return (
    <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))" }}>
      {packs.map((candidate) => (
        <button
          key={candidate.id}
          type="button"
          className={`card p-4 text-left${candidate.id === activeId ? " is-active" : ""}`}
          style={candidate.id === activeId ? { outline: "2px solid var(--accent)" } : undefined}
          onClick={() => onPick(candidate.id)}
        >
          <div className="row" style={{ justifyContent: "space-between" }}>
            <strong>{t(candidate.titleKey)}</strong>
            <Bot size={15} />
          </div>
          <p className="muted small mt-1">{t(candidate.descriptionKey)}</p>
        </button>
      ))}
    </div>
  );
}
