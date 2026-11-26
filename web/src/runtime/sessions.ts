import type { TranscriptChunk } from "@/types";
import { requestResponse } from "../api";

const requests = new Set<AbortController>();

export function cancelSessionRequests() {
  requests.forEach((controller) => controller.abort());
  requests.clear();
}

export async function streamSession(
  token: string,
  args: Record<string, unknown>,
) {
  const channel = args.onChunk as {
    onmessage?: (chunk: TranscriptChunk) => void;
  };
  const controller = new AbortController();
  requests.add(controller);
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  try {
    const response = await requestResponse(token, "/sessions/stream", {
      method: "POST",
      body: JSON.stringify({
        providerId: args.providerId,
        sourcePath: args.sourcePath,
      }),
      signal: controller.signal,
    });
    reader = response.body?.getReader();
    if (!reader) throw new Error("会话读取失败");
    const decoder = new TextDecoder();
    let buffer = "";
    while (true) {
      const { value, done } = await reader.read();
      controller.signal.throwIfAborted();
      if (done) throw new Error("会话传输中断，请重新读取");
      buffer += decoder.decode(value, { stream: true });
      let end: number;
      while ((end = buffer.indexOf("\n")) !== -1) {
        const line = buffer.slice(0, end);
        buffer = buffer.slice(end + 1);
        if (!line.trim()) continue;
        const chunk = JSON.parse(line) as TranscriptChunk;
        channel.onmessage?.(chunk);
        if (chunk.type === "error") throw new Error(chunk.message);
        if (chunk.type === "done") return;
      }
    }
  } finally {
    requests.delete(controller);
    await reader?.cancel().catch(() => {});
    reader?.releaseLock();
  }
}

export async function sessionImage(
  token: string,
  args: Record<string, unknown>,
) {
  const controller = new AbortController();
  requests.add(controller);
  try {
    const response = await requestResponse(token, "/sessions/image", {
      method: "POST",
      body: JSON.stringify(args),
      signal: controller.signal,
    });
    return await response.arrayBuffer();
  } finally {
    requests.delete(controller);
  }
}

export function downloadMarkdown(name: string, content: string) {
  const filename =
    name
      .split(/[\\/]/)
      .pop()
      ?.replace(/[\x00-\x1f\x7f]/g, "") || "session.md";
  const url = URL.createObjectURL(
    new Blob([content], { type: "text/markdown;charset=utf-8" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
  return filename;
}
