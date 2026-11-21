/** Planner: tasks with due dates, priorities and completion state. */
import { useEffect, useMemo, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useT } from "../../lib/i18n";
import { usePlanner } from "../../lib/office-store";
import { useSettings } from "../../lib/store";
import { Screen } from "../../components/layout";
import { localIsoDate } from "../../lib/format";

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

export function PlannerScreen() {
  const t = useT();
  const { tasks, loaded, load, add, update, remove } = usePlanner();
  const [month, setMonth] = useState(() => new Date());
  const [draft, setDraft] = useState("");
  const language = useSettings((state) => state.settings.language);
  // Monday-first short weekday names in the interface language (2026-01-05
  // is a Monday); they used to be hard-coded English.
  const weekdays = useMemo(() => {
    const format = new Intl.DateTimeFormat(language === "tr" ? "tr-TR" : "en-GB", { weekday: "short" });
    return Array.from({ length: 7 }, (_, index) => format.format(new Date(2026, 0, 5 + index)));
  }, [language]);
  const [selectedDate, setSelectedDate] = useState(() => localIsoDate());

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const days = useMemo(() => {
    const first = new Date(month.getFullYear(), month.getMonth(), 1);
    const start = new Date(first);
    start.setDate(first.getDate() - ((first.getDay() + 6) % 7));
    const list: Date[] = [];
    for (let index = 0; index < 42; index += 1) {
      const date = new Date(start);
      date.setDate(start.getDate() + index);
      list.push(date);
    }
    return list;
  }, [month]);

  const today = localIsoDate();
  const tasksFor = (date: string) => tasks.filter((task) => task.date === date);
  const weekAhead = useMemo(() => {
    // Task dates are local `YYYY-MM-DD` strings, which sort chronologically;
    // `new Date("YYYY-MM-DD")` would be UTC midnight and drop today's tasks
    // west of Greenwich.
    const today = localIsoDate();
    return tasks
      .filter((task) => task.date >= today)
      .sort((a, b) => a.date.localeCompare(b.date))
      .slice(0, 12);
  }, [tasks]);

  return (
    <Screen
      title={t("planner.title")}
      subtitle={t("planner.subtitle")}
      actions={
        <div className="row">
          <button
            type="button"
            className="btn btn-soft"
            onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}
          >
            ‹
          </button>
          <strong>
            {month.toLocaleDateString(language === "tr" ? "tr-TR" : "en-GB", { month: "long", year: "numeric" })}
          </strong>
          <button
            type="button"
            className="btn btn-soft"
            onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}
          >
            ›
          </button>
        </div>
      }
    >
      <div className="planner-layout">
        <div className="card planner-calendar">
          <div className="planner-weekdays">
            {weekdays.map((day) => (
              <span key={day}>{day}</span>
            ))}
          </div>
          <div className="planner-grid">
            {days.map((date) => {
              const iso = localIsoDate(date);
              const isCurrentMonth = date.getMonth() === month.getMonth();
              const dayTasks = tasksFor(iso);
              return (
                <button
                  key={iso}
                  type="button"
                  className={`planner-day${isCurrentMonth ? "" : " is-outside"}${iso === today ? " is-today" : ""}${selectedDate === iso ? " is-selected" : ""}`}
                  onClick={() => setSelectedDate(iso)}
                >
                  <span>{date.getDate()}</span>
                  {dayTasks.slice(0, 3).map((task) => (
                    <i key={task.id} className={task.done ? "is-done" : ""} />
                  ))}
                </button>
              );
            })}
          </div>
        </div>
        <div className="card planner-side">
          <h3>{selectedDate}</h3>
          <div className="row">
            <input
              className="text-input"
              placeholder={t("planner.addTask")}
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && draft.trim()) {
                  add(selectedDate, draft.trim());
                  setDraft("");
                }
              }}
            />
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                if (draft.trim()) {
                  add(selectedDate, draft.trim());
                  setDraft("");
                }
              }}
            >
              <Plus size={15} />
            </button>
          </div>
          <div className="stack">
            {tasksFor(selectedDate).map((task) => (
              <div key={task.id} className="row task-row">
                <input
                  type="checkbox"
                  checked={task.done}
                  onChange={(event) => update(task.id, { done: event.target.checked })}
                />
                <span className={task.done ? "is-done" : ""}>{task.title}</span>
                <span className="spacer" />
                <select
                  value={task.priority}
                  onChange={(event) => update(task.id, { priority: event.target.value as "low" | "normal" | "high" })}
                >
                  <option value="low">{t("planner.low")}</option>
                  <option value="normal">{t("planner.normal")}</option>
                  <option value="high">{t("planner.high")}</option>
                </select>
                <button type="button" className="icon-btn" onClick={() => remove(task.id)}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
          <h3>{t("planner.upcoming")}</h3>
          <div className="stack">
            {weekAhead.map((task) => (
              <div key={task.id} className="row">
                <span className="muted">{task.date}</span>
                <span className={task.done ? "is-done" : ""}>{task.title}</span>
              </div>
            ))}
            {weekAhead.length === 0 ? <p className="muted">{t("planner.noTasks")}</p> : null}
          </div>
        </div>
      </div>
    </Screen>
  );
}
