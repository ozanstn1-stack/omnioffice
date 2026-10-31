/**
 * Registers the desk-task palette commands for the current file selection
 * (D13).
 *
 * The catalogue is loaded as its own chunk: the tasks are only needed once the
 * palette or a shortcut asks for them, and keeping them out of the entry bundle
 * is what holds the entry budget. Registration is idempotent by id, so a
 * re-render re-registers the same commands; cleanup removes only the ids this
 * hook owns.
 */
import { useEffect } from "react";
import { registerCommand, unregisterCommand } from "./commands";
import type { Navigate } from "./nav";

export function useDeskTaskCommands(files: string[], navigate: Navigate): void {
  useEffect(() => {
    let cancelled = false;
    let owned: string[] = [];
    void import("./desk-tasks").then(({ registerDeskTaskCommands, unregisterDeskTaskCommands }) => {
      if (cancelled) return;
      owned = registerDeskTaskCommands({
        getFiles: () => files,
        navigate,
        register: registerCommand,
        unregister: unregisterCommand,
      });
      return unregisterDeskTaskCommands;
    });
    return () => {
      cancelled = true;
      if (owned.length > 0)
        void import("./desk-tasks").then(({ unregisterDeskTaskCommands }) => {
          unregisterDeskTaskCommands(owned, unregisterCommand);
        });
    };
  }, [files, navigate]);
}
