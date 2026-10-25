import { useCallback } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import {
  appsApi,
  errorMessage,
  opsApi,
  type AppView,
  type Target,
  type TrackedApp,
  type UpdateMethod,
} from "@/lib/api";
import { normalizeCheckSummary } from "@/lib/checkSummary";
import { queryKeys } from "@/lib/query/keys";
import { clearRun, markRunStarted } from "@/lib/updateRuns";

/** The tracked apps with their current status. */
export function useApps() {
  return useQuery({
    queryKey: queryKeys.apps,
    queryFn: () => appsApi.list(),
    // The CLI and the scheduler may change things behind our back: the backend
    // pushes `headway://apps-changed` for that, and the window refetches on
    // focus. No periodic whole-list polling is needed on top of those events.
  });
}

/** One app with its current status. */
export function useApp(id: string | undefined) {
  return useQuery({
    queryKey: queryKeys.app(id ?? ""),
    queryFn: () => appsApi.get(id as string),
    enabled: Boolean(id),
  });
}

/** Update history of one app, newest first. */
export function useAppHistory(id: string | undefined) {
  return useQuery({
    queryKey: queryKeys.history(id ?? ""),
    queryFn: () => appsApi.history(id as string),
    enabled: Boolean(id),
  });
}

/** Build presets the backend knows about. */
export function usePresets() {
  return useQuery({
    queryKey: queryKeys.presets,
    queryFn: () => appsApi.presets(),
    staleTime: 5 * 60_000,
  });
}

/** Refresh the app list and the detail of one app. */
function useInvalidateApps() {
  const queryClient = useQueryClient();

  return useCallback(
    async (id?: string) => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.apps });
      if (id) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.app(id) });
        await queryClient.invalidateQueries({
          queryKey: queryKeys.history(id),
        });
        await queryClient.invalidateQueries({
          queryKey: ["source-storage", id],
        });
        await queryClient.invalidateQueries({
          queryKey: ["revision-options", id],
        });
      }
    },
    [queryClient],
  );
}

/** Check one app for new upstream changes. */
export function useCheckApp() {
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: (id: string) => opsApi.check(id),
    onSuccess: (_result, id) => invalidate(id),
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Check every tracked app. */
export function useCheckAll() {
  const { t } = useTranslation();
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: () => opsApi.checkAll(),
    onSuccess: async (summary) => {
      const normalized = normalizeCheckSummary(summary);
      await invalidate();
      toast.success(
        t("toast.checkAllDone", {
          checked: normalized.checked,
          updates: normalized.with_updates,
        }),
      );
      if (normalized.failures.length > 0) {
        // The shared record shape carries the per-app error and its hint.
        const details = normalized.failures
          .slice(0, 3)
          .map((failure) =>
            failure.message ? `${failure.id}: ${failure.message}` : failure.id,
          )
          .join("\n");
        toast.error(
          t("toast.checkAllFailed", { count: normalized.failures.length }),
          details ? { description: details } : undefined,
        );
      }
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/**
 * Start an update in the background.
 *
 * The backend answers immediately and reports progress through
 * `headway://update-event` / `headway://update-finished`.
 */
export function useStartUpdate() {
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: ({
      id,
      target,
      method,
    }: {
      id: string;
      target: Target;
      method?: UpdateMethod;
    }) => opsApi.startUpdate(id, target, method),
    onMutate: ({ id }) => {
      // show the progress block right away, before the first event arrives
      markRunStarted(id);
    },
    onSuccess: (_result, { id }) => invalidate(id),
    onError: (error, { id }) => {
      clearRun(id);
      toast.error(errorMessage(error));
    },
  });
}

/** Cancel a running update. */
export function useCancelUpdate() {
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: (id: string) => opsApi.cancel(id),
    onSuccess: (_result, id) => invalidate(id),
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Mark a commit or tag as "this is what I have installed". */
export function useMarkInstalled() {
  const { t } = useTranslation();
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: ({ id, reference }: { id: string; reference: string }) =>
      opsApi.markInstalled(id, reference),
    onSuccess: async (_result, { id }) => {
      await invalidate(id);
      toast.success(t("toast.markedInstalled"));
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Track a new repository. */
export function useAddApp(options: { silent?: boolean } = {}) {
  const { t } = useTranslation();
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: (app: TrackedApp) => appsApi.add(app),
    onSuccess: async (_result, app) => {
      await invalidate(app.id);
      toast.success(t("toast.appAdded", { name: app.name }));
    },
    onError: (error) => {
      if (!options.silent) toast.error(errorMessage(error));
    },
  });
}

/** Change the configuration of a tracked app. */
export function useSaveApp(
  options: { silent?: boolean; toastSuccess?: boolean } = {},
) {
  const { t } = useTranslation();
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: (app: TrackedApp) => appsApi.save(app),
    onSuccess: async (_result, app) => {
      await invalidate(app.id);
      if (options.toastSuccess !== false) {
        toast.success(t("toast.appSaved", { name: app.name }));
      }
    },
    onError: (error) => {
      if (!options.silent) toast.error(errorMessage(error));
    },
  });
}

/** Stop tracking an app, optionally deleting its cached checkout. */
export function useRemoveApp() {
  const { t } = useTranslation();
  const invalidate = useInvalidateApps();

  return useMutation({
    mutationFn: ({ id, purge }: { id: string; purge: boolean }) =>
      appsApi.remove(id, purge),
    onSuccess: async (_result, { id }) => {
      await invalidate(id);
      toast.success(t("toast.appRemoved"));
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Look up an app in the cached list. */
export function findApp(
  apps: AppView[] | undefined,
  id: string,
): AppView | undefined {
  return apps?.find((entry) => entry.app.id === id);
}
