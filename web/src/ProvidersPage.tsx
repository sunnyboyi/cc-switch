import {
  lazy,
  Suspense,
  useCallback,
  useState,
  type ComponentProps,
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Search } from "lucide-react";
import { SwitchModePanel } from "@/components/providers/mode/SwitchModePanel";
import { ProfileSwitcher } from "@/components/profiles/ProfileSwitcher";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { AppGlyph, APP_DISPLAY_NAME } from "@/components/shell/AppGlyph";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { toast } from "@/lib/toast";
import type { Provider } from "@/types";
import type { AppMode, AppModeView } from "@/types/proxy";
import { generateUUID } from "@/utils/uuid";
import { command, invoke } from "./runtime/core";
import { errorMessage, usePageSettings } from "./App";

type App = "claude" | "codex" | "gemini";
type WebProvider = Provider & { _webRevision: string };
const AddProviderDialog = lazy(() =>
  import("@/components/providers/AddProviderDialog").then((module) => ({
    default: module.AddProviderDialog,
  })),
);
const EditProviderDialog = lazy(() =>
  import("@/components/providers/EditProviderDialog").then((module) => ({
    default: module.EditProviderDialog,
  })),
);

export function ProvidersPage({
  app,
  onOpenProxySettings,
}: {
  app: App;
  onOpenProxySettings: () => void;
}) {
  const client = useQueryClient();
  const { settings } = usePageSettings();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<WebProvider | null>(null);
  const [deleting, setDeleting] = useState<WebProvider | null>(null);
  const [search, setSearch] = useState(false);
  const [formMode, setFormMode] = useState<AppMode>("direct");
  const providers = useQuery({
    queryKey: ["providers", app],
    queryFn: async () => {
      const [providers, currentProviderId] = await Promise.all([
        command<Record<string, WebProvider>>("get_providers", { app }),
        command<string>("get_current_provider", { app }),
      ]);
      return { providers, currentProviderId };
    },
  });
  const mode = useQuery({
    queryKey: ["providers", app, "mode"],
    queryFn: () => command<AppModeView>("get_app_mode", { appType: app }),
  });
  const onViewChange = useCallback(
    (_app: unknown, view: AppMode) => setFormMode(view),
    [],
  );

  async function refresh() {
    await client.invalidateQueries({ queryKey: ["providers", app] });
  }
  async function mutation<T>(work: () => Promise<T>): Promise<T> {
    try {
      const result = await work();
      await refresh();
      return result;
    } catch (error) {
      // The shared editor owns its native-file conflict dialog; keep its structured error intact.
      if (!errorMessage(error).includes("LIVE_EDIT_CONFLICT"))
        toast.error(errorMessage(error));
      throw error;
    }
  }
  const add: ComponentProps<typeof AddProviderDialog>["onSubmit"] = async (
    input,
  ) => {
    const { editorSave, ...provider } = input;
    await mutation(() =>
      command("add_provider", {
        app,
        provider: { ...provider, id: generateUUID() },
        editorSave,
      }),
    );
    toast.success("供应商已添加");
  };
  const update: ComponentProps<typeof EditProviderDialog>["onSubmit"] = async ({
    provider,
    originalId,
    editorSave,
  }) => {
    await mutation(() =>
      command(
        "update_provider",
        { app, provider, originalId, editorSave },
        editing?._webRevision,
      ),
    );
    toast.success("供应商已保存");
  };
  const remove = useMutation({
    mutationFn: async () => {
      if (!deleting) return;
      await mutation(() =>
        command(
          "delete_provider",
          { app, id: deleting.id },
          deleting._webRevision,
        ),
      );
    },
    onSuccess: () => {
      setDeleting(null);
      toast.success("供应商已删除");
    },
  });
  function onSwitch(provider: Provider) {
    return mutation(() =>
      command<{ warnings: string[] }>("switch_provider", {
        app,
        id: provider.id,
      }),
    )
      .then((result) => {
        if (result.warnings.length) toast.warning(result.warnings.join("\n"));
        else toast.success("供应商已切换");
      })
      .catch(() => {});
  }
  function duplicate(provider: Provider) {
    void mutation(() =>
      command("add_provider", {
        app,
        addToLive: false,
        provider: {
          ...provider,
          id: generateUUID(),
          name: `${provider.name}（副本）`,
          sortIndex: undefined,
        },
      }),
    )
      .then(() => toast.success("供应商已复制"))
      .catch(() => {});
  }

  return (
    <>
      <AppPageHeader
        variant="app"
        icon={<AppGlyph app={app} size={22} />}
        title={APP_DISPLAY_NAME[app]}
        subtitle="供应商"
        titleExtra={<ProfileSwitcher activeApp={app} />}
        actions={
          <>
            {(settings?.showProviderSearch ?? true) && (
              <Button
                variant="quiet"
                size="icon"
                aria-label="搜索供应商"
                onClick={() => setSearch(!search)}
              >
                <Search size={16} />
              </Button>
            )}
            <Button size="compact" onClick={() => setAdding(true)}>
              <Plus size={15} />
              添加供应商
            </Button>
          </>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        {providers.isError ? (
          <div role="alert" className="p-6 text-body text-danger-text">
            {errorMessage(providers.error)}
            <Button
              className="ms-3"
              variant="outline"
              onClick={() => void providers.refetch()}
            >
              重试
            </Button>
          </div>
        ) : mode.data?.mode === "stack" ? (
          <div role="status" className="p-6 text-body">
            服务器当前处于聚合模式。本阶段 Web
            入口仅开放直连和基础路由，请先恢复直连。
            <Button
              className="ms-3"
              variant="outline"
              onClick={onOpenProxySettings}
            >
              打开本地路由
            </Button>
          </div>
        ) : (
          <SwitchModePanel
            allowStack={false}
            app={app}
            providers={providers.data?.providers ?? {}}
            currentProviderId={providers.data?.currentProviderId ?? ""}
            isLoading={providers.isPending}
            onSwitch={onSwitch}
            onEdit={(provider) => setEditing(provider as WebProvider)}
            onDelete={(provider) => setDeleting(provider as WebProvider)}
            onDuplicate={duplicate}
            onCreate={() => setAdding(true)}
            onOpenWebsite={(url) =>
              void invoke("open_external", { url }).catch((error) =>
                toast.error(errorMessage(error)),
              )
            }
            searchOpen={search}
            onSearchOpenChange={setSearch}
            onOpenRoutingSettings={onOpenProxySettings}
            onViewChange={onViewChange}
          />
        )}
      </div>
      <Suspense
        fallback={
          <p role="status" className="px-6 py-2 text-body text-fg-3">
            正在加载供应商表单…
          </p>
        }
      >
        {adding && (
          <AddProviderDialog
            appId={app}
            open={adding}
            onOpenChange={setAdding}
            onSubmit={add}
            modeView={formMode}
          />
        )}
        {editing && (
          <EditProviderDialog
            appId={app}
            provider={editing}
            open={!!editing}
            onOpenChange={(open) => {
              if (!open) setEditing(null);
            }}
            onSubmit={update}
            isCurrent={editing.id === providers.data?.currentProviderId}
            isProxyTakeover={
              mode.data?.mode !== undefined && mode.data.mode !== "direct"
            }
            modeView={formMode}
          />
        )}
      </Suspense>
      <ConfirmDialog
        isOpen={!!deleting}
        title="删除供应商"
        message={`确定删除「${deleting?.name ?? ""}」？`}
        pending={remove.isPending}
        onCancel={() => setDeleting(null)}
        onConfirm={() => remove.mutate()}
      />
    </>
  );
}
