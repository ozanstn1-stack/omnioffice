/**
 * Application command ACL: an unknown command is refused and a granted one still answers.
 */
import { executeAsync, screenshot } from "../lib/driver.mjs";

/**
 * The application command ACL (E1/4b). With the ACL manifest active the webview
 * may only invoke commands its capability allows: this proves both directions
 * against the running app - an unknown command is refused before dispatch, and
 * a granted one (`app_info`, which backs the About panel) still answers. The
 * static half lives in `src-tauri/src/command_acl.rs` and `npm run acl:check`.
 */
export async function aclGate(session) {
  const raw = await executeAsync(
    session,
    `const done = arguments[arguments.length - 1];
     const ipc = window.__TAURI_INTERNALS__;
     if (!ipc || typeof ipc.invoke !== "function") { done(JSON.stringify({ error: "no ipc bridge" })); return; }
     const outcome = (name) =>
       ipc.invoke(name).then(() => "allowed", (error) => String((error && error.message) || error));
     Promise.all([outcome("acl_probe_not_a_command"), outcome("app_info")]).then(
       ([unknown, known]) => done(JSON.stringify({ unknown, known })),
       (error) => done(JSON.stringify({ error: String(error) })),
     );`,
  );
  const result = JSON.parse(raw);
  if (result.error) throw new Error(`the ACL scenario could not reach the IPC bridge: ${result.error}`);
  if (result.known !== "allowed") throw new Error(`app_info must be allowed by the ACL, got: ${result.known}`);
  if (!/not allowed|not found/i.test(result.unknown)) {
    throw new Error(`an unknown command must be refused by the ACL, got: ${result.unknown}`);
  }
  await screenshot(session, "acl");
  console.log(`PASS: the command ACL refused an unknown command and allowed app_info`);
}
