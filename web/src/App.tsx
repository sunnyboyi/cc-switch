import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useState,
  type FormEvent,
} from "react";
import {
  QueryClient,
  QueryClientProvider,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { LogOut, Menu, Moon, Route, Server, Sun } from "lucide-react";
import { ThemeProvider, useTheme } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { UpdateProvider } from "@/contexts/UpdateContext";
import { Sidebar } from "@/components/shell/Sidebar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { FrontendErrorBoundary } from "@/components/FrontendErrorBoundary";
import { Toaster } from "@/components/ui/sonner";
import { toast } from "@/lib/toast";
import { copyText } from "@/lib/clipboard";
import type { AppId } from "@/lib/api";
import { settingsApi } from "@/lib/api";
import { DEFAULT_VISIBLE_APPS } from "@/config/appConfig";
import type { SettingsSection, View } from "@/lib/navigation";
import { hasUnsavedChanges, discardUnsavedChanges } from "@/lib/unsavedChanges";
import { request } from "./api";
import { command, connectChanges, setAccessToken } from "./runtime/core";
import { ProvidersPage } from "./ProvidersPage";
import { ProxySettings } from "./ProxySettings";
import { setServerPlatform } from "./runtime/platform";

const McpPanel = lazy(() => import("@/components/mcp/UnifiedMcpPanel"));
const PromptPanel = lazy(() => import("@/components/prompts/PromptPanel"));
const SkillsPanel = lazy(
  () => import("@/components/skills/UnifiedSkillsPanel"),
);
const UsagePage = lazy(() => import("./UsagePage"));
const AppsPage = lazy(() =>
  import("@/components/apps/AppsPage").then((module) => ({
    default: module.AppsPage,
  })),
);
const SettingsPage = lazy(() => import("./SettingsPage"));
const AppsStatus = lazy(() => import("./AppsStatus"));
const apps: AppId[] = ["claude", "codex", "gemini"];
const pages = [
  "mcp",
  "skills",
  "prompts",
  "usage",
  "apps",
  "settings",
] as const;

export function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error || "操作失败");
}

export function App() {
  const [connected, setConnected] = useState(false);
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: { retry: false, refetchOnWindowFocus: true },
        },
      }),
  );
  const logout = useCallback(() => {
    setAccessToken("");
    setConnected(false);
    client.clear();
  }, [client]);
  useEffect(() => {
    window.addEventListener("web-session-expired", logout);
    return () => window.removeEventListener("web-session-expired", logout);
  }, [logout]);
  return (
    <QueryClientProvider client={client}>
      <ThemeProvider storageKey="cc-switch-web-theme">
        <TooltipProvider delayDuration={300}>
          <UpdateProvider enabled={false}>
            <FrontendErrorBoundary>
              {connected ? (
                <Workspace logout={logout} />
              ) : (
                <Login
                  onConnect={(token) => {
                    setAccessToken(token);
                    setConnected(true);
                  }}
                />
              )}
            </FrontendErrorBoundary>
            <Toaster />
          </UpdateProvider>
        </TooltipProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

function Login({ onConnect }: { onConnect: (token: string) => void }) {
  const [token, setToken] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (pending) return;
    setPending(true);
    setError("");
    try {
      const session = await request<{ platform: string }>(token, "/session");
      setServerPlatform(session.platform);
      onConnect(token);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setPending(false);
    }
  }
  return (
    <main className="grid min-h-full place-items-center bg-sidebar px-6 py-12">
      <div className="w-full max-w-sm">
        <div className="mb-8 flex items-center gap-3">
          <Server className="h-8 w-8 text-action" />
          <div>
            <h1 className="text-title font-semibold">CC Switch</h1>
            <p className="text-caption text-fg-3">WEB 管理平台</p>
          </div>
        </div>
        <form
          onSubmit={submit}
          className="rounded-dialog border border-border bg-surface p-6 shadow-v7-sm"
        >
          <h2 className="text-section font-semibold">连接到管理服务</h2>
          <p className="mt-2 text-body text-fg-2">
            管理服务器上的配置、MCP、提示词、Skills 和用量。
          </p>
          <label htmlFor="access-token" className="mb-2 mt-6 block text-body">
            访问令牌
          </label>
          <Input
            id="access-token"
            type="password"
            autoComplete="off"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            required
            disabled={pending}
          />
          {error && (
            <p role="alert" className="mt-3 text-body text-danger-text">
              {error}
            </p>
          )}
          <Button
            className="mt-5 w-full"
            type="submit"
            disabled={pending || !token}
          >
            {pending ? "正在连接…" : "连接管理平台"}
          </Button>
          <p className="mt-4 text-caption text-fg-3">
            令牌仅保存在当前页面内存中，刷新后需重新连接。
          </p>
        </form>
      </div>
    </main>
  );
}

