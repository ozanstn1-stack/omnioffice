import { useEffect, useState } from "react";
import { pickDocumentToken, pickSaveToken } from "../lib/file-tokens";
import { ArrowRight, FolderOpen, ListPlus, Play, Save, Trash2 } from "lucide-react";
import { Button, Card, Field, TextInput } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import {
  recipeDelete,
  recipeList,
  recipeRun,
  recipeSave,
  watchedFolderGet,
  watchedFolderScan,
  watchedFolderSet,
  watchedFolderStart,
  watchedFolderStop,
  type Recipe,
  type RecipeStep,
  type WatchedFolder,
} from "../lib/api";

/** The step palette; each adds with sensible defaults that can be edited later. */
const STEP_TEMPLATES: { kind: RecipeStep["kind"]; build: () => RecipeStep }[] = [
  { kind: "compress", build: () => ({ kind: "compress", options: {} }) },
  {
    kind: "stamp",
    build: () => ({
      kind: "stamp",
      headerFooter: { headerCenter: "Sayfa {page}", fontSizePt: 10, color: "#111111", marginPt: 24 },
    }),
  },
  { kind: "pdfa", build: () => ({ kind: "pdfa", level: "pdfa2b" }) },
  {
    kind: "ocr",
    build: () => ({ kind: "ocr", options: { languages: ["tur", "eng"], preprocess: { contrast: true } } }),
  },
  { kind: "redact", build: () => ({ kind: "redact", options: {} }) },
];

function stepLabel(step: RecipeStep) {
  return step.kind;
}

/**
 * Chainable batch recipes and a watched folder.
 *
 * A recipe is an ordered list of steps run over one PDF (compress, stamp,
 * PDF/A, OCR, redact). Steps start from sensible defaults; the detailed options
 * live in the dedicated tools. The watched folder runs the chosen recipe over
 * new files in a folder.
 */
