import { webAliases } from "./aliases";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  define: { "import.meta.env.VITE_WEB_MODE": "true" },
  plugins: [react()],
  resolve: { alias: webAliases },
  test: {
    include: ["web/src/**/*.test.tsx"],
    environment: "jsdom",
    globals: true,
    testTimeout: 20000,
    setupFiles: ["./web/src/test-setup.ts"],
  },
});
