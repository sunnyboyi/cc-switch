import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { command, getAccessToken, setAccessToken } from "./runtime/core";

const token = "test-only-token-kept-in-memory-123456";
let rejectAuth = false;
let calls: string[];
let savedSettings: Record<string, unknown> | undefined;

beforeEach(() => {
  rejectAuth = false;
  calls = [];
  savedSettings = undefined;
  localStorage.clear();
  setAccessToken("");
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const headers = init?.headers as Record<string, string>;
      expect(headers.Authorization).toBe(`Bearer ${token}`);
      if (rejectAuth)
        return Response.json(
          { error: "Access token required" },
          { status: 401 },
        );
      if (url === "/api/session")
        return Response.json({ authenticated: true, platform: "linux" });
      if (url === "/api/events") {
        return new Response(
          new ReadableStream({
            start(controller) {
              controller.enqueue(
                new TextEncoder().encode('data: {"command":"connected"}\n\n'),
              );
              init?.signal?.addEventListener(
                "abort",
                () => controller.close(),
                { once: true },
              );
            },
          }),
          { headers: { "Content-Type": "text/event-stream" } },
        );
      }
      const name = url.split("/").pop()!;
      calls.push(name);
      const args = JSON.parse(String(init?.body)).args;
      if (name === "save_settings") {
        savedSettings = args.settings;
        return Response.json(true);
      }
      const data: Record<string, unknown> = {
        get_settings: savedSettings ?? {
          enableClaudePluginIntegration: false,
          launchOnStartup: false,
          language: "zh",
          visibleApps: {
            claude: true,
            codex: true,
            gemini: true,
            "claude-desktop": false,
            grokbuild: false,
            opencode: false,
            openclaw: false,
            hermes: false,
            pi: false,
            kimi: false,
            dsh: false,
            mcode: false,
            agy: false,
            omp: false,
          },
          sessionAutoSyncEnabled: false,
        },
        get_providers: {},
        get_current_provider: "",
        get_app_mode: {
          mode: "direct",
          attached: false,
          routeProviderId: null,
          directProviderId: null,
        },
        get_proxy_status: {
          running: false,
          address: "127.0.0.1",
          port: 15721,
          active_connections: 0,
        },
        get_auto_failover_enabled: false,
        get_failover_queue: [],
        get_proxy_stack: { members: [], notices: [] },
        get_usage_summary: {
          totalCost: "0",
          totalRequests: 0,
          realTotalTokens: 0,
          cacheHitRate: 0,
        },
        list_profiles: {
          profiles: [],
          currentIds: { claude: null, claudeDesktop: null, codex: null },
        },
        get_mcp_servers: {},
        get_prompts: {},
        get_current_prompt_file_content: null,
        get_prompt_file_location: {
          path: `/server/${args.app}/CLAUDE.md`,
          displayPath: "~/CLAUDE.md",
        },
        get_installed_skills: [],
        get_models_dev_sync_config: {
          config: { autoSyncEnabled: false, lastSyncAt: null },
        },
        scan_unmanaged_skills: [],
        get_skill_backups: [],
        web_home_dir: "/server",
        web_join_path: "/server/.cc-switch",
        is_portable_mode: false,
        get_app_config_dir_override: null,
        get_config_dir: "/server/.cc-switch",
        web_storage_info: {
          directory: "/server/.cc-switch-web",
          database: "/server/.cc-switch-web/cc-switch-web.db",
          schemaVersion: 24,
          platform: "linux",
        },
        get_tool_versions: (args.tools || []).map((name: string) => ({
          name,
          version: null,
          latest_version: null,
          error: null,
          installed_but_broken: false,
          env_type: "linux",
          wsl_distro: null,
          update_blocked_by_node: null,
        })),
        list_tool_installations: [],
        get_claude_desktop_status: { installed: false },
        list_db_backups: [],
      };
      if (!(name in data)) throw new Error(`Unexpected API request: ${name}`);
      return Response.json(data[name]);
    }),
  );
});

