import { useMemo, useState } from "react";
import { Search } from "lucide-react";
import { Screen } from "../components/layout";
import { EmptyState } from "../components/ui";
import { useT } from "../lib/i18n";
import { foldSearch } from "../lib/search-fold";
import { routeEnabled } from "../lib/routes";
import { CARDS, GROUPS, type ToolGroup } from "../lib/tool-cards";
import type { Navigate } from "../lib/nav";

/**
 * The Tools hub (7.32): every tool as a tile, grouped, with a search field
 * and one chip per group. On a phone it is the "Tools" tab, four tiles a
 * row; on the desktop it is the sidebar's "Tools" entry.
 */
export function ToolsScreen({ onNavigate }: { onNavigate: Navigate }) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState<ToolGroup | "all">("all");

  const cards = useMemo(() => CARDS.filter((card) => routeEnabled(card.id)), []);
  const needle = foldSearch(query.trim());
  const sections = useMemo(() => {
    const matches = cards.filter((card) => {
      if (group !== "all" && card.group !== group) return false;
      if (!needle) return true;
      return foldSearch(`${t(card.titleKey)} ${card.descKey ? t(card.descKey) : ""} ${card.id}`).includes(needle);
    });
    return GROUPS.map((entry) => ({ ...entry, items: matches.filter((card) => card.group === entry.key) })).filter(
      (entry) => entry.items.length > 0,
    );
  }, [cards, group, needle, t]);

  return (
    <Screen title={t("nav.tools")} subtitle={t("tools.subtitle", { count: cards.length })}>
      <div className="tools-head">
        <label className="tools-search">
          <Search size={18} aria-hidden />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("home.searchTools")}
            aria-label={t("home.searchTools")}
          />
        </label>
        <div className="recent-filters" role="group" aria-label={t("tools.groupLabel")}>
          <button type="button" className="chip" aria-pressed={group === "all"} onClick={() => setGroup("all")}>
            {t("recent.filterAll")}
          </button>
          {GROUPS.map((entry) => (
            <button
              key={entry.key}
              type="button"
              className="chip"
              aria-pressed={group === entry.key}
              onClick={() => setGroup(entry.key)}
            >
              {t(entry.labelKey)}
            </button>
          ))}
        </div>
      </div>

      {sections.length === 0 ? (
        <EmptyState icon={<Search size={22} />} title={t("home.noToolResults")} />
      ) : (
        sections.map((section) => (
          <section key={section.key} aria-labelledby={`tools-${section.key}`}>
            <h2 id={`tools-${section.key}`} className="section-title">
              {t(section.labelKey)}
            </h2>
            <div className="tool-tiles">
              {section.items.map((card) => (
                <button key={card.id} type="button" className="tool-tile" onClick={() => onNavigate(card.id)}>
                  <span className={`tool-tile-icon tool-tile-${card.group}`} aria-hidden>
                    {card.icon}
                  </span>
                  <span className="tool-tile-label">{t(card.titleKey)}</span>
                </button>
              ))}
            </div>
          </section>
        ))
      )}
    </Screen>
  );
}
