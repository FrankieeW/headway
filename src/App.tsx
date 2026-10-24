import { useState } from "react";
import { useTranslation } from "react-i18next";

import { AppFormDialog } from "@/components/apps/AppFormDialog";
import { AppPage } from "@/components/apps/AppPage";
import { EmptyState } from "@/components/apps/EmptyState";
import { SettingsPage } from "@/components/settings/SettingsPage";
import { Sidebar } from "@/components/shell/Sidebar";
import { useAppNavigation } from "@/hooks/useAppNavigation";
import { useApps } from "@/hooks/useApps";
import { useBackendEvents } from "@/hooks/useBackendEvents";

/**
 * The shell: sidebar on the left, the current view on the right.
 */
export default function App() {
  const { t } = useTranslation();
  const { data: apps, dataUpdatedAt } = useApps();
  const navigation = useAppNavigation(apps, dataUpdatedAt);
  const [addOpen, setAddOpen] = useState(false);

  useBackendEvents({
    onSelectApp: navigation.selectApp,
    onShowLog: navigation.showLog,
  });

  const selectedId = navigation.view.kind === "app" ? navigation.view.id : null;

  return (
    <div className="flex h-screen w-full flex-col sm:flex-row overflow-hidden bg-app text-fg-1">
      <Sidebar
        selectedId={selectedId}
        onSelectApp={navigation.selectApp}
        onOpenSettings={navigation.openSettings}
        onAddApp={() => setAddOpen(true)}
      />

      <main className="flex min-h-0 min-w-0 flex-1 flex-col bg-app">
        {navigation.view.kind === "settings" ? <SettingsPage /> : null}

        {navigation.view.kind === "app" ? (
          <AppPage
            appId={navigation.view.id}
            tab={navigation.tab}
            onTabChange={navigation.setTab}
          />
        ) : null}

        {navigation.view.kind === "empty" ? (
          apps === undefined ? (
            <div className="flex flex-1 items-center justify-center text-caption text-fg-3">
              {t("common.loading")}
            </div>
          ) : (
            <EmptyState onAddApp={() => setAddOpen(true)} />
          )
        ) : null}
      </main>

      <AppFormDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        onSaved={(id) => navigation.selectApp(id)}
      />
    </div>
  );
}
