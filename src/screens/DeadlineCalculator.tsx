import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, CalendarPlus, Download, FileSearch } from "lucide-react";
import { Button, Card } from "../components/ui";
import { DropZone } from "../components/files";
import { Screen } from "../components/layout";
import { pageText } from "../lib/api";
import { useT } from "../lib/i18n";
import {
  PERIOD_PRESETS,
  computeDeadline,
  deadlineIcs,
  findNotificationDates,
  type NotificationDateCandidate,
  type PeriodUnit,
} from "../lib/legal-deadline";
import { saveFileBytes } from "../lib/mobile";
import { usePlanner } from "../lib/office-store";
import { reportError, useToasts } from "../lib/store";
import { uid } from "../lib/format";

/** Pages of a notification PDF read for its dates. */
const NOTICE_PAGES = 3;

function today(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

/**
 * Notification deadline calculator (C2): the arrival date (typed or read
 * from an e-Tebligat PDF), the period and the judicial recess give the last
 * day, with every step shown; the result goes to the Planner or a `.ics`.
 */
/** Dates in the first pages of a notification PDF, the labelled ones first. */
async function noticeDates(path: string): Promise<NotificationDateCandidate[]> {
  const texts: string[] = [];
  for (let page = 1; page <= NOTICE_PAGES; page++) {
    const text = await pageText(path, page).catch(() => null);
    if (text === null) break;
    texts.push(text);
  }
  return findNotificationDates(texts.join("\n"));
}

export function DeadlineCalculatorScreen({
  initialFiles = [],
  dragging = false,
}: {
  initialFiles?: string[];
  dragging?: boolean;
}) {
  const t = useT();
  const push = useToasts((state) => state.push);
  const addTask = usePlanner((state) => state.add);
  const loadPlanner = usePlanner((state) => state.load);
  const plannerLoaded = usePlanner((state) => state.loaded);
  const [received, setReceived] = useState(today);
  const [electronic, setElectronic] = useState(true);
  const [preset, setPreset] = useState(PERIOD_PRESETS[0].key);
  const [amount, setAmount] = useState(2);
  const [unit, setUnit] = useState<PeriodUnit>("week");
  const [recess, setRecess] = useState(true);
  const [title, setTitle] = useState("");
  const [candidates, setCandidates] = useState<NotificationDateCandidate[] | null>(null);
  const [reading, setReading] = useState(false);

  const result = useMemo(
    () => computeDeadline({ received, electronic, period: { amount, unit }, judicialRecess: recess }),
    [received, electronic, amount, unit, recess],
  );

  const choosePreset = (key: string) => {
    setPreset(key);
    const found = PERIOD_PRESETS.find((entry) => entry.key === key);
    if (found) {
      setAmount(found.period.amount);
      setUnit(found.period.unit);
    }
  };

  const applyDates = (found: NotificationDateCandidate[]) => {
    setCandidates(found);
    if (found[0]) {
      setReceived(found[0].date);
      setElectronic(found[0].kind !== "served");
    }
  };

  const readNotice = async (path: string) => {
    setReading(true);
    try {
      applyDates(await noticeDates(path));
    } catch (error) {
      reportError(error, t);
    } finally {
      setReading(false);
    }
  };

  // A PDF dropped on the window or opened with this screen is read once.
  const initialPath = initialFiles[0] ?? "";
  useEffect(() => {
    if (!initialPath) return;
    let live = true;
    void noticeDates(initialPath).then((found) => {
      if (live) applyDates(found);
    });
    return () => {
      live = false;
    };
  }, [initialPath]);

  const label = title.trim() || t("deadline.defaultTitle");
  const summary = result
    ? t("deadline.summary", { served: result.served, deadline: result.deadline, period: periodText(amount, unit, t) })
    : "";

  const addToPlanner = async () => {
    if (!result) return;
    if (!plannerLoaded) await loadPlanner();
    addTask(result.deadline, `${label} — ${t("deadline.lastDay")}`);
    push({ kind: "success", title: t("deadline.addedToPlanner"), detail: result.deadline });
  };

  const downloadIcs = async () => {
    if (!result) return;
    const ics = deadlineIcs({
      title: `${label} — ${t("deadline.lastDay")}`,
      deadline: result.deadline,
      description: summary,
      uid: `${uid("deadline")}@omnioffice`,
      now: new Date(),
    });
    try {
      await saveFileBytes(new TextEncoder().encode(ics), `${result.deadline}.ics`, {
        name: t("deadline.calendarFile"),
        extensions: ["ics"],
      });
    } catch (error) {
      reportError(error, t);
    }
  };

  return (
    <Screen title={t("deadline.title")} subtitle={t("deadline.subtitle")}>
      <DropZone
        compact
        accept="pdf"
        dragging={dragging}
        title={t("deadline.readPdf")}
        hint={t("deadline.readPdfHint")}
        onPaths={(paths) => paths[0] && void readNotice(paths[0])}
      />
      {reading ? <p className="text-xs muted">{t("deadline.reading")}</p> : null}
      {candidates ? (
        <Card className="p-3">
          {candidates.length === 0 ? (
            <p className="text-xs muted">{t("deadline.noDates")}</p>
          ) : (
            <div className="flex flex-wrap items-center gap-2">
              <FileSearch size={14} aria-hidden />
              <span className="text-xs muted">{t("deadline.datesFound")}</span>
              {candidates.map((candidate) => (
                <Button
                  key={candidate.date}
                  size="sm"
                  variant={candidate.date === received ? "primary" : "ghost"}
                  onClick={() => {
                    setReceived(candidate.date);
                    setElectronic(candidate.kind !== "served");
                  }}
                >
                  {candidate.date} · {t(`deadline.kind.${candidate.kind}`)}
                </Button>
              ))}
            </div>
          )}
        </Card>
      ) : null}

      <Card className="p-4 flex flex-col gap-3">
        <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))" }}>
          <div>
            <label className="label" htmlFor="deadline-received">
              {electronic ? t("deadline.received") : t("deadline.servedOn")}
            </label>
            <input
              id="deadline-received"
              className="input"
              type="date"
              value={received}
              onChange={(event) => setReceived(event.target.value)}
            />
          </div>
          <div>
            <label className="label" htmlFor="deadline-preset">
              {t("deadline.period")}
            </label>
            <select
              id="deadline-preset"
              className="select"
              value={preset}
              onChange={(event) => choosePreset(event.target.value)}
            >
              {PERIOD_PRESETS.map((entry) => (
                <option key={entry.key} value={entry.key}>
                  {t(`deadline.preset.${entry.key}`)}
                </option>
              ))}
              <option value="custom">{t("deadline.preset.custom")}</option>
            </select>
          </div>
          {preset === "custom" ? (
            <div className="flex gap-2 items-end">
              <div className="flex-1">
                <label className="label" htmlFor="deadline-amount">
                  {t("deadline.amount")}
                </label>
                <input
                  id="deadline-amount"
                  className="input"
                  type="number"
                  min={1}
                  max={3650}
                  value={amount}
                  onChange={(event) => setAmount(Number(event.target.value))}
                />
              </div>
              <select
                className="select"
                aria-label={t("deadline.unit")}
                value={unit}
                onChange={(event) => setUnit(event.target.value as PeriodUnit)}
              >
                <option value="day">{t("deadline.unit.day")}</option>
                <option value="week">{t("deadline.unit.week")}</option>
                <option value="month">{t("deadline.unit.month")}</option>
              </select>
            </div>
          ) : null}
          <div>
            <label className="label" htmlFor="deadline-title">
              {t("deadline.caseTitle")}
            </label>
            <input
              id="deadline-title"
              className="input"
              value={title}
              maxLength={120}
              placeholder={t("deadline.caseTitlePlaceholder")}
              onChange={(event) => setTitle(event.target.value)}
            />
          </div>
        </div>
        <label className="checkbox">
          <input type="checkbox" checked={electronic} onChange={(event) => setElectronic(event.target.checked)} />
          <span>{t("deadline.electronic")}</span>
        </label>
        <label className="checkbox">
          <input type="checkbox" checked={recess} onChange={(event) => setRecess(event.target.checked)} />
          <span>{t("deadline.recess")}</span>
        </label>
      </Card>

      {result ? (
        <Card className="p-4 flex flex-col gap-3" aria-live="polite">
          <p className="text-xs muted">{t("deadline.lastDay")}</p>
          <p className="text-2xl font-bold" data-testid="deadline-result">
            {result.deadline}
          </p>
          <ol className="flex flex-col gap-1 text-[13px]">
            {result.steps.map((step) => (
              <li key={`${step.kind}-${step.date}`}>
                {t(`deadline.step.${step.kind}`, { date: step.date, skipped: (step.skipped ?? []).join(", ") })}
              </li>
            ))}
          </ol>
          {result.halfDay ? <Warning text={t("deadline.halfDay")} /> : null}
          {result.outsideHolidayTable ? <Warning text={t("deadline.outsideTable")} /> : null}
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" icon={<CalendarPlus size={14} />} onClick={() => void addToPlanner()}>
              {t("deadline.addToPlanner")}
            </Button>
            <Button icon={<Download size={14} />} onClick={() => void downloadIcs()}>
              {t("deadline.saveIcs")}
            </Button>
          </div>
        </Card>
      ) : (
        <Card className="p-4">
          <p className="text-[13px]">{t("deadline.invalid")}</p>
        </Card>
      )}
      <p className="text-xs muted">{t("deadline.disclaimer")}</p>
    </Screen>
  );
}

function Warning({ text }: { text: string }) {
  return (
    <p className="flex items-start gap-2 text-[13px]" role="note">
      <AlertTriangle size={14} style={{ color: "var(--warn)", marginTop: 2 }} aria-hidden />
      {text}
    </p>
  );
}

function periodText(
  amount: number,
  unit: PeriodUnit,
  t: (key: string, params?: Record<string, string | number>) => string,
) {
  return t(`deadline.periodText.${unit}`, { count: amount });
}
