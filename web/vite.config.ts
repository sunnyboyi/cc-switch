import path from "node:path";
import { createRequire } from "node:module";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "tailwindcss";
import autoprefixer from "autoprefixer";
import { webAliases } from "./aliases";

const desktopTheme = createRequire(import.meta.url)("../tailwind.config.cjs");

export default defineConfig({
  root: path.resolve(__dirname),
  plugins: [react()],
  define: { "import.meta.env.VITE_WEB_MODE": "true" },
  resolve: { alias: webAliases },
  css: {
    postcss: {
      plugins: [
        tailwindcss({
          ...desktopTheme,
          content: [
            path.resolve(__dirname, "index.html"),
            path.resolve(__dirname, "src/**/*.{ts,tsx}"),
            path.resolve(__dirname, "../src/**/*.{ts,tsx}"),
          ],
        }),
        autoprefixer(),
      ],
    },
  },
  server: {
    host: "127.0.0.1",
    port: 3001,
    strictPort: true,
    proxy: { "/api": "http://127.0.0.1:8080" },
  },
  build: { outDir: "dist", emptyOutDir: true },
});
