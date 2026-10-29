export function getCurrentWindow(): never {
  throw new Error("浏览器没有桌面窗口控制能力");
}
