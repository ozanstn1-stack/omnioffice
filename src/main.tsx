import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ensureLanguage } from "./lib/i18n";
import { preloadTypedCommands } from "./lib/typed-call";
import "./styles.css";

const splash = document.getElementById("root");
if (splash) {
  splash.style.height = "100%";
}

// Frontend diagnostics: errors are written to the app log folder (never
// document content, never passwords).
const report = (level: string, message: string) => {
  void import("./lib/api").then(({ logFrontend }) => logFrontend(level, message));
};
window.addEventListener("error", (event) => {
  report("error", `${event.message} @ ${event.filename}:${event.lineno}:${event.colno}\n${event.error?.stack ?? ""}`);
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason as { message?: string; stack?: string } | string;
  report("rejection", typeof reason === "string" ? reason : `${reason?.message ?? "unknown"}\n${reason?.stack ?? ""}`);
});

report(
  "info",
  `viewport innerWidth=${window.innerWidth} innerHeight=${window.innerHeight} dpr=${window.devicePixelRatio} screen=${window.screen.width}x${window.screen.height} avail=${window.screen.availWidth}x${window.screen.availHeight}`,
);

// The English dictionary is a JSON asset (not a JS chunk), so wait for it
// before the first render; a failed load falls back to raw keys instead of
// leaving a blank window. The typed command wrappers (#316) load alongside,
// so the first screen's calls do not wait on their chunk.
async function start() {
  const typed = preloadTypedCommands().catch(() => undefined);
  try {
    await ensureLanguage("en");
  } catch {
    // The UI is still usable; the audit covers missing strings.
  }
  await typed;
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {/* Last resort: a crash in the shell itself must not leave a blank window. */}
      <ErrorBoundary scope="app">
        <App />
      </ErrorBoundary>
    </React.StrictMode>,
  );
}

void start();
