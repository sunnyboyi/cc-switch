import { afterEach, expect, test, vi } from "vitest";
import {
  Channel,
  command,
  connectChanges,
  invoke,
  listen,
  setAccessToken,
} from "./core";

afterEach(() => {
  setAccessToken("");
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

test("provider edits carry their revision and preserve structured native-file conflicts", async () => {
  const conflict = JSON.stringify({
    code: "LIVE_EDIT_CONFLICT",
    paths: ["env.MODEL"],
  });
  const fetch = vi.fn(async () =>
    Response.json({ error: conflict }, { status: 409 }),
  );
  vi.stubGlobal("fetch", fetch);
  setAccessToken("fixture-token");
  await expect(
    command("update_provider", { app: "claude" }, "revision-a"),
  ).rejects.toMatchObject({ status: 409, message: conflict });
  expect(fetch).toHaveBeenCalledWith(
    "/api/command/update_provider",
    expect.objectContaining({
      headers: expect.objectContaining({
        Authorization: "Bearer fixture-token",
      }),
      body: JSON.stringify({
        args: { app: "claude" },
        expected_revision: "revision-a",
      }),
      credentials: "omit",
      cache: "no-store",
    }),
  );
});

test("fragmented SSE events refresh state, notify profiles, and reconnect without leaking the token", async () => {
  vi.useFakeTimers();
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const fetch = vi.fn(
    async () =>
      new Response(
        new ReadableStream<Uint8Array>({
          start(controller) {
            stream = controller;
          },
        }),
      ),
  );
  vi.stubGlobal("fetch", fetch);
  setAccessToken("fixture-secret");
  const change = vi.fn();
  const status = vi.fn();
  const profile = vi.fn();
  const unlisten = await listen("profile-applied", profile);
  const stop = connectChanges(change, status);
  await vi.waitFor(() => expect(status).toHaveBeenCalledWith(true));
  const encode = (value: string) => new TextEncoder().encode(value);
  stream.enqueue(
    encode(
      'data: {"command":"connected"}\r\n\r\n: keepalive\n\ndata: {"command":"apply_',
    ),
  );
  stream.enqueue(encode('profile","scope":"claude"}\n\n'));
  await vi.waitFor(() => expect(change).toHaveBeenCalledTimes(2));
  expect(profile).toHaveBeenCalledWith(
    expect.objectContaining({ payload: { scope: "claude" } }),
  );
  expect(fetch.mock.calls[0]).toEqual([
    "/api/events",
    expect.objectContaining({
      headers: { Authorization: "Bearer fixture-secret" },
    }),
  ]);
  stream.close();
  await vi.waitFor(() => expect(status).toHaveBeenCalledWith(false));
  await vi.advanceTimersByTimeAsync(2000);
  expect(fetch).toHaveBeenCalledTimes(2);
  stop();
  stream.close();
  unlisten();
  await vi.advanceTimersByTimeAsync(6000);
  expect(fetch).toHaveBeenCalledTimes(2);
});

test("ZIP upload requires a browser-selected file and external links reject executable protocols", async () => {
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);
  await expect(
    invoke("install_skills_from_zip", {
      filePath: "/etc/passwd",
      currentApp: "claude",
    }),
  ).rejects.toThrow("请重新选择");
  await expect(
    invoke("open_external", { url: "javascript:alert(1)" }),
  ).rejects.toThrow("HTTP");
  expect(fetch).not.toHaveBeenCalled();
});

test("MCP channels receive authenticated results and stop after cancellation", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (url === "/api/events")
        return new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              stream = controller;
            },
          }),
        );
      return Response.json(
        url.endsWith("start_mcp_connectivity_checks") ? "fixture-batch" : null,
      );
    }),
  );
  setAccessToken("fixture-secret");
  const status = vi.fn();
  const stop = connectChanges(vi.fn(), status);
  await vi.waitFor(() => expect(status).toHaveBeenCalledWith(true));
  const channel = new Channel<{ serverId: string; status: string }>();
  channel.onmessage = vi.fn();
  await invoke("start_mcp_connectivity_checks", {
    serverIds: ["fixture", "pending"],
    onResult: channel,
  });
  const result = { serverId: "fixture", status: "failed" };
  const frame = new TextEncoder().encode(
    `data: ${JSON.stringify({ command: "mcp-connectivity-result", channelId: channel.id, payload: result })}\n\n`,
  );
  stream.enqueue(frame);
  await vi.waitFor(() =>
    expect(channel.onmessage).toHaveBeenCalledWith(result),
  );
  await invoke("cancel_mcp_connectivity_checks", { batchId: "fixture-batch" });
  stream.enqueue(frame);
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(channel.onmessage).toHaveBeenCalledTimes(1);
  stop();
  stream.close();
});
