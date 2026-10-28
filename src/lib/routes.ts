/**
 * The single route manifest (v6.0.1 audit, B1).
 *
 * Before this file the application kept three hand-written lists that drifted:
 * the command palette, the sidebar and the Home cards. Now the screen ids, the
 * sidebar sections, the palette entries and the feature-flag gates all come
 * from here; `App.tsx` only supplies the components (and the icons, which are
 * presentation) and `Home.tsx` its card copy.
 *
 * The `ScreenId` type is derived from the runtime list, so "a screen exists"
 * and "a screen is in the manifest" can never disagree: an id missing from
 * `ROUTES` fails the compile (`Record<ScreenId, ...>` in App.tsx) and the
 * `routes.test.ts` coverage test.
 */
import { flagEnabled, type FeatureFlag } from "./flags";

export const SCREEN_IDS = [
  "home",
  "reader",
  "ai",
  "aiLibrary",
  "merge",
  "organize",
  "split",
  "compress",
  "ocr",
  "pdfToImages",
  "imagesToPdf",
  "protect",
  "unlock",
  "watermark",
  "annotate",
  "redact",
  "compare",
  "inspect",
  "metadata",
  "pageTools",
  "batch",
  "history",
  "settings",
  "info",
  "office",
  "documents",
  "spreadsheets",
  "presentations",
  "notes",
  "templates",
  "converter",
  "invoices",
  "eml",
  "scanner",
  "recipes",
  "mailMerge",
  "cleaner",
  "draw",
  "planner",
  "data",
  "pdfForms",
  "pdfStudio",
  "vault",
  "sync",
  "compat",
  "plugins",
  "jobs",
  "petition",
  "deadline",
  "kvkk",
  "documentHealth",
] as const;

export type ScreenId = (typeof SCREEN_IDS)[number];

/** Sidebar section ids (`labelKey` is the visible heading, if any). */
export const NAV_SECTIONS = [
  { id: "top", labelKey: undefined },
  { id: "office", labelKey: "nav.office" },
  { id: "productivity", labelKey: "nav.productivity" },
  { id: "convert", labelKey: "nav.convert" },
  { id: "pdfTools", labelKey: "nav.pdfTools" },
  { id: "images", labelKey: "nav.convert" },
  { id: "ocr", labelKey: undefined },
  { id: "security", labelKey: "nav.security" },
  { id: "ai", labelKey: "nav.ai" },
  { id: "system", labelKey: "nav.batch" },
] as const;

export type NavSectionId = (typeof NAV_SECTIONS)[number]["id"];

export interface RouteDef {
  id: ScreenId;
  /** i18n key used by the sidebar, the palette and the Home cards. */
  labelKey: string;
  /** Sidebar section; `null` keeps the route out of the sidebar. */
  section: NavSectionId | null;
  /** Command-palette category. */
  category: "view" | "settings";
  /** The route is listed (and reachable) only while this flag is on. */
  flag?: FeatureFlag;
  /** Command-palette keybinding hint, if any. */
  keybinding?: string;
}

const route = (
  id: ScreenId,
  section: NavSectionId | null,
  extra: Partial<Pick<RouteDef, "category" | "flag" | "keybinding">> = {},
): RouteDef => ({
  id,
  labelKey: `nav.${id}`,
  section,
  category: extra.category ?? "view",
  ...(extra.flag ? { flag: extra.flag } : {}),
  ...(extra.keybinding ? { keybinding: extra.keybinding } : {}),
});

export const ROUTES: readonly RouteDef[] = [
  route("home", "top"),
  route("office", "top"),
  route("reader", "top"),
  route("documents", "office"),
  route("spreadsheets", "office"),
  route("presentations", "office"),
  route("templates", "office"),
  route("petition", "office"),
  route("deadline", "office"),
  route("kvkk", "office", { flag: "experimentalKvkkWizard" }),
  route("notes", "productivity"),
  route("planner", "productivity"),
  route("data", "productivity"),
  route("draw", "productivity"),
  route("vault", "productivity"),
  route("converter", "convert"),
  route("invoices", "convert"),
  route("eml", "convert"),
  route("scanner", "convert"),
  route("recipes", "convert"),
  route("mailMerge", "convert"),
  route("cleaner", "convert"),
  route("pdfForms", "convert"),
  route("merge", "pdfTools"),
  route("organize", "pdfTools"),
  route("split", "pdfTools"),
  route("compress", "pdfTools"),
  route("pageTools", "pdfTools"),
  route("pdfStudio", "pdfTools"),
  route("documentHealth", "pdfTools", { flag: "experimentalDocumentHealth" }),
  route("watermark", "pdfTools"),
  route("annotate", "pdfTools"),
  route("redact", "pdfTools"),
  route("compare", "pdfTools"),
  route("inspect", "pdfTools"),
  route("metadata", "pdfTools"),
  route("pdfToImages", "images"),
  route("imagesToPdf", "images"),
  route("ocr", "ocr"),
  route("protect", "security"),
  route("unlock", "security"),
  route("ai", "ai"),
  route("aiLibrary", "ai"),
  route("batch", "system"),
  route("jobs", "system"),
  route("sync", "system"),
  route("plugins", "system"),
  route("compat", "system"),
  route("info", "system"),
  route("history", "system"),
  route("settings", "system", { category: "settings", keybinding: "Ctrl+," }),
];

const BY_ID = new Map<ScreenId, RouteDef>(ROUTES.map((entry) => [entry.id, entry]));

/** The manifest entry for `id` (the compile-time maps guarantee one exists). */
export function routeFor(id: ScreenId): RouteDef {
  const entry = BY_ID.get(id);
  if (!entry) {
    // Unreachable: `Record<ScreenId, ...>` exhaustiveness and the coverage
    // test keep the manifest in sync with the id list.
    throw new Error(`route missing from the manifest: ${id}`);
  }
  return entry;
}

/** True when the route's feature flag (if any) is on. */
export function routeEnabled(id: ScreenId): boolean {
  const flag = routeFor(id).flag;
  return flag ? flagEnabled(flag) : true;
}

/** Routes eligible for the sidebar and the command palette, in manifest order. */
export function enabledRoutes(): RouteDef[] {
  return ROUTES.filter((entry) => routeEnabled(entry.id));
}

/** Sidebar entries grouped by section (empty sections are dropped). */
export function navSections(): { id: NavSectionId; labelKey?: string; routes: RouteDef[] }[] {
  return NAV_SECTIONS.map((section) => ({
    id: section.id,
    ...(section.labelKey ? { labelKey: section.labelKey } : {}),
    routes: enabledRoutes().filter((entry) => entry.section === section.id),
  })).filter((section) => section.routes.length > 0);
}
