import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { proxyApi } from "@/lib/api/proxy";
import type { GlobalProxyConfig } from "@/types/proxy";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/lib/toast";
import { errorMessage } from "./App";

export function ProxySettings({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const client = useQueryClient();
  const config = useQuery({
    queryKey: ["proxy", "global"],
    queryFn: () => proxyApi.getGlobalProxyConfig(),
    enabled: open,
  });
  const status = useQuery({
    queryKey: ["proxy", "status"],
    queryFn: () => proxyApi.getProxyStatus(),
    enabled: open,
    refetchInterval: open ? 5000 : false,
  });
  const [draft, setDraft] = useState<GlobalProxyConfig | null>(null);
  useEffect(() => {
    if (!open) setDraft(null);
  }, [open]);
  useEffect(() => {
    if (open && !draft && config.data) setDraft(config.data);
  }, [open, draft, config.data]);
  const action = useMutation({
    mutationFn: async (kind: "save" | "start" | "stop") => {
      if (kind === "start") await proxyApi.startProxyServer();
      else if (kind === "stop") await proxyApi.stopProxyWithRestore();
      else if (draft) {
        if (
          !draft.listenAddress.trim() ||
          !Number.isInteger(draft.listenPort) ||
          draft.listenPort < 1 ||
          draft.listenPort > 65535
        )
          throw new Error("请填写监听地址及 1～65535 的端口");
        await proxyApi.updateGlobalProxyConfig(draft);
      }
    },
    onSuccess: async () => {
      await client.invalidateQueries();
      toast.success("本地路由设置已更新");
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => {
        if (!action.isPending) onOpenChange(value);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>本地路由</DialogTitle>
          <DialogDescription>
            路由服务运行在管理服务所在机器上。进入应用页的路由模式后，才会接管对应
            CLI 配置。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 px-6 py-3 text-body">
          {(config.isError || status.isError) && (
            <p role="alert" className="text-danger-text">
              {errorMessage(config.error || status.error)}{" "}
              <Button
                variant="outline"
                size="compact"
                onClick={() => {
                  void config.refetch();
                  void status.refetch();
                }}
              >
                重试
              </Button>
            </p>
          )}
          <p>
            {status.data
              ? status.data.running
                ? `运行中 · ${status.data.address}:${status.data.port}`
                : "服务已停止"
              : "正在读取状态…"}
          </p>
          {draft && (
            <>
              <label className="block space-y-2">
                <span>监听地址</span>
                <Input
                  value={draft.listenAddress}
                  disabled={action.isPending}
                  onChange={(event) =>
                    setDraft({ ...draft, listenAddress: event.target.value })
                  }
                />
              </label>
              <label className="block space-y-2">
                <span>监听端口</span>
                <Input
                  type="number"
                  min={1}
                  max={65535}
                  value={draft.listenPort}
                  disabled={action.isPending}
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      listenPort: Number(event.target.value),
                    })
                  }
                />
              </label>
              <label className="flex items-center justify-between">
                <span>记录请求用量</span>
                <Switch
                  checked={draft.enableLogging}
                  disabled={action.isPending}
                  onCheckedChange={(checked) =>
                    setDraft({ ...draft, enableLogging: checked })
                  }
                />
              </label>
              <p className="text-caption text-fg-3">
                服务运行时修改监听地址或端口，保存会重启路由服务并重写已接管的客户端配置。
              </p>
            </>
          )}
        </div>
        <DialogFooter>
          <Button
            variant="outline"
            disabled={!status.data || action.isPending}
            onClick={() =>
              action.mutate(status.data?.running ? "stop" : "start")
            }
          >
            {status.data?.running ? "恢复直连并停止" : "启动服务"}
          </Button>
          <Button
            disabled={!draft || action.isPending}
            onClick={() => action.mutate("save")}
          >
            保存设置
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
