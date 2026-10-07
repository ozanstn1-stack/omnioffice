/**
 * Loads the built-in function library.
 *
 * Each module registers its own functions on import; this is the single place
 * that pulls them all in, so adding a function is "write it, import it here"
 * rather than "append it to a 2000-line file".
 */
import { functionCount } from "../registry";
import "./arrays";
import "./datetime";
import "./financial";
import "./lookup";
import "./math";
import "./reference";
import "./statistics";
import "./text";

let loaded = false;

/** Registers every built-in function exactly once. */
export function registerBuiltinFunctions(): void {
  if (loaded) return;
  loaded = true;
}

/** Number of registered functions, for the About screen and the tests. */
export function builtinFunctionCount(): number {
  registerBuiltinFunctions();
  return functionCount();
}
