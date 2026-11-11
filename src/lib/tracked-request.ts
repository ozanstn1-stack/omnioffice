import { trackJobInvocation } from "./job-retries";
import { call, type Commands } from "./typed-call";
import type { CommandResult } from "./typed-ipc";

/**
 * A long job through its generated wrapper (3.4). The retry record is written
 * first with the exact `{ request }` the wrapper sends, so the Jobs screen can
 * re-run it after a restart; `command` must be the name that wrapper invokes
 * (api-tracked.test.ts checks every tracked job). Kept out of typed-call.ts:
 * job-retries -> jobs -> typed-call would otherwise import itself.
 */
export function trackedRequest<R, T>(
  command: string,
  request: R,
  run: (commands: Commands, request: R) => Promise<CommandResult<T, unknown>>,
): Promise<T> {
  trackJobInvocation(command, { request });
  return call((commands) => run(commands, request));
}
