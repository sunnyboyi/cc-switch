import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Settings as SettingsIcon } from "lucide-react";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { GeneralSection } from "@/components/settings/sections/GeneralSection";
import { GlobalProxySettings } from "@/components/settings/GlobalProxySettings";
import { BackupListSection } from "@/components/settings/BackupListSection";
import {
  SettingsBlock,
  SettingsBody,
  SettingsCard,
  SettingsRow,
  SettingsSwitchRow,
} from "@/components/settings/SettingsLayout";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useSettings, type SettingsFormState } from "@/hooks/useSettings";
import { useUnsavedChangesTracker } from "@/lib/unsavedChanges";
import type { SettingsSection } from "@/lib/navigation";
import { toast } from "@/lib/toast";
import { command } from "./runtime/core";
import { errorMessage } from "./App";

const directories = [
  ["claude", "claudeConfigDir", "Claude Code"],
  ["codex", "codexConfigDir", "Codex"],
  ["gemini", "geminiConfigDir", "Gemini CLI"],
] as const;

export default function SettingsPage({
  section,
  onOpenApps,
  onOpenProxySettings,
}: {
  section: SettingsSection;
  onOpenApps: () => void;
  onOpenProxySettings: () => void;
}) {
  const { t } = useTranslation();
  const {
    settings,
    updateSettings,
    autoSaveSettings,
    saveSettings,
    isSaving,
    resolvedDirs,
    updateDirectory,
  } = useSettings();
  const storage = useQuery({
    queryKey: ["web-storage"],
    queryFn: () =>
      command<{
        directory: string;
        database: string;
        schemaVersion: number;
        platform: string;
      }>("web_storage_info"),
  });
  // Only directory drafts need explicit save; switches use the shared autosave hook.
  const [savedRevision, setSavedRevision] = useState(0);
  const dirty = useUnsavedChangesTracker(
    section === "appConfig",
    savedRevision,
  );
  async function save(updates: Partial<SettingsFormState>) {
    if (!settings) return false;
    const previous = settings;
    updateSettings(updates);
    try {
      await autoSaveSettings(updates);
      return true;
    } catch (error) {
      updateSettings(previous);
      toast.error(errorMessage(error));
      return false;
    }
  }
  return (
    <>
      <AppPageHeader
        icon={<SettingsIcon size={22} />}
        title={t(`settings.sections.${section}`)}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <SettingsBody>
          {!settings ? (
            <p role="status">正在读取设置…</p>
          ) : (
            <>
              {section === "general" && (
                <GeneralSection
                  desktopControls={false}
                  settings={settings}
                  onAutoSave={save}
                  onOpenApps={onOpenApps}
                />
              )}
              {section === "appConfig" && (
                <SettingsBlock title="服务器 CLI 配置目录">
                  <p className="text-caption text-fg-3">
                    输入服务器上的绝对路径；留空使用默认目录。保存目录设置不会搬迁已有文件。
                  </p>
                  <div
                    onInputCapture={dirty.onInputCapture}
                    onChangeCapture={dirty.onChangeCapture}
                  >
                    <SettingsCard>
                      {directories.map(([app, field, label]) => (
                        <SettingsRow
                          key={app}
                          label={label}
                          htmlFor={`web-dir-${app}`}
                        >
                          <Input
                            id={`web-dir-${app}`}
                            value={settings[field] || ""}
                            placeholder={resolvedDirs[app]}
                            onChange={(event) =>
                              updateDirectory(app, event.target.value)
                            }
                          />
                        </SettingsRow>
                      ))}
                    </SettingsCard>
                  </div>
                  <Button
                    disabled={isSaving}
                    onClick={() =>
                      void saveSettings()
                        .then((result) => {
                          if (result) setSavedRevision((value) => value + 1);
                        })
                        .catch((error) => toast.error(errorMessage(error)))
                    }
                  >
                    保存目录设置
                  </Button>
                </SettingsBlock>
              )}
              {section === "routing" && (
                <SettingsBlock title="本地路由">
                  <SettingsCard>
                    <SettingsRow
                      label="代理服务、监听地址和请求用量"
                      control={
                        <Button variant="outline" onClick={onOpenProxySettings}>
                          管理本地路由
                        </Button>
                      }
                    />
                  </SettingsCard>
                  <p className="text-caption text-fg-3">
                    按应用切换路由、故障转移和超时参数，请进入对应应用的供应商页面。
                  </p>
                </SettingsBlock>
              )}
              {section === "network" && (
                <SettingsBlock title={t("settings.advanced.globalProxy.title")}>
                  <GlobalProxySettings />
                </SettingsBlock>
              )}
              {section === "data" && (
                <>
                  <SettingsBlock title="Web 数据位置">
                    <SettingsCard>
                      <SettingsRow
                        label="独立数据库"
                        description={storage.data?.database}
                      />
                      <SettingsRow
                        label="设置与管理文件"
                        description={storage.data?.directory}
                      />
                    </SettingsCard>
                    <p className="text-caption text-fg-3">
                      首次复制官方数据后独立保存，之后不会自动同步或覆盖官方库。
                    </p>
                  </SettingsBlock>
                  <SettingsBlock title="备份与恢复">
                    <BackupListSection
                      showStorageOverview={false}
                      backupIntervalHours={settings.backupIntervalHours}
                      backupRetainCount={settings.backupRetainCount}
                      onSettingsChange={(updates) => {
                        void save(updates);
                      }}
                    />
                  </SettingsBlock>
                  <SettingsBlock title="会话用量">
                    <SettingsCard>
                      <SettingsSwitchRow
                        label="自动同步服务器会话用量"
                        checked={settings.sessionAutoSyncEnabled ?? true}
                        onCheckedChange={(enabled) => {
                          void save({ sessionAutoSyncEnabled: enabled });
                        }}
                      />
                    </SettingsCard>
                  </SettingsBlock>
                </>
              )}
              {section === "about" && (
                <SettingsBlock title="CC Switch Web 管理平台">
                  <SettingsCard>
                    <SettingsRow
                      label="服务系统"
                      description={storage.data?.platform}
                    />
                    <SettingsRow
                      label="数据库版本"
                      description={
                        storage.data
                          ? `v${storage.data.schemaVersion}`
                          : undefined
                      }
                    />
                  </SettingsCard>
                  <p className="text-body text-fg-2">
                    复用桌面端管理界面与业务功能。服务程序通过重新部署更新；托盘、窗口和桌面自动更新不在
                    Web 中提供。
                  </p>
                </SettingsBlock>
              )}
            </>
          )}
          {storage.isError && (
            <p role="alert" className="text-danger-text">
              {errorMessage(storage.error)}
            </p>
          )}
        </SettingsBody>
      </div>
    </>
  );
}
