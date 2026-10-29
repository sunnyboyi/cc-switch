import path from "node:path";

export const webAliases = [
  {
    find: /^@\/lib\/platform$/,
    replacement: path.resolve(__dirname, "src/runtime/platform.ts"),
  },
  {
    find: /^@\/lib\/modelsDev$/,
    replacement: path.resolve(__dirname, "src/runtime/modelsDev.ts"),
  },
  {
    find: "@tauri-apps/api/window",
    replacement: path.resolve(__dirname, "src/runtime/window.ts"),
  },
  {
    find: "@tauri-apps/plugin-updater",
    replacement: path.resolve(__dirname, "src/runtime/updater.ts"),
  },
  {
    find: "@tauri-apps/api/core",
    replacement: path.resolve(__dirname, "src/runtime/core.ts"),
  },
  {
    find: "@tauri-apps/api/event",
    replacement: path.resolve(__dirname, "src/runtime/core.ts"),
  },
  {
    find: "@tauri-apps/api/app",
    replacement: path.resolve(__dirname, "src/runtime/app.ts"),
  },
  {
    find: "@tauri-apps/api/path",
    replacement: path.resolve(__dirname, "src/runtime/path.ts"),
  },
  {
    find: "@tauri-apps/plugin-log",
    replacement: path.resolve(__dirname, "src/runtime/log.ts"),
  },
  { find: "@", replacement: path.resolve(__dirname, "../src") },
];
