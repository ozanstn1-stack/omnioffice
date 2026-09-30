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
        manualChunks: {
          react: ["react", "react-dom"],
          state: ["zustand"],
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
  },
});
