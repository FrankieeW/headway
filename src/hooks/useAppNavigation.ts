import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { AppView } from "@/lib/api";
import {
  readStoredAppId,
  storeAppId,
  type AppTab,
  type View,
} from "@/lib/navigation";

export interface AppNavigation {
  view: View;
  tab: AppTab;
  selectApp: (id: string) => void;
  openSettings: () => void;
  showLog: (id: string) => void;
  setTab: (tab: AppTab) => void;
}

/**
 * Which app is selected and which tab of it is open.
 *
 * The selected id is remembered in local storage so the app reopens on the
 * same entry; when it is gone the first app of the list is used.
 *
 * @param apps the tracked apps
 * @param appsUpdatedAt when the list was last fetched — a list that is
 *   older than the current selection may simply not know the app yet
 */
export function useAppNavigation(
  apps: AppView[] | undefined,
  appsUpdatedAt: number,
): AppNavigation {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<AppTab>("changes");
  const [showSettings, setShowSettings] = useState(false);

  /** when the current selection was made */
  const selectedAt = useRef(0);

  const selectApp = useCallback((id: string) => {
    selectedAt.current = Date.now();
    setSelectedId(id);
    setTab("changes");
    setShowSettings(false);
    storeAppId(id);
  }, []);

  const openSettings = useCallback(() => {
    setShowSettings(true);
  }, []);

  const showLog = useCallback((id: string) => {
    selectedAt.current = Date.now();
    setSelectedId(id);
    setTab("log");
    setShowSettings(false);
    storeAppId(id);
  }, []);

  // fall back to the remembered id, then to the first app
  useEffect(() => {
    if (!apps || apps.length === 0) return;
    if (selectedId && apps.some((entry) => entry.app.id === selectedId)) return;

    // The selected app is not in the list. If the list was fetched before
    // the selection, it may simply not know the app yet — keep the
    // selection and wait for the next fetch.
    if (selectedId && appsUpdatedAt < selectedAt.current) return;

    const stored = readStoredAppId();
    const fallback =
      stored && apps.some((entry) => entry.app.id === stored)
        ? stored
        : apps[0].app.id;

    setSelectedId(fallback);
    storeAppId(fallback);
  }, [apps, appsUpdatedAt, selectedId]);

  const view = useMemo<View>(() => {
    if (showSettings) return { kind: "settings" };
    if (selectedId) return { kind: "app", id: selectedId };
    return { kind: "empty" };
  }, [showSettings, selectedId]);

  return { view, tab, selectApp, openSettings, showLog, setTab };
}