function Workspace({ logout }: { logout: () => void }) {
  const client = useQueryClient();
  const { theme, setTheme } = useTheme();
  const [app, setApp] = useState<AppId>("claude");
  const [promptApp, setPromptApp] = useState<AppId>("claude");
  const [view, setView] = useState<View>("providers");
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>("general");
  const [mobileNav, setMobileNav] = useState(false);
  const [eventsConnected, setEventsConnected] = useState(false);
  const [proxyOpen, setProxyOpen] = useState(false);
  const [toolUpdatesAvailable, setToolUpdatesAvailable] = useState(false);
  const [directory, setDirectory] = useState("");
  const [busy, setBusy] = useState(false);
  const [pendingNavigation, setPendingNavigation] = useState<
    (() => void) | null
  >(null);
  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: () => settingsApi.get(),
  });
  useEffect(() => {
    void import("@/lib/modelsDevAutoSync")
      .then(({ syncModelsDevPricingOnStartup }) =>
        syncModelsDevPricingOnStartup(),
      )
      .then(() => client.invalidateQueries({ queryKey: ["usage"] }))
      .catch((error) => console.warn("[Web] 模型定价自动同步失败", error));
  }, [client]);
  const visibleApps = { ...DEFAULT_VISIBLE_APPS };
  for (const name of Object.keys(visibleApps) as AppId[])
    visibleApps[name] =
      apps.includes(name) && (settings.data?.visibleApps?.[name] ?? true);
  if (!apps.some((name) => visibleApps[name])) visibleApps.claude = true;

  useEffect(
    () =>
      connectChanges(() => {
        void client.invalidateQueries();
      }, setEventsConnected),
    [client],
  );
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") {
        void client.invalidateQueries({
          predicate: (query) =>
            ["providers", "proxy", "usage"].includes(String(query.queryKey[0])),
        });
      }
    }, 15_000);
    return () => clearInterval(timer);
  }, [client]);
  useEffect(() => {
    const show = (event: Event) =>
      setDirectory((event as CustomEvent<string>).detail);
    const refresh = () => {
      void client.invalidateQueries();
    };
    window.addEventListener("web-show-directory", show);
    window.addEventListener("web-refresh", refresh);
    return () => {
      window.removeEventListener("web-show-directory", show);
      window.removeEventListener("web-refresh", refresh);
    };
  }, [client]);

  function navigate(action: () => void) {
    if (busy) {
      toast.warning("操作正在进行，请稍候。");
      return;
    }
    if (hasUnsavedChanges()) {
      setPendingNavigation(() => action);
      return;
    }
    action();
    setMobileNav(false);
  }

  return (
    <div className="web-shell flex overflow-hidden bg-surface text-fg-1">
      <Suspense fallback={null}>
        <AppsStatus onChange={setToolUpdatesAvailable} />
      </Suspense>
      <aside className={`web-navigation ${mobileNav ? "is-open" : ""}`}>
        <Sidebar
          activeApp={app}
          view={view}
          visibleApps={visibleApps}
          settingsSection={settingsSection}
          availablePages={pages}
          appsUpdateAvailable={
            settings.data?.checkToolUpdatesOnStartup && toolUpdatesAvailable
          }
          onSelectApp={(next) =>
            navigate(() => {
              setApp(next);
              setView("providers");
            })
          }
          onSelectPage={(next) => navigate(() => setView(next))}
          onSelectSettingsSection={(section) =>
            navigate(() => setSettingsSection(section))
          }
          onExitSettings={() => navigate(() => setView("providers"))}
        />
      </aside>
      {mobileNav && (
        <button
          aria-label="关闭导航"
          className="web-nav-backdrop"
          onClick={() => setMobileNav(false)}
        />
      )}
      <main id="main-content" className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3 text-caption text-fg-3">
          <Button
            variant="quiet"
            size="icon"
            aria-label="展开导航"
            className="web-menu"
            onClick={() => setMobileNav(!mobileNav)}
          >
            <Menu size={16} />
          </Button>
          <span
            className={`h-1.5 w-1.5 shrink-0 rounded-full ${eventsConnected ? "bg-success" : "bg-warning"}`}
          />
          <span className="min-w-0 truncate">
            {eventsConnected ? "已连接服务器" : "正在重新连接服务器…"}
          </span>
          <span className="ms-auto flex shrink-0 items-center gap-1">
            <Button
              size="compact"
              variant="quiet"
              onClick={() => setProxyOpen(true)}
            >
              <Route size={14} />
              本地路由
            </Button>
            <Button
              variant="quiet"
              size="icon"
              aria-label="切换主题"
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            >
              {theme === "dark" ? <Sun size={15} /> : <Moon size={15} />}
            </Button>
            <Button
              variant="quiet"
              size="icon"
              aria-label="退出连接"
              onClick={() => navigate(logout)}
            >
              <LogOut size={15} />
            </Button>
          </span>
        </div>
        {settings.isError && (
          <p role="alert" className="m-4 text-body text-danger-text">
            {errorMessage(settings.error)}{" "}
            <Button
              variant="outline"
              size="compact"
              onClick={() => void settings.refetch()}
            >
              重试
            </Button>
          </p>
        )}
        <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
          <Suspense
            fallback={
              <div role="status" className="p-6 text-body text-fg-3">
                正在加载页面…
              </div>
            }
          >
            {view === "providers" && (
              <ProvidersPage
                key={app}
                app={app as "claude" | "codex" | "gemini"}
                onOpenProxySettings={() => setProxyOpen(true)}
              />
            )}
            {view === "mcp" && (
              <McpPanel onInteractionBlockedChange={setBusy} />
            )}
            {view === "skills" && (
              <SkillsPanel onNavigationBlockedChange={setBusy} />
            )}
            {view === "prompts" && (
              <PromptPanel
                appId={promptApp}
                apps={apps}
                onAppChange={setPromptApp}
                onNavigationBlockedChange={setBusy}
              />
            )}
            {view === "usage" && (
              <UsagePage onOpenProxySettings={() => setProxyOpen(true)} />
            )}
            {view === "apps" && <AppsPage availableApps={apps} />}
            {view === "settings" && (
              <SettingsPage
                key={settingsSection}
                section={settingsSection}
                onOpenApps={() => navigate(() => setView("apps"))}
                onOpenProxySettings={() => setProxyOpen(true)}
              />
            )}
          </Suspense>
        </div>
      </main>
      <ProxySettings open={proxyOpen} onOpenChange={setProxyOpen} />
      <Dialog
        open={!!directory}
        onOpenChange={(open) => {
          if (!open) setDirectory("");
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>服务器目录</DialogTitle>
            <DialogDescription>
              此路径位于运行管理服务的服务器上。
            </DialogDescription>
          </DialogHeader>
          <div className="px-6 pb-6">
            <Input aria-label="服务器目录路径" value={directory} readOnly />
            <Button
              className="mt-3"
              variant="outline"
              onClick={() =>
                void copyText(directory)
                  .then(() => toast.success("路径已复制"))
                  .catch(() => toast.error("请选中路径手动复制"))
              }
            >
              复制路径
            </Button>
          </div>
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        isOpen={pendingNavigation !== null}
        title="离开编辑页面？"
        message="尚未保存的修改将被丢弃。"
        confirmText="丢弃并离开"
        onCancel={() => setPendingNavigation(null)}
        onConfirm={() => {
          discardUnsavedChanges();
          pendingNavigation?.();
          setPendingNavigation(null);
          setMobileNav(false);
        }}
      />
    </div>
  );
}

export function usePageSettings() {
  const client = useQueryClient();
  const { data } = useQuery({
    queryKey: ["settings"],
    queryFn: () => settingsApi.get(),
  });
  async function save(updates: Record<string, unknown>) {
    if (!data) return false;
    try {
      await command("save_settings", { settings: { ...data, ...updates } });
      await client.invalidateQueries({ queryKey: ["settings"] });
      return true;
    } catch (error) {
      toast.error(errorMessage(error));
      return false;
    }
  }
  return { settings: data, save };
}
