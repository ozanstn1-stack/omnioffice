import { Dialog } from "../../office-ui";
import { useT } from "../../../lib/i18n";
import type { CompiledSearch } from "../find-replace";

/**
 * Writer's Find & replace dialog (split out of WriterEditor in 3.3a). The
 * query, options and every action stay with the editor, which matches on
 * the model (writer/find-replace.ts); this only draws them.
 */
export function FindReplaceDialog({
  findText,
  replaceText,
  matchCase,
  wholeWord,
  useRegex,
  onFindText,
  onReplaceText,
  onMatchCase,
  onWholeWord,
  onUseRegex,
  search,
  status,
  onStep,
  onReplace,
  onReplaceAll,
  onClose,
}: {
  findText: string;
  replaceText: string;
  matchCase: boolean;
  wholeWord: boolean;
  useRegex: boolean;
  onFindText: (value: string) => void;
  onReplaceText: (value: string) => void;
  onMatchCase: (value: boolean) => void;
  onWholeWord: (value: boolean) => void;
  onUseRegex: (value: boolean) => void;
  search: CompiledSearch | null;
  status: string;
  onStep: (forward: boolean) => void;
  onReplace: () => void;
  onReplaceAll: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const ready = search?.ok === true;
  return (
    <Dialog title={t("writer.findReplace")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("writer.findWhat")}</span>
          <input
            value={findText}
            onChange={(event) => onFindText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return;
              event.preventDefault();
              onStep(!event.shiftKey);
            }}
            aria-invalid={search?.ok === false}
            // eslint-disable-next-line jsx-a11y/no-autofocus -- opening Find must put the caret in the search field
            autoFocus
          />
        </label>
        {search && !search.ok ? (
          <p className="find-error" role="alert">
            {t("writer.findInvalidRegex")}: {search.error}
          </p>
        ) : (
          <p className="find-count muted" role="status">
            {status}
          </p>
        )}
        <label className="field">
          <span>{t("writer.replaceWith")}</span>
          <input value={replaceText} onChange={(event) => onReplaceText(event.target.value)} />
        </label>
        <div className="row">
          <label className="check">
            <input type="checkbox" checked={matchCase} onChange={(event) => onMatchCase(event.target.checked)} />{" "}
            {t("writer.matchCase")}
          </label>
          <label className="check">
            <input type="checkbox" checked={wholeWord} onChange={(event) => onWholeWord(event.target.checked)} />{" "}
            {t("writer.wholeWord")}
          </label>
          <label className="check">
            <input type="checkbox" checked={useRegex} onChange={(event) => onUseRegex(event.target.checked)} />{" "}
            {t("writer.regex")}
          </label>
        </div>
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={() => onStep(true)} disabled={!ready}>
            {t("writer.findNext")}
          </button>
          <button type="button" className="btn btn-soft" onClick={() => onStep(false)} disabled={!ready}>
            {t("writer.findPrevious")}
          </button>
          <button type="button" className="btn btn-soft" onClick={onReplace} disabled={!ready}>
            {t("writer.replace")}
          </button>
          <button type="button" className="btn btn-primary" onClick={onReplaceAll} disabled={!ready}>
            {t("writer.replaceAll")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
