/**
 * The output target state for a tool session: the display path, the write
 * token issued by Rust, and the folder used by multi-output tools or Android
 * staging. Kept apart from `useTool` so the token plumbing does not grow the
 * session hook past its line budget.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appDataDir } from "@tauri-apps/api/path";
import { dirName, fileBaseName, fileStem, joinPath } from "./format";
import { isAndroid } from "./mobile";
import { suggestOutput } from "./file-tokens";

export interface OutputTarget {
  outputPath: string;
  outputToken: string;
  outputDir: string;
  /** Directory token for the folder-writing tools (PDF to images, split). */
  outputDirToken: string;
  setOutputPath: (value: string) => void;
  setOutputDir: (value: string) => void;
  /** A save target picked in the Rust dialog: token for the command, path for the UI. */
  setOutputTarget: (target: { token: string; path: string }) => void;
  /** A folder picked in the Rust dialog: token for the command, path for the UI. */
  setDirectoryTarget: (target: { token: string; path: string }) => void;
}

export function useOutputTarget(options: {
  input: string | null;
  suffix: string;
  accept: "pdf" | "image" | "document" | "any";
  isMultiOutput: boolean;
  defaultOutputDir: string;
}): OutputTarget {
  const { input, suffix, accept, isMultiOutput, defaultOutputDir } = options;
  const [outputPath, setOutputPath] = useState("");
  const [outputToken, setOutputToken] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [outputDirToken, setOutputDirToken] = useState("");

  useEffect(() => {
    if (!input) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- clearing the suggestion for an empty selection
      setOutputPath("");
      setOutputToken("");
      setOutputDir("");
      setOutputDirToken("");
      return;
    }
    let cancelled = false;
    const suggestedName = input.replace(/\.[^.\\]+$/, "") + suffix + ".pdf";
    // Android stages inside the private data directory and publishes through
    // SAF afterwards; no token is needed for the intermediate file.
    if (isAndroid()) {
      void (async () => {
        let base = defaultOutputDir;
        if (!base) {
          const appData = await appDataDir().catch(() => "");
          base = appData ? joinPath(appData, "Outputs") : dirName(input);
        }
        await invoke("ensure_dir", { path: base }).catch(() => undefined);
        if (cancelled) return;
        setOutputPath(joinPath(base, fileBaseName(suggestedName)));
        setOutputDir(base);
      })();
      return () => {
        cancelled = true;
      };
    }
    // Multi-output tools write into a folder named after the document.
    if (isMultiOutput) {
      const folder = joinPath(dirName(input), `${fileStem(input)}${suffix}`);
      void invoke("ensure_dir", { path: folder }).catch(() => undefined);
      setOutputPath(joinPath(folder, fileBaseName(suggestedName)));
      setOutputDir(folder);
      return;
    }
    // Rust issues the write token for the suggested target; the display path
    // comes back with it (a configured output folder is validated there too).
    void suggestOutput(input, suffix, defaultOutputDir)
      .then((suggestion) => {
        if (cancelled) return;
        setOutputPath(suggestion.path);
        setOutputToken(suggestion.token);
      })
      .catch(() => {
        if (cancelled) return;
        setOutputPath(suggestedName);
        setOutputToken("");
      });
    setOutputDir(defaultOutputDir || dirName(input));
    return () => {
      cancelled = true;
    };
  }, [input, suffix, accept, defaultOutputDir, isMultiOutput]);

  return {
    outputPath,
    outputToken,
    outputDir,
    outputDirToken,
    setOutputPath,
    setOutputDir: (value: string) => {
      setOutputDirToken("");
      setOutputDir(value);
    },
    setOutputTarget: (target: { token: string; path: string }) => {
      setOutputToken(target.token);
      setOutputPath(target.path);
    },
    setDirectoryTarget: (target: { token: string; path: string }) => {
      setOutputDirToken(target.token);
      setOutputDir(target.path);
    },
  };
}
