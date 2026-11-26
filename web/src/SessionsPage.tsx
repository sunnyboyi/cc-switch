import type { AppId } from "@/lib/api";
import { SessionManagerPage } from "@/components/sessions/SessionManagerPage";
import { usePageSettings } from "./App";

export default function SessionsPage({ app }: { app: AppId }) {
  const { save } = usePageSettings();
  return (
    <SessionManagerPage
      appId={app}
      allowTerminal={false}
      onContentSearchChange={(enabled) =>
        save({ sessionContentSearchEnabled: enabled })
      }
    />
  );
}
