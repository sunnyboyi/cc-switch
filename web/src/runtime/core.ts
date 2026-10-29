import { request } from "../api";
import { generateUUID } from "@/utils/uuid";

let token = "";
const uploads = new Map<string, File>();
const listeners = new EventTarget();

export function setAccessToken(value: string) {
  token = value;
  uploads.clear();
}

export function getAccessToken() {
  return token;
}

export function isTauri() {
  return false;
}

// Session IPC channels are outside the web management feature set.
export class Channel<T> {
  onmessage?: (message: T) => void;
  constructor() {
    throw new Error("Web 管理入口尚不支持桌面 IPC Channel");
  }
}

export async function command<T>(
  name: string,
  args: Record<string, unknown> = {},
  expectedRevision?: string,
): Promise<T> {
  try {
    return await request<T>(token, `/command/${encodeURIComponent(name)}`, {
      method: "POST",
      body: JSON.stringify({
        args,
        ...(expectedRevision ? { expected_revision: expectedRevision } : {}),
      }),
    });
  } catch (error) {
    if (
      error &&
      typeof error === "object" &&
      "status" in error &&
      error.status === 401
    ) {
      window.dispatchEvent(new Event("web-session-expired"));
    }
    throw error;
  }
}

async function chooseZip(): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".zip,application/zip";
    input.addEventListener("cancel", () => resolve(null), { once: true });
    input.addEventListener(
      "change",
      () => {
        const file = input.files?.[0];
        if (!file) {
          resolve(null);
          return;
        }
        const id = generateUUID();
        uploads.set(id, file);
        resolve(id);
      },
      { once: true },
    );
    input.click();
  });
}

export async function invoke<T>(
  name: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (name === "update_tray_menu") {
    // Shared mutation hooks refresh desktop chrome. In this entry, refresh the
    // browser queries instead; there is no tray process to contact.
    window.dispatchEvent(new Event("web-refresh"));
    return true as T;
  }
  if (name === "open_external") {
    const url = new URL(String(args.url));
    if (!["https:", "http:"].includes(url.protocol))
      throw new Error("只允许打开 HTTP 或 HTTPS 链接");
    window.open(url.href, "_blank", "noopener,noreferrer");
    return true as T;
  }
  if (name === "copy_text_to_clipboard") {
    await navigator.clipboard.writeText(String(args.text));
    return undefined as T;
  }
  if (name === "open_zip_file_dialog") return (await chooseZip()) as T;
  if (name === "install_skills_from_zip") {
    const file = uploads.get(String(args.filePath));
    if (!file) throw new Error("请重新选择要上传的 ZIP 文件");
    if (file.size > 25 * 1024 * 1024) throw new Error("ZIP 文件不能超过 25 MB");
    const result = await request<T>(
      token,
      `/skills/upload/${encodeURIComponent(String(args.currentApp))}`,
      { method: "POST", body: file },
    );
    uploads.delete(String(args.filePath));
    return result;
  }
  if (
    [
      "open_skill_dir",
      "open_cc_switch_skills_dir",
      "open_config_folder",
      "open_app_config_folder",
      "open_model_pricing_folder",
    ].includes(name)
  ) {
    const path = await command<string>("web_directory", {
      kind: name,
      ...args,
    });
    window.dispatchEvent(
      new CustomEvent("web-show-directory", { detail: path }),
    );
    return true as T;
  }
  return command<T>(name, args);
}

export function emitLocalEvent(name: string, payload: unknown) {
  listeners.dispatchEvent(new CustomEvent(name, { detail: payload }));
}

export async function listen<T>(
  name: string,
  handler: (event: { event: string; id: number; payload: T }) => void,
) {
  const callback = (event: Event) =>
    handler({ event: name, id: 0, payload: (event as CustomEvent<T>).detail });
  listeners.addEventListener(name, callback);
  return () => listeners.removeEventListener(name, callback);
}

/** Authenticated SSE uses headers; access tokens never appear in URLs. */
export function connectChanges(
  onChange: () => void,
  onStatus: (connected: boolean) => void,
) {
  const controller = new AbortController();
  let retry: ReturnType<typeof setTimeout> | undefined;
  async function connect() {
    try {
      const response = await fetch("/api/events", {
        headers: { Authorization: `Bearer ${token}` },
        cache: "no-store",
        credentials: "omit",
        signal: controller.signal,
      });
      if (response.status === 401) {
        window.dispatchEvent(new Event("web-session-expired"));
        return;
      }
      if (!response.ok || !response.body) throw new Error("事件连接失败");
      onStatus(true);
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (!controller.signal.aborted) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true }).replace(/\r/g, "");
        let end: number;
        while ((end = buffer.indexOf("\n\n")) !== -1) {
          const frame = buffer.slice(0, end);
          buffer = buffer.slice(end + 2);
          const line = frame
            .split("\n")
            .find((item) => item.startsWith("data:"));
          if (!line) continue;
          const event = JSON.parse(line.slice(5)) as {
            command: string;
            scope?: string;
          };
          if (event.command === "apply_profile")
            emitLocalEvent("profile-applied", { scope: event.scope });
          onChange();
        }
      }
    } catch {
      // Show reconnecting state and refresh after reconnect, including missed events.
    } finally {
      if (!controller.signal.aborted) {
        onStatus(false);
        retry = setTimeout(() => void connect(), 2000);
      }
    }
  }
  void connect();
  return () => {
    controller.abort();
    clearTimeout(retry);
  };
}