export function Recipes() {
  const t = useT();
  const [recipes, setRecipes] = useState<Recipe[]>([]);
  const [name, setName] = useState("");
  const [steps, setSteps] = useState<RecipeStep[]>([]);
  const [watch, setWatch] = useState<WatchedFolder>({ folder: "", outputDir: "", recipeId: "" });
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    try {
      setRecipes((await recipeList()) ?? []);
      setWatch((await watchedFolderGet()) ?? { folder: "", outputDir: "", recipeId: "" });
    } catch {
      // A missing recipe directory is fine; the list stays empty.
    }
  };

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = (await recipeList()) ?? [];
        const config = (await watchedFolderGet()) ?? { folder: "", outputDir: "", recipeId: "" };
        if (!cancelled) {
          setRecipes(list);
          setWatch(config);
        }
      } catch {
        // A missing recipe directory is fine; the list stays empty.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const notify = (kind: "success" | "error" | "info", title: string, detail?: string) =>
    useToasts.getState().push({ kind, title, detail });

  const save = async () => {
    if (!name.trim() || !steps.length) {
      notify("info", t("recipes.needNameAndSteps"));
      return;
    }
    try {
      await recipeSave({ id: name.trim(), name: name.trim(), steps });
      setName("");
      setSteps([]);
      await refresh();
      notify("success", t("recipes.saved"));
    } catch (error) {
      reportError(error, t);
    }
  };

  const run = async (recipe: Recipe) => {
    const picked = await pickDocumentToken(["pdf"]).catch(() => null);
    if (!picked) return;
    const output = await pickSaveToken("islenmis.pdf", ["pdf"]).catch(() => null);
    if (!output) return;
    setBusy(true);
    try {
      const result = await recipeRun(picked.token, output.token, recipe);
      notify("success", t("recipes.ran", { steps: result.applied.join(" → ") }), result.output);
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (recipe: Recipe) => {
    try {
      await recipeDelete(recipe.id);
      await refresh();
    } catch (error) {
      reportError(error, t);
    }
  };

  const saveWatch = async () => {
    try {
      await watchedFolderSet(watch);
      notify("success", t("recipes.watchSaved"));
    } catch (error) {
      reportError(error, t);
    }
  };

  const scanWatch = async () => {
    setBusy(true);
    try {
      const results = await watchedFolderScan();
      notify(results.length ? "success" : "info", t("recipes.watchScan", { count: results.length }));
      for (const result of results) {
        if (!result.ok) notify("error", t("recipes.watchError"), `${result.input}: ${result.detail}`);
      }
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const startWatch = async () => {
    try {
      await watchedFolderStart();
      notify("success", t("recipes.watchStarted"));
    } catch (error) {
      reportError(error, t);
    }
  };

  const stopWatch = async () => {
    try {
      await watchedFolderStop();
      notify("info", t("recipes.watchStopped"));
    } catch (error) {
      reportError(error, t);
    }
  };

  return (
    <Screen title={t("recipes.title")} subtitle={t("recipes.subtitle")}>
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <strong>{t("recipes.builder")}</strong>
          <Field label={t("recipes.name")}>
            <TextInput
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t("recipes.namePlaceholder")}
            />
          </Field>
          <div className="flex flex-wrap gap-1">
            {STEP_TEMPLATES.map((template) => (
              <Button
                key={template.kind}
                size="sm"
                variant="ghost"
                onClick={() => setSteps((current) => [...current, template.build()])}
              >
                <ListPlus size={14} /> {template.kind}
              </Button>
            ))}
          </div>
          {steps.length ? (
            <div className="flex flex-wrap items-center gap-1 text-sm">
              {steps.map((step, index) => (
                <span key={index} className="flex items-center gap-1">
                  {index > 0 ? <ArrowRight size={12} /> : null}
                  <button type="button" onClick={() => setSteps((current) => current.filter((_, i) => i !== index))}>
                    {stepLabel(step)}
                  </button>
                </span>
              ))}
            </div>
          ) : (
            <p className="muted text-sm">{t("recipes.noSteps")}</p>
          )}
          <div>
            <Button size="sm" variant="primary" onClick={() => void save()}>
              <Save size={14} /> {t("recipes.saveRecipe")}
            </Button>
          </div>
        </Card>

        <Card className="p-5 flex flex-col gap-2">
          <strong>{t("recipes.savedTitle")}</strong>
          {recipes.length === 0 ? (
            <p className="muted text-sm">{t("recipes.noneSaved")}</p>
          ) : (
            recipes.map((recipe) => (
              <div key={recipe.id} className="flex items-center gap-2 flex-wrap">
                <span className="font-medium">{recipe.name}</span>
                <span className="text-xs muted">{(recipe.steps ?? []).map(stepLabel).join(" → ")}</span>
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => void run(recipe)}>
                  <Play size={13} /> {t("recipes.run")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => void remove(recipe)}>
                  <Trash2 size={13} />
                </Button>
              </div>
            ))
          )}
        </Card>

        <Card className="p-5 flex flex-col gap-3">
          <strong>{t("recipes.watchTitle")}</strong>
          <Field label={t("recipes.watchFolder")}>
            <TextInput value={watch.folder} onChange={(event) => setWatch({ ...watch, folder: event.target.value })} />
          </Field>
          <Field label={t("recipes.watchOutput")}>
            <TextInput
              value={watch.outputDir}
              onChange={(event) => setWatch({ ...watch, outputDir: event.target.value })}
            />
          </Field>
          <Field label={t("recipes.watchRecipe")}>
            <select
              className="input"
              value={watch.recipeId}
              onChange={(event) => setWatch({ ...watch, recipeId: event.target.value })}
            >
              <option value="">—</option>
              {recipes.map((recipe) => (
                <option key={recipe.id} value={recipe.id}>
                  {recipe.name}
                </option>
              ))}
            </select>
          </Field>
          <div className="flex flex-wrap gap-1">
            <Button size="sm" variant="ghost" onClick={() => void saveWatch()}>
              <FolderOpen size={14} /> {t("common.save")}
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void scanWatch()}>
              {t("recipes.watchScanNow")}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void startWatch()}>
              {t("recipes.watchStart")}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void stopWatch()}>
              {t("recipes.watchStop")}
            </Button>
          </div>
        </Card>
      </div>
    </Screen>
  );
}
