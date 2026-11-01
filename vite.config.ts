import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed port and does not want the screen cleared on restart.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
  build: {
    target: "chrome110",
    sourcemap: false,
    // Screens are loaded lazily (see src/App.tsx); the vendor chunk keeps the
    // React runtime cacheable across releases. The budget is enforced by
    // scripts/check-bundle-budget.mjs in CI.
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (/node_modules[\\/]react(?:-dom)?[\\/]/.test(id)) return "react";
          if (/node_modules[\\/]zustand[\\/]/.test(id)) return "state";
          // The spreadsheet function library (and the value types it shares
          // with the evaluator) is a leaf: it imports nothing from the editor,
          // so it can sit in its own chunk and keep the workspace chunk under
          // the per-chunk budget.
          if (/src[\\/]office[\\/]calc[\\/]functions[\\/]/.test(id)) return "calc-functions";
          return undefined;
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    setupFiles: ["./src/test-setup.ts"],
    restoreMocks: true,
    // The component tests drive real editors with userEvent; on a loaded
    // machine (or while cargo tests run next to them) the default 5 s timeout
    // is tight enough to fail a passing test.
    testTimeout: 20_000,
    // Coverage thresholds are ratchet floors: raise them as coverage grows,
    // never lower them to make a red build green.
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "json-summary", "html"],
      reportsDirectory: "coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.test.{ts,tsx}", "src/test-setup.ts"],
      // Ratchet floors, re-measured on the full suite (219 files / 2472 tests,
      // 2026-11-01): statements 78.4, branches 80.2, functions 65.9, lines 78.4.
      // Every floor sits one point under its measurement so run-to-run jitter
      // cannot turn a green suite red. Raise them as coverage grows; never
      // lower one to make a red build green - write the missing test instead.
      thresholds: {
        statements: 77,
        branches: 79,
        functions: 64,
        lines: 77,
      },
    },
  },
});
