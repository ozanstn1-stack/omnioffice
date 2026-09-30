import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import jsxA11y from "eslint-plugin-jsx-a11y";

/**
 * ESLint flat config for the React 19 + TypeScript frontend.
 *
 * Scope: `src/` (the Tauri frontend), the Node helper scripts and the sample
 * plugin. The Chrome extension has its own package.json, and the Rust crates
 * are covered by clippy (see .github/workflows/desktop.yml).
 *
 * Severity policy - the point of the split is that CI can stay strict:
 *
 *  * correctness rules (eslint recommended, TypeScript recommended, the two
 *    classic react-hooks rules) are **errors** and must stay at zero;
 *  * the React Compiler rules that ship in eslint-plugin-react-hooks v7 and
 *    the jsx-a11y interaction rules are **warnings**. They are a real backlog
 *    (mostly `setState` inside effects and clickable non-interactive
 *    elements), but fixing them is a refactor, not a lint change, so they run
 *    under a frozen warning budget: `npm run lint` fails if the count grows.
 *
 * Type-aware linting is deliberately off so `npm run lint` stays fast enough
 * for every commit; `tsc --noEmit` already covers the type-level checks.
 */

// React Compiler diagnostics from eslint-plugin-react-hooks v7. Each one is
// accurate, and each one needs a code change before it can become an error.
const reactCompilerBacklog = {
  "react-hooks/static-components": "warn",
  "react-hooks/use-memo": "warn",
  "react-hooks/preserve-manual-memoization": "warn",
  "react-hooks/incompatible-library": "warn",
  "react-hooks/immutability": "warn",
  "react-hooks/globals": "warn",
  "react-hooks/refs": "warn",
  "react-hooks/set-state-in-effect": "warn",
  "react-hooks/error-boundaries": "warn",
  "react-hooks/purity": "warn",
  "react-hooks/set-state-in-render": "warn",
  "react-hooks/unsupported-syntax": "warn",
  "react-hooks/config": "warn",
  "react-hooks/gating": "warn",
  "react-hooks/void-use-memo": "warn",
};

// jsx-a11y ships its recommended set as errors; the interaction rules are
// downgraded to warnings until the clickable `div`s in the editors and PDF
// screens carry a role and a keyboard handler.
const a11yAsWarnings = Object.fromEntries(
  Object.entries(jsxA11y.flatConfigs.recommended.rules).map(([rule, value]) => [
    rule,
    Array.isArray(value) ? ["warn", ...value.slice(1)] : "warn",
  ]),
);

export default [
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "target/**",
      "release-artifacts/**",
      "src-tauri/**",
      "chrome-extension/**",
      "crates/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
      globals: { ...globals.browser },
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
      "jsx-a11y": jsxA11y,
    },
    rules: {
      // The two rules React itself ships as the baseline contract.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      ...reactCompilerBacklog,
      ...a11yAsWarnings,
      // Promoted to errors after the V3.1.1 accessibility pass. The remaining
      // interaction backlog (`click-events-have-key-events`,
      // `no-static-element-interactions`, `control-has-associated-label`) is
      // frozen per rule in scripts/lint-baseline.json and must be burned down
      // in V3.2 before those rules are promoted too.
      "jsx-a11y/no-autofocus": "error",
      "jsx-a11y/no-noninteractive-element-interactions": "error",
      "jsx-a11y/no-noninteractive-tabindex": "error",
      "jsx-a11y/interactive-supports-focus": "error",
      // `label-has-for` is deprecated in eslint-plugin-jsx-a11y (v6.3) and is
      // scheduled for removal in v7; `label-has-associated-control` replaces
      // it and belongs to the same accessibility backlog as the interaction
      // rules above.
      "jsx-a11y/label-has-for": "off",
      // Fast Refresh granularity only (it never fires in the production
      // bundle): many modules intentionally export a screen plus its helpers.
      "react-refresh/only-export-components": "off",
      // `tsc` already enforces noUnusedLocals/noUnusedParameters; this keeps
      // the ESLint view consistent and allows `_` for deliberately ignored
      // values.
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
    },
  },
  {
    files: ["src/**/*.test.{ts,tsx}", "src/test-setup.ts"],
    languageOptions: {
      globals: {
        ...globals.browser,
        describe: "readonly",
        it: "readonly",
        test: "readonly",
        expect: "readonly",
        vi: "readonly",
        beforeAll: "readonly",
        beforeEach: "readonly",
        afterAll: "readonly",
        afterEach: "readonly",
      },
    },
  },
  {
    files: ["scripts/**/*.mjs", "*.config.{js,ts}", "vite.config.ts"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    // The sample plugin runs inside the sandboxed Web Worker.
    files: ["plugins/**/*.js"],
    languageOptions: { globals: { ...globals.worker } },
  },
];
