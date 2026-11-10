/**
 * Home grid and settings navigation: the smoke run every PR does.
 */
import { execute, screenshot, sleep, waitForScript, withSession } from "../lib/driver.mjs";

withSession.index = 0;

/** Opens the phone drawer when the window is narrow (no sidebar entries yet). */
const OPEN_DRAWER =
  "const menu=document.querySelector('header button[aria-label]'); if(menu && !document.querySelector('.nav-item')) menu.click(); return true;";

/** Clicks the first closed sidebar hub and returns its id. */
const CLICK_FIRST_CLOSED_HUB =
  "const t=document.querySelector('.nav-hub-toggle[aria-expanded=\\'false\\']'); if(!t) return ''; t.click(); return t.getAttribute('aria-controls');";

/** Whether the hub panel `id` is open now. */
function hubOpen(id) {
  return `const p=document.getElementById(${JSON.stringify(id)}); const t=document.querySelector('[aria-controls=${JSON.stringify(id)}]'); return Boolean(p && !p.hidden && t && t.getAttribute('aria-expanded')==='true');`;
}

/** Clicks the sidebar or drawer entry whose text matches `pattern`. */
function clickNavItem(pattern) {
  return `const item=[...document.querySelectorAll('.nav-item')].find((el)=>/^(${pattern})$/i.test((el.textContent||'').trim())); if(item) item.click(); return Boolean(item);`;
}

export async function homeAndSettings(session) {
  // The home screen renders the tool grid once settings and recents loaded.
  const cards = await waitForScript(
    session,
    "return document.querySelectorAll('.tool-card').length",
    30000,
    "tool grid",
  );
  if (cards < 10) throw new Error(`expected the full tool grid, found ${cards} cards`);

  // 7.31 hub sidebar: a closed hub opens on click and its entries show.
  await execute(session, OPEN_DRAWER);
  await sleep(300);
  const hub = String(await execute(session, CLICK_FIRST_CLOSED_HUB));
  if (!hub) throw new Error("no closed sidebar hub to open");
  await sleep(300);
  if (!(await execute(session, hubOpen(hub)))) throw new Error(`sidebar hub ${hub} did not open`);

  // 7.32 Tools hub: every tool as a tile.
  await execute(session, clickNavItem("Tools|Araçlar"));
  const tiles = await waitForScript(
    session,
    "const n=document.querySelectorAll('.tool-tile').length; return n >= 40 ? n : 0",
    15000,
    "tools hub",
  );

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
  console.log(
    `PASS: home grid (${cards} cards), sidebar hub (${hub}), tools hub (${tiles} tiles), settings navigation`,
  );
}
