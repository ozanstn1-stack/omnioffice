/**
 * Home grid and settings navigation: the smoke run every PR does.
 */
import { execute, screenshot, sleep, waitForScript, withSession } from "../lib/driver.mjs";

withSession.index = 0;

export async function homeAndSettings(session) {
  // The home screen renders the tool grid once settings and recents loaded.
  const cards = await waitForScript(
    session,
    "return document.querySelectorAll('.tool-card').length",
    30000,
    "tool grid",
  );
  if (cards < 10) throw new Error(`expected the full tool grid, found ${cards} cards`);

  // Navigation: click Settings in the sidebar (or in the drawer when the
  // window is narrow) and wait for its form.
  await execute(
    session,
    "const menu=document.querySelector('header button[aria-label]'); if(menu && !document.querySelector('.nav-item')) menu.click(); return true;",
  );
  await sleep(500);
  const clickResult = await execute(
    session,
    "const item=[...document.querySelectorAll('.nav-item')].find((el)=>/Settings|Ayarlar/i.test(el.textContent||'')); if(item) item.click(); return JSON.stringify({clicked: Boolean(item), navItems: document.querySelectorAll('.nav-item').length});",
  );
  try {
    await waitForScript(
      session,
      "return document.querySelectorAll('.input').length > 0 && /Settings|Ayarlar/i.test(document.querySelector('h1')?.textContent || '')",
      15000,
      "settings screen",
    );
  } catch (error) {
    throw new Error(`${error.message}; click: ${clickResult}`);
  }
  await screenshot(session, "settings");
  console.log(`PASS: home grid (${cards} cards), settings navigation`);
}
