import { afterEach, expect, test, vi } from "vitest";
import { Channel, invoke, setAccessToken } from "./core";
import type { TranscriptChunk } from "@/types";

afterEach(() => {
  setAccessToken("");
  vi.unstubAllGlobals();
});

test("session chunks handle split UTF-8 frames and keep credentials out of URLs", async () => {
  const chunks = [
    { type: "header", total: 1, turns: [], cached: false, parseMs: 1 },
    { type: "messages", start: 0, messages: [{ content: "中文消息" }] },
    { type: "done", payloadBytes: 32 },
  ];
  const bytes = new TextEncoder().encode(
    chunks.map((chunk) => JSON.stringify(chunk)).join("\n") + "\n",
  );
  const fetch = vi.fn(
    async () =>
      new Response(
        new ReadableStream({
          start(controller) {
            for (let offset = 0; offset < bytes.length; offset += 5)
              controller.enqueue(bytes.slice(offset, offset + 5));
            controller.close();
          },
        }),
      ),
  );
  vi.stubGlobal("fetch", fetch);
  setAccessToken("private-fixture-token");
  const channel = new Channel<TranscriptChunk>();
  channel.onmessage = vi.fn();
  await invoke("stream_session_messages", {
    providerId: "claude",
    sourcePath: "/server/session.jsonl",
    onChunk: channel,
  });
  expect(channel.onmessage).toHaveBeenCalledTimes(3);
  chunks.forEach((chunk) =>
    expect(channel.onmessage).toHaveBeenCalledWith(chunk),
  );
  expect(fetch).toHaveBeenCalledWith(
    "/api/sessions/stream",
    expect.objectContaining({
      headers: expect.objectContaining({
        Authorization: "Bearer private-fixture-token",
      }),
      body: JSON.stringify({
        providerId: "claude",
        sourcePath: "/server/session.jsonl",
      }),
    }),
  );
});

test("interrupted and rejected session streams fail instead of reporting completion", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response('{"type":"header","total":1,"turns":[]}\n')),
  );
  await expect(
    invoke("stream_session_messages", {
      providerId: "claude",
      sourcePath: "/fixture",
      onChunk: new Channel(),
    }),
  ).rejects.toThrow("中断");
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response('{"type":"error","message":"outside session root"}\n'),
    ),
  );
  await expect(
    invoke("stream_session_messages", {
      providerId: "claude",
      sourcePath: "/fixture",
      onChunk: new Channel(),
    }),
  ).rejects.toThrow("outside session root");
});

test("binary images preserve bytes and expired credentials trigger logout", async () => {
  setAccessToken("fixture-token");
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(new Uint8Array([1, 2, 255]))),
  );
  expect(
    new Uint8Array(await invoke<ArrayBuffer>("get_session_image", {})),
  ).toEqual(new Uint8Array([1, 2, 255]));
  const expired = vi.fn();
  window.addEventListener("web-session-expired", expired);
  try {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Response.json({ error: "expired" }, { status: 401 })),
    );
    await expect(invoke("get_session_image", {})).rejects.toMatchObject({
      status: 401,
    });
    expect(expired).toHaveBeenCalledOnce();
  } finally {
    window.removeEventListener("web-session-expired", expired);
  }
});

test("a recorded server path can be displayed without a filesystem request", async () => {
  const show = vi.fn();
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  window.addEventListener("web-show-directory", show, { once: true });
  await invoke("reveal_session_path", { path: "/old-project/main.rs" });
  expect(show).toHaveBeenCalledWith(
    expect.objectContaining({ detail: "/old-project/main.rs" }),
  );
  expect(fetch).not.toHaveBeenCalled();
});

test("Markdown export downloads locally and terminal launch never contacts the server", async () => {
  vi.useFakeTimers();
  const create = vi.fn((_blob: Blob) => "blob:fixture");
  const revoke = vi.fn();
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL = create;
      static revokeObjectURL = revoke;
    },
  );
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => {});
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  try {
    expect(
      await invoke("export_session_markdown", {
        defaultName: "../../session.md",
        content: "# 会话",
      }),
    ).toBe("session.md");
    const blob = create.mock.calls[0][0] as Blob;
    expect(blob.type).toBe("text/markdown;charset=utf-8");
    expect(click).toHaveBeenCalledOnce();
    vi.runOnlyPendingTimers();
    expect(revoke).toHaveBeenCalledWith("blob:fixture");
    await expect(
      invoke("launch_session_terminal", { command: "anything" }),
    ).rejects.toThrow("复制恢复命令");
    expect(fetch).not.toHaveBeenCalled();
  } finally {
    click.mockRestore();
    vi.useRealTimers();
  }
});
