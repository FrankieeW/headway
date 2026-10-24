/** The tabs of the app detail page. */
export type AppTab = "changes" | "commits" | "history" | "log" | "config";

/** What the content area shows. */
export type View =
  { kind: "app"; id: string } | { kind: "settings" } | { kind: "empty" };

/** Where the selected app id is remembered between launches. */
export const SELECTED_APP_STORAGE_KEY = "headway-selected-app";

/** Read the remembered app id, if any. */
export function readStoredAppId(): string | null {
  try {
    return window.localStorage.getItem(SELECTED_APP_STORAGE_KEY);
  } catch {
    return null;
  }
}

/** Remember the selected app id. */
export function storeAppId(id: string): void {
  try {
    window.localStorage.setItem(SELECTED_APP_STORAGE_KEY, id);
  } catch {
    // storage may be unavailable (private mode); selection still works
  }
}

/** The tabs of the app detail page, in display order. */
export const APP_TABS: { value: AppTab; labelKey: string }[] = [
  { value: "changes", labelKey: "tabs.changes" },
  { value: "commits", labelKey: "versions.allCommits" },
  { value: "history", labelKey: "tabs.history" },
  { value: "log", labelKey: "tabs.log" },
  { value: "config", labelKey: "tabs.config" },
];

/** Is this view the app detail page? */
export function isAppView(view: View): view is { kind: "app"; id: string } {
  return view.kind === "app";
}
