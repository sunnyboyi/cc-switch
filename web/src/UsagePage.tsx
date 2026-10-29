import { UsageDashboard } from "@/components/usage/UsageDashboard";
import { usePageSettings } from "./App";

export default function UsagePage({
  onOpenProxySettings,
}: {
  onOpenProxySettings: () => void;
}) {
  const { settings, save } = usePageSettings();
  return (
    <UsageDashboard
      settingsReady={!!settings}
      refreshIntervalMs={settings?.usageDashboardRefreshIntervalMs}
      sessionAutoSyncEnabled={settings?.sessionAutoSyncEnabled}
      usageDisplay={settings?.usageDisplay}
      onRefreshIntervalChange={(value) =>
        save({ usageDashboardRefreshIntervalMs: value })
      }
      onSessionAutoSyncEnabledChange={(value) =>
        save({ sessionAutoSyncEnabled: value })
      }
      onUsageDisplayChange={(value) => save({ usageDisplay: value })}
      onOpenRoutingSettings={onOpenProxySettings}
    />
  );
}
