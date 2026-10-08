import { useId, useState } from "react";
import { Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { CondRule, CondThreshold } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { ICON_SETS } from "../conditional";
import {
  HIGHLIGHT_TYPES,
  RULE_TYPES,
  defaultRuleForm,
  ruleFromForm,
  type RuleForm,
  type RuleType,
  type StopForm,
} from "../conditional-form";
import { CfIcon } from "./CfDecor";

/** Where a threshold can sit; `min` and `max` are the ends of the data. */
const STOP_KINDS: Array<CondThreshold["kind"]> = ["min", "num", "percent", "percentile", "formula", "max"];

const STOP_LABELS: Record<CondThreshold["kind"], string> = {
  min: "calc.cfLowest",
  max: "calc.cfHighest",
  num: "calc.cfNumber",
  percent: "calc.cfPercent",
  percentile: "calc.cfPercentile",
  formula: "calc.cfFormula",
};

/** One threshold: where it sits, the number or formula that places it, and optionally a colour. */
function StopFields({
  label,
  stop,
  kinds,
  withColor,
  onChange,
}: {
  label: string;
  stop: StopForm;
  kinds: Array<CondThreshold["kind"]>;
  withColor: boolean;
  onChange: (stop: StopForm) => void;
}) {
  const t = useT();
  const open = stop.kind === "min" || stop.kind === "max";
  return (
    <fieldset className="cf-stop">
      <legend>{label}</legend>
      <div className="row">
        <select
          aria-label={`${label}: ${t("calc.cfType")}`}
          value={stop.kind}
          onChange={(event) => onChange({ ...stop, kind: event.target.value as CondThreshold["kind"] })}
        >
          {kinds.map((kind) => (
            <option key={kind} value={kind}>
              {t(STOP_LABELS[kind])}
            </option>
          ))}
        </select>
        {open ? null : (
          <input
            aria-label={`${label}: ${t("calc.value")}`}
            value={stop.value}
            onChange={(event) => onChange({ ...stop, value: event.target.value })}
          />
        )}
        {withColor ? (
          <input
            type="color"
            aria-label={`${label}: ${t("calc.cfColor")}`}
            value={stop.color}
            onChange={(event) => onChange({ ...stop, color: event.target.value })}
          />
        ) : null}
      </div>
    </fieldset>
  );
}

/** The kinds a bound of an icon set can use: the ends of the data make no sense for a lower bound. */
const BOUND_KINDS: Array<CondThreshold["kind"]> = ["num", "percent", "percentile", "formula"];

/** Lists the rules of the sheet and adds one: a highlight, colour scale, data bar, icon set or formula rule. */
export function ConditionalDialog({
  rules,
  defaultRange,
  onClose,
  onApply,
  onDelete,
}: {
  rules: readonly CondRule[];
  /** The range a new rule starts with: the selection, or the used range. */
  defaultRange: string;
  onClose: () => void;
  onApply: (rule: Omit<CondRule, "id">) => void;
  onDelete: (id: string) => void;
}) {
  const t = useT();
  const formulaHint = useId();
  const [form, setForm] = useState<RuleForm>(() => defaultRuleForm(defaultRange));
  const [error, setError] = useState<string | null>(null);
  const patch = (next: Partial<RuleForm>) => {
    setForm((current) => ({ ...current, ...next }));
    setError(null);
  };
  const setStop = (index: 0 | 1 | 2, stop: StopForm) =>
    patch({ stops: form.stops.map((entry, at) => (at === index ? stop : entry)) as RuleForm["stops"] });
  const setIcon = (index: 0 | 1, stop: StopForm) =>
    patch({ icons: form.icons.map((entry, at) => (at === index ? stop : entry)) as RuleForm["icons"] });
  const { type } = form;
  const highlight = HIGHLIGHT_TYPES.includes(type);

  const submit = () => {
    const result = ruleFromForm(form);
    if ("error" in result) return setError(t(result.error));
    onApply(result.rule);
  };

  return (
    <Dialog title={t("calc.conditionalFormatting")} onClose={onClose}>
      <div className="stack">
        {rules.length > 0 ? (
          <ul className="cf-rules" aria-label={t("calc.cfRules")}>
            {rules.map((rule) => (
              <li key={rule.id}>
                <span>
                  {t(ruleLabel(rule.kind))} <span className="muted">{rule.range}</span>
                </span>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => onDelete(rule.id)}
                  aria-label={`${t("calc.cfDelete")}: ${t(ruleLabel(rule.kind))} ${rule.range}`}
                  title={t("calc.cfDelete")}
                >
                  <Trash2 size={13} />
                </button>
              </li>
            ))}
          </ul>
        ) : null}
        <label className="field">
          <span>{t("calc.cfRange")}</span>
          <input value={form.range} onChange={(event) => patch({ range: event.target.value })} />
        </label>
        <label className="field">
          <span>{t("calc.rule")}</span>
          <select value={type} onChange={(event) => patch({ type: event.target.value as RuleType })}>
            {RULE_TYPES.map((candidate) => (
              <option key={candidate} value={candidate}>
                {t(ruleLabel(candidate))}
              </option>
            ))}
          </select>
        </label>

        {type === "greater" || type === "less" || type === "equal" || type === "textContains" || type === "between" ? (
          <div className="row">
            <label className="field">
              <span>{t("calc.value")}</span>
              <input value={form.first} onChange={(event) => patch({ first: event.target.value })} />
            </label>
            {type === "between" ? (
              <label className="field">
                <span>{t("calc.and")}</span>
                <input value={form.second} onChange={(event) => patch({ second: event.target.value })} />
              </label>
            ) : null}
          </div>
        ) : null}
        {type === "top" || type === "bottom" ? (
          <label className="field">
            <span>{t("calc.value")}</span>
            <input value={form.first} onChange={(event) => patch({ first: event.target.value })} />
          </label>
        ) : null}
        {type === "expression" ? (
          <>
            <label className="field">
              <span>{t("calc.cfFormula")}</span>
              <input
                value={form.formula}
                placeholder="=$B2>100"
                aria-describedby={formulaHint}
                onChange={(event) => patch({ formula: event.target.value })}
              />
            </label>
            <small id={formulaHint} className="muted">
              {t("calc.cfFormulaHint")}
            </small>
          </>
        ) : null}

        {highlight ? (
          <>
            <label className="field">
              <span>{t("calc.fillColor")}</span>
              <input type="color" value={form.fill} onChange={(event) => patch({ fill: event.target.value })} />
            </label>
            <div className="row">
              <label className="cf-check">
                <input
                  type="checkbox"
                  checked={form.useFontColor}
                  onChange={(event) => patch({ useFontColor: event.target.checked })}
                />
                <span>{t("calc.cfFontColor")}</span>
              </label>
              {form.useFontColor ? (
                <input
                  type="color"
                  aria-label={t("calc.cfFontColor")}
                  value={form.fontColor}
                  onChange={(event) => patch({ fontColor: event.target.value })}
                />
              ) : null}
              <label className="cf-check">
                <input
                  type="checkbox"
                  checked={form.bold}
                  onChange={(event) => patch({ bold: event.target.checked })}
                />
                <span>{t("calc.cfBold")}</span>
              </label>
              <label className="cf-check">
                <input
                  type="checkbox"
                  checked={form.italic}
                  onChange={(event) => patch({ italic: event.target.checked })}
                />
                <span>{t("calc.cfItalic")}</span>
              </label>
            </div>
          </>
        ) : null}

        {type === "colorScale" ? (
          <>
            <label className="field">
              <span>{t("calc.cfColors")}</span>
              <select value={form.colors} onChange={(event) => patch({ colors: Number(event.target.value) as 2 | 3 })}>
                <option value={2}>{t("calc.cfColors2")}</option>
                <option value={3}>{t("calc.cfColors3")}</option>
              </select>
            </label>
            <StopFields
              label={t("calc.cfMin")}
              stop={form.stops[0]}
              kinds={STOP_KINDS}
              withColor
              onChange={(stop) => setStop(0, stop)}
            />
            {form.colors === 3 ? (
              <StopFields
                label={t("calc.cfMid")}
                stop={form.stops[1]}
                kinds={STOP_KINDS}
                withColor
                onChange={(stop) => setStop(1, stop)}
              />
            ) : null}
            <StopFields
              label={t("calc.cfMax")}
              stop={form.stops[2]}
              kinds={STOP_KINDS}
              withColor
              onChange={(stop) => setStop(2, stop)}
            />
          </>
        ) : null}

        {type === "dataBar" ? (
          <>
            <label className="field">
              <span>{t("calc.cfBarColor")}</span>
              <input type="color" value={form.barColor} onChange={(event) => patch({ barColor: event.target.value })} />
            </label>
            <label className="cf-check">
              <input
                type="checkbox"
                checked={form.showValue}
                onChange={(event) => patch({ showValue: event.target.checked })}
              />
              <span>{t("calc.cfShowValue")}</span>
            </label>
          </>
        ) : null}

        {type === "iconSet" ? (
          <>
            <label className="field">
              <span>{t("calc.cfIconSet")}</span>
              <select value={form.iconSet} onChange={(event) => patch({ iconSet: event.target.value })}>
                {ICON_SETS.map((set) => (
                  <option key={set} value={set}>
                    {t(`calc.icons_${set}`)}
                  </option>
                ))}
              </select>
            </label>
            <div className="cf-preview" aria-hidden>
              {[0, 1, 2].map((tier) => (
                <CfIcon key={tier} set={form.iconSet} tier={form.reverseIcons ? 2 - tier : tier} count={3} size={16} />
              ))}
            </div>
            <StopFields
              label={t("calc.cfIconMid")}
              stop={form.icons[0]}
              kinds={BOUND_KINDS}
              withColor={false}
              onChange={(stop) => setIcon(0, stop)}
            />
            <StopFields
              label={t("calc.cfIconTop")}
              stop={form.icons[1]}
              kinds={BOUND_KINDS}
              withColor={false}
              onChange={(stop) => setIcon(1, stop)}
            />
            <div className="row">
              <label className="cf-check">
                <input
                  type="checkbox"
                  checked={form.reverseIcons}
                  onChange={(event) => patch({ reverseIcons: event.target.checked })}
                />
                <span>{t("calc.cfReverse")}</span>
              </label>
              <label className="cf-check">
                <input
                  type="checkbox"
                  checked={form.showValue}
                  onChange={(event) => patch({ showValue: event.target.checked })}
                />
                <span>{t("calc.cfShowValue")}</span>
              </label>
            </div>
          </>
        ) : null}

        {error ? (
          <p className="find-error" role="alert">
            {error}
          </p>
        ) : null}
        <button type="button" className="btn btn-primary" onClick={submit}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}

/** The translation key naming a rule kind. */
function ruleLabel(kind: string): string {
  switch (kind) {
    case "greater":
      return "calc.ruleGreater";
    case "less":
      return "calc.ruleLess";
    case "between":
      return "calc.ruleBetween";
    case "equal":
      return "calc.ruleEqual";
    case "textContains":
      return "calc.ruleText";
    case "duplicate":
      return "calc.ruleDuplicate";
    case "top":
      return "calc.ruleTop";
    case "bottom":
      return "calc.ruleBottom";
    case "expression":
      return "calc.ruleFormula";
    case "colorScale":
      return "calc.ruleColorScale";
    case "dataBar":
      return "calc.ruleDataBar";
    case "iconSet":
      return "calc.ruleIconSet";
    default:
      return "calc.rule";
  }
}
