/** Document cleaner: metadata, hidden layers and sanitization. */
import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Wand2 } from "lucide-react";
import { useT } from "../../lib/i18n";
import { reportError } from "../../lib/store";
import { isAndroid, pickOfficeFiles, publishOutputs } from "../../lib/mobile";
import * as api from "../../lib/office-api";
import { Screen } from "../../components/layout";

// ---------------------------------------------------------------------------
// Document cleaner
// ---------------------------------------------------------------------------

export function CleanerScreen() {
  const t = useT();
  const [path, setPath] = useState("");
  const [options, setOptions] = useState({
    removeMetadata: true,
    removeComments: false,
    optimizeImages: true,
    imageMaxPixels: 1600,
    imageQuality: 82,
  });
  const [footprint, setFootprint] = useState<number | null>(null);
  const [result, setResult] = useState<api.CleanResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [showCleaner, setShowCleaner] = useState(true);
  void setShowCleaner;

  const pick = async () => {
    let selection: string | null = null;
    if (isAndroid()) {
      const [picked] = await pickOfficeFiles(false).catch(() => []);
      selection = picked ?? null;
    } else {
      const chosen = await openDialog({
        multiple: false,
        filters: [{ name: "Office documents", extensions: ["docx", "xlsx", "pptx", "odt", "ods", "odp"] }],
      });
      selection = typeof chosen === "string" ? chosen : null;
    }
    if (selection) {
      setPath(selection);
      setFootprint(await api.imageFootprint(selection).catch(() => null));
    }
  };

  const run = async () => {
    if (!path) return;
    setBusy(true);
    try {
      const cleaned = await api.cleanDocument(path, options);
      setResult(cleaned);
      setFootprint(await api.imageFootprint(path).catch(() => null));
      // Android: the cleaned document lives in the app cache; copy it to a
      // visible location so the user can open or share it.
      if (isAndroid()) await publishOutputs([path]);
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen
      title={t("cleaner.title")}
      subtitle={t("cleaner.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={pick}>
            {t("cleaner.pickFile")}
          </button>
          <button type="button" className="btn btn-primary" onClick={run} disabled={!path || busy}>
            <Wand2 size={15} /> {t("cleaner.clean")}
          </button>
        </div>
      }
    >
      {showCleaner ? (
        <div className="card">
          <p className="mono">{path || t("cleaner.noFile")}</p>
          {footprint !== null ? (
            <p className="muted">
              {t("cleaner.images")}: {(footprint / 1024).toFixed(0)} KB
            </p>
          ) : null}
          <div className="stack">
            <label className="check">
              <input
                type="checkbox"
                checked={options.removeMetadata}
                onChange={(event) => setOptions({ ...options, removeMetadata: event.target.checked })}
              />{" "}
              {t("cleaner.removeMetadata")}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={options.removeComments}
                onChange={(event) => setOptions({ ...options, removeComments: event.target.checked })}
              />{" "}
              {t("cleaner.removeComments")}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={options.optimizeImages}
                onChange={(event) => setOptions({ ...options, optimizeImages: event.target.checked })}
              />{" "}
              {t("cleaner.optimizeImages")}
            </label>
            <div className="row">
              <label className="field">
                <span>{t("cleaner.maxPixels")}</span>
                <input
                  type="number"
                  value={options.imageMaxPixels}
                  onChange={(event) => setOptions({ ...options, imageMaxPixels: Number(event.target.value) })}
                />
              </label>
              <label className="field">
                <span>{t("cleaner.quality")}</span>
                <input
                  type="number"
                  min={40}
                  max={95}
                  value={options.imageQuality}
                  onChange={(event) => setOptions({ ...options, imageQuality: Number(event.target.value) })}
                />
              </label>
            </div>
          </div>
          <p className="muted">{t("cleaner.note")}</p>
        </div>
      ) : null}
      {result ? (
        <div className="card">
          <h3>{t("cleaner.results")}</h3>
          <p>
            {(result.bytesBefore / 1024).toFixed(0)} KB → {(result.bytesAfter / 1024).toFixed(0)} KB
          </p>
          <ul>
            {result.actions.map((action, index) => (
              <li key={index}>{action}</li>
            ))}
            {result.warnings.map((warning, index) => (
              <li key={`w-${index}`} className="muted">
                {warning}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </Screen>
  );
}
