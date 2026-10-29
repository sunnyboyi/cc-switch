// Tool commands describe the server, even when the browser runs on another OS.
let platform = "";
export function setServerPlatform(value: string) {
  platform = value;
}
export const isWindows = () => platform === "windows";
export const isMac = () => platform === "macos";
export const isLinux = () => platform === "linux";
export const DRAG_REGION_ENABLED = false;
export const DRAG_REGION_ATTR = {};
export const DRAG_REGION_STYLE = {};
