import { useEffect, useRef } from "react";
import {
  checkToolUpdatesInBackground,
  useToolUpdatesAvailable,
} from "@/components/apps/useToolManagement";
import { useSettingsQuery } from "@/lib/query";

export default function AppsStatus({
  onChange,
}: {
  onChange: (available: boolean) => void;
}) {
  const { data: settings } = useSettingsQuery();
  const available = useToolUpdatesAvailable();
  const checked = useRef(false);
  useEffect(() => {
    onChange(available);
  }, [available, onChange]);
  useEffect(() => {
    if (!settings?.checkToolUpdatesOnStartup || checked.current) return;
    checked.current = true;
    void checkToolUpdatesInBackground();
  }, [settings?.checkToolUpdatesOnStartup]);
  return null;
}
