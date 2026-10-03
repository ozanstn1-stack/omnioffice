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
 * V3.2 promoted the entire backlog to errors: correctness rules, the React
 * Compiler diagnostics (eslint-plugin-react-hooks v7) and the jsx-a11y
 * interaction rules all fail the build now. The code base is clean, so there
 * is no warning budget left to freeze.
 *
 * Type-aware linting is deliberately off so `npm run lint` stays fast enough
 * for every commit; `tsc --noEmit` already covers the type-level checks.
 */

// React Compiler diagnostics from eslint-plugin-react-hooks v7. Every rule is
// an error now that the code base passes them.
const reactCompilerRules = {
  "react-hooks/static-components": "error",
  "react-hooks/use-memo": "error",
  "react-hooks/preserve-manual-memoization": "error",
  "react-hooks/incompatible-library": "error",
  "react-hooks/immutability": "error",
  "react-hooks/globals": "error",
  "react-hooks/refs": "error",
  "react-hooks/set-state-in-effect": "error",
  "react-hooks/error-boundaries": "error",
  "react-hooks/purity": "error",
  "react-hooks/set-state-in-render": "error",
  "react-hooks/unsupported-syntax": "error",
  "react-hooks/config": "error",
  "react-hooks/gating": "error",
  "react-hooks/void-use-memo": "error",
};

// jsx-a11y's recommended set as errors (its default severity), explicit here
// so the whole accessibility contract is visible in one place.
const a11yAsErrors = Object.fromEntries(
  Object.entries(jsxA11y.flatConfigs.recommended.rules).map(([rule, value]) => [
    rule,
    Array.isArray(value) ? ["error", ...value.slice(1)] : "error",
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
      "react-hooks/exhaustive-deps": "error",
      ...reactCompilerRules,
      ...a11yAsErrors,
      // `label-has-for` is deprecated in eslint-plugin-jsx-a11y (v6.3) and is
      // scheduled for removal in v7; `label-has-associated-control` replaces
      // it and is already part of the recommended (error) set above.
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
    files: ["scripts/**/*.mjs", "e2e/**/*.mjs", "*.config.{js,ts}", "vite.config.ts"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    // The sample plugin runs inside the sandboxed Web Worker.
    files: ["plugins/**/*.js"],
    languageOptions: { globals: { ...globals.worker } },
  },
];