afterEach(() => {
  cleanup();
  setAccessToken("");
  vi.unstubAllGlobals();
});

async function connect() {
  const user = userEvent.setup();
  render(<App />);
  await user.type(screen.getByLabelText("访问令牌"), token);
  await user.click(screen.getByRole("button", { name: "连接管理平台" }));
  await screen.findByRole("heading", { name: "Claude Code" });
  return user;
}

test("authentication stays in memory, shared desktop navigation mounts, and logout clears the session", async () => {
  const user = await connect();
  expect(getAccessToken()).toBe(token);
  expect(Object.values(localStorage).join(" ")).not.toContain(token);
  await user.click(screen.getByRole("button", { name: "MCP", exact: true }));
  await screen.findByRole("heading", { name: /MCP/ });
  await waitFor(() => expect(calls).toContain("get_mcp_servers"));
  await user.click(screen.getByRole("button", { name: "Skills", exact: true }));
  await screen.findByRole("heading", { name: "Skills", exact: true });
  await waitFor(() => expect(calls).toContain("get_installed_skills"));
  await user.click(screen.getByRole("button", { name: "提示词", exact: true }));
  await screen.findByRole("heading", { name: "提示词", exact: true });
  await waitFor(() => expect(calls).toContain("get_prompts"));
  await user.click(screen.getByRole("button", { name: "退出连接" }));
  expect(await screen.findByLabelText("访问令牌")).toHaveValue("");
  expect(getAccessToken()).toBe("");
});

test("a rejected token stays on login and exposes the error", async () => {
  rejectAuth = true;
  const user = userEvent.setup();
  render(<App />);
  await user.type(screen.getByLabelText("访问令牌"), token);
  await user.click(screen.getByRole("button", { name: "连接管理平台" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Access token required",
  );
  expect(getAccessToken()).toBe("");
  expect(calls).toEqual([]);
});

test("an expired session closes shared pages and clears credentials", async () => {
  await connect();
  rejectAuth = true;
  await act(async () => {
    await command("get_settings").catch(() => {});
  });
  expect(await screen.findByLabelText("访问令牌")).toHaveValue("");
  expect(getAccessToken()).toBe("");
});

test("apps and settings reuse navigation, persist web preferences, and show the independent database", async () => {
  const user = await connect();
  await user.click(screen.getByRole("button", { name: "应用", exact: true }));
  await screen.findByRole("heading", { name: "应用", exact: true });
  await waitFor(() => expect(calls).toContain("get_tool_versions"));
  await user.click(
    await screen.findByRole("switch", { name: /在侧栏.*Gemini|Gemini.*侧栏/ }),
  );
  await waitFor(() =>
    expect(savedSettings?.visibleApps).toMatchObject({ gemini: false }),
  );
  await user.click(screen.getByRole("button", { name: "设置", exact: true }));
  await screen.findByRole("heading", { name: "通用", exact: true });
  expect(screen.queryByText("开机自启")).not.toBeInTheDocument();
  await user.click(
    await screen.findByRole("switch", { name: "显示供应商搜索" }),
  );
  await waitFor(() => expect(savedSettings?.showProviderSearch).toBe(false));
  await user.click(screen.getByRole("button", { name: "数据", exact: true }));
  expect(
    await screen.findByText("/server/.cc-switch-web/cc-switch-web.db"),
  ).toBeInTheDocument();
  expect(calls).toContain("list_db_backups");
  await user.click(
    screen.getByRole("button", { name: "应用配置", exact: true }),
  );
  await user.type(
    await screen.findByRole("textbox", { name: "Claude Code" }),
    "/srv/claude",
  );
  await user.click(screen.getByRole("button", { name: "返回", exact: true }));
  expect(await screen.findByText("离开编辑页面？")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "取消", exact: true }));
  expect(screen.getByRole("textbox", { name: "Claude Code" })).toHaveValue(
    "/srv/claude",
  );
});
