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
import {
  completedOutcomeNote,
  updateNotification,
} from "@/lib/updateNotifications";
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
      const notice = updateNotification(payload, name, t);
      if (notice.tone === "warning") {
        // Integrity evidence was unavailable: never announce this as verified
        // success. The backend warning strings stay unchanged plain text.
        toast.warning(notice.title, {
          description: notice.description,
          action: {
            label: t("toast.showLog"),
            onClick: () => onShowLog(payload.app_id),
          },
        });
      } else {
        toast.success(notice.title);
      }
      return;
    }

    // The user just asked to cancel: an error toast would be noise.
    if (payload.error?.code === "cancelled") {
      return;
    }

    // The last lines of a failed build say more than the exit code. A cycle
    // that failed after some steps still names the last completed outcome.
    const details = payload.error?.details?.slice(-3).join("\n");
    const completed = completedOutcomeNote(payload, t);
    const description = [completed, details].filter(Boolean).join("\n");
    toast.error(errorMessage(payload.error), {
      description: description || undefined,
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
