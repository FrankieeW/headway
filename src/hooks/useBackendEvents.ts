import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { useTauriEvent } from "@/hooks/useTauriEvent";
import {
  EVENTS,
  errorMessage,
  type AppView,
  type UpdateEvent,
  type UpdateFinished,
} from "@/lib/api";
import { isLocalDataQuery, queryKeys } from "@/lib/query/keys";
import { applyUpdateEvent, markRunFinished } from "@/lib/updateRuns";

export interface BackendEventHandlers {
  /** the tray asked to show an app */
  onSelectApp: (appId: string) => void;
  /** a run failed: jump to the log of that app */
  onShowLog: (appId: string) => void;
}

function cachedAppName(queryClient: QueryClient, id: string): string {
  const apps = queryClient.getQueryData<AppView[]>(queryKeys.apps);
  return apps?.find((entry) => entry.app.id === id)?.app.name ?? id;
}

/**
 * The single place where the desktop backend talks to the UI.
 *
 * The CLI and the scheduler change files behind our back, so the backend
 * emits `headway://apps-changed` whenever that happens; the UI has to
 * refresh on its own.
 */
export function useBackendEvents({
  onSelectApp,
  onShowLog,
}: BackendEventHandlers): void {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  // config or state changed (this app, the CLI, the scheduler) — refetch
  useTauriEvent(EVENTS.appsChanged, () => {
    void queryClient.invalidateQueries({ predicate: isLocalDataQuery });
  });

  // live output of a running update
  useTauriEvent<UpdateEvent>(EVENTS.updateEvent, (event) => {
    applyUpdateEvent(event);
  });

  // an update finished
  useTauriEvent<UpdateFinished>(EVENTS.updateFinished, (payload) => {
    markRunFinished(payload);
    void queryClient.invalidateQueries({ predicate: isLocalDataQuery });

    const name = cachedAppName(queryClient, payload.app_id);

    if (payload.ok) {
      const version = payload.outcome?.to.version || payload.outcome?.to.id;
      toast.success(
        version
          ? t("toast.updatedTo", { name, version })
          : t("toast.updated", { name }),
      );
      return;
    }

    toast.error(errorMessage(payload.error), {
      action: {
        label: t("toast.showLog"),
        onClick: () => onShowLog(payload.app_id),
      },
    });
  });

  // the tray asked to show an app
  useTauriEvent<{ app_id: string }>(EVENTS.navigate, (payload) => {
    if (payload?.app_id) {
      onSelectApp(payload.app_id);
    }
  });
}
