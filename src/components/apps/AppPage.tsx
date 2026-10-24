import { AllCommits } from "./AllCommits";
import { RevisionDialog } from "./RevisionDialog";
import { RepositorySelector } from "./RepositorySelector";
import { SourceStorageSection } from "./SourceStorageSection";
import { VariantSection } from "./VariantSection";
import { RecoverySection } from "./RecoverySection";
import { AutoUpdatePaused } from "./AutoUpdatePaused";
import { DEFAULT_UPDATE_POLICY } from "@/lib/api";
import { supportsRefs } from "@/lib/sources";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { AppFormDialog } from "@/components/apps/AppFormDialog";
import { AppHeader } from "@/components/apps/AppHeader";
import { AppSummary } from "@/components/apps/AppSummary";
import { ConfigSummary } from "@/components/apps/ConfigSummary";
import { HistoryList } from "@/components/apps/HistoryList";
import { LogPanel } from "@/components/apps/LogPanel";
import { PendingList } from "@/components/apps/PendingList";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { SegmentedControl } from "@/components/ui/segmented-control";
import {
  useApp,
  useAppHistory,
  useApps,
  useCancelUpdate,
  useCheckApp,
  useMarkInstalled,
  useRemoveApp,
  useSaveApp,
  useStartUpdate,
} from "@/hooks/useApps";
import type { AppTab } from "@/lib/navigation";
import { useUpdateRun } from "@/lib/updateRuns";

export interface AppPageProps {
  appId: string;
  tab: AppTab;
  onTabChange: (tab: AppTab) => void;
  onSelectApp?: (id: string) => void;
}

/**
 * The detail page of one tracked app.
 */
export function AppPage({
  appId,
  tab,
  onTabChange,
  onSelectApp,
}: AppPageProps) {
  const { t } = useTranslation();
  const { data: app, isLoading } = useApp(appId);
  const { data: list } = useApps();
  const { data: history, isLoading: historyLoading } = useAppHistory(appId);
  const run = useUpdateRun(appId);

  const checkApp = useCheckApp();
  const startUpdate = useStartUpdate();
  const cancelUpdate = useCancelUpdate();
  const markInstalled = useMarkInstalled();
  const saveApp = useSaveApp();
  const removeApp = useRemoveApp();

  const [editOpen, setEditOpen] = useState(false);
  const [removeOpen, setRemoveOpen] = useState(false);
  const [revisionReference, setRevisionReference] = useState<string | null>(
    null,
  );
  const [pendingVariant, setPendingVariant] = useState<string | null>(null);
  const variantRun = useUpdateRun(pendingVariant ?? "");
  useEffect(() => {
    if (!pendingVariant) return;
    if (list?.some((entry) => entry.app.id === pendingVariant)) {
      setPendingVariant(null);
      onSelectApp?.(pendingVariant);
    } else if (variantRun?.finished) setPendingVariant(null);
  }, [pendingVariant, list, onSelectApp, variantRun?.finished]);

  // the list may know the app before the single-app query answered
  const view = app ?? list?.find((entry) => entry.app.id === appId);

  if (!view) {
    // still loading the first time
    if (isLoading && !list) {
      return (
        <div className="flex flex-1 items-center justify-center text-caption text-fg-3">
          {t("appPage.loading")}
        </div>
      );
    }
    // the app is gone (removed) or was never tracked
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
        <p className="text-body font-medium text-fg-1">
          {t("appPage.notFound")}
        </p>
        <p className="max-w-sm text-caption text-fg-2">
          {t("appPage.notFoundDescription")}
        </p>
      </div>
    );
  }

  const latest = view.last_check?.latest ?? null;
  const installing = startUpdate.isPending || view.busy;

  const handleAutoUpdate = (value: boolean) => {
    saveApp.mutate({
      ...view.app,
      auto_update: value,
      update_policy:
        value && view.app.update_policy?.mode === "notify"
          ? { ...view.app.update_policy, mode: "latest" }
          : (view.app.update_policy ?? DEFAULT_UPDATE_POLICY),
    });
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <AppHeader
        app={view}
        checking={checkApp.isPending}
        onCheck={() => checkApp.mutate(view.app.id)}
        onEdit={() => setEditOpen(true)}
        onMarkLatestInstalled={() =>
          markInstalled.mutate({
            id: view.app.id,
            reference: latest?.id ?? "latest",
          })
        }
        onRemove={() => setRemoveOpen(true)}
      />

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3 sm:p-6">
        {pendingVariant ? (
          <p role="status" className="text-caption text-fg-3">
            {t("versions.waitingRegistration", { id: pendingVariant })}
          </p>
        ) : null}
        <VariantSection key={view.app.id} view={view} />
        {view.auto_update_paused ? (
          <AutoUpdatePaused id={view.app.id} busy={view.busy} />
        ) : null}
        <AppSummary
          app={view}
          run={run}
          starting={installing}
          cancelling={cancelUpdate.isPending}
          onUpdateLatest={() =>
            startUpdate.mutate({ id: view.app.id, target: "latest" })
          }
          onUpdateNext={() =>
            startUpdate.mutate({ id: view.app.id, target: "next" })
          }
          onCancel={() => cancelUpdate.mutate(view.app.id)}
          onToggleAutoUpdate={handleAutoUpdate}
        />

        <div>
          <SegmentedControl
            size="sm"
            aria-label={t("appPage.tabsLabel")}
            value={tab}
            onValueChange={(next) => onTabChange(next as AppTab)}
            items={[
              {
                value: "changes",
                label: t("tabs.changes"),
                trailing:
                  view.pending_count > 0 ? (
                    <span className="rounded-full bg-action-soft px-1.5 text-badge font-semibold text-action-text">
                      {view.pending_count}
                    </span>
                  ) : undefined,
              },
              ...(view.app.source === "github"
                ? [{ value: "commits", label: t("versions.allCommits") }]
                : []),
              { value: "history", label: t("tabs.history") },
              { value: "log", label: t("tabs.log") },
              { value: "config", label: t("tabs.config") },
            ]}
          />
        </div>

        <div className="min-h-0 shrink-0 flex-1">
          {tab === "changes" ? (
            <PendingList
              app={view}
              installing={installing}
              onInstall={(ref) =>
                startUpdate.mutate({
                  id: view.app.id,
                  target: supportsRefs(view.app) ? { ref } : "latest",
                })
              }
              onMarkInstalled={(ref) =>
                markInstalled.mutate({ id: view.app.id, reference: ref })
              }
              onCheck={() => checkApp.mutate(view.app.id)}
            />
          ) : null}

          {tab === "commits" ? (
            <AllCommits
              key={`${view.app.id}:${view.app.source === "github" ? view.app.repo : ""}:${view.app.source === "github" && view.app.track.mode === "commit" ? view.app.track.branch : ""}`}
              view={view}
              onChoose={setRevisionReference}
            />
          ) : null}
          {tab === "history" ? (
            <HistoryList
              canReinstall={supportsRefs(view.app)}
              entries={history}
              loading={historyLoading}
              onReinstall={(ref) =>
                startUpdate.mutate({ id: view.app.id, target: { ref } })
              }
            />
          ) : null}

          {tab === "log" ? <LogPanel appId={view.app.id} /> : null}

          {tab === "config" ? (
            <div className="space-y-4">
              <RepositorySelector
                key={`${view.app.id}:${view.app.source === "github" ? view.app.repo : ""}`}
                view={view}
              />
              <ConfigSummary app={view.app} onEdit={() => setEditOpen(true)} />
              <SourceStorageSection app={view.app} />
              <RecoverySection key={view.app.id} view={view} />
            </div>
          ) : null}
        </div>
      </div>

      <ConfirmDialog
        isOpen={removeOpen}
        title={t("appPage.removeTitle")}
        message={t("appPage.removeMessage", { name: view.app.name })}
        confirmText={t("appPage.remove")}
        variant="destructive"
        checkboxLabel={t("appPage.removePurge")}
        pending={removeApp.isPending}
        onConfirm={(purge) => {
          removeApp.mutate(
            { id: view.app.id, purge },
            { onSuccess: () => setRemoveOpen(false) },
          );
        }}
        onCancel={() => setRemoveOpen(false)}
      />

      {revisionReference !== null ? (
        <RevisionDialog
          key={`${view.app.id}:${view.app.source === "github" ? view.app.repo : ""}:${revisionReference}`}
          view={view}
          reference={revisionReference}
          onClose={() => setRevisionReference(null)}
          onVariant={setPendingVariant}
        />
      ) : null}
      <AppFormDialog
        open={editOpen}
        app={view.app}
        onOpenChange={setEditOpen}
      />
    </div>
  );
}
