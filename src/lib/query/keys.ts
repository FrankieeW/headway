/**
 * Query keys for every cached read.
 *
 * Keys are plain arrays so they can be invalidated by prefix:
 * `["app", id]` invalidates one app, `["apps"]` the whole list.
 */
export const queryKeys = {
  sourceMatches: (input: string, offline = false) =>
    ["source-matches", input, offline] as const,
  /** the tracked app list */
  apps: ["apps"] as const,
  /** one app with its status */
  app: (id: string) => ["app", id] as const,
  /** update history of one app */
  history: (id: string) => ["history", id] as const,
  /** persisted preferences */
  settings: ["settings"] as const,
  /** GitHub token status */
  token: ["token"] as const,
  /** build presets offered by the backend */
  presets: ["presets"] as const,
  /** config / state / cache / log directories */
  paths: ["paths"] as const,
  /** `headway` CLI installation status */
  cli: ["cli"] as const,
  /** launch at login */
  autostart: ["autostart"] as const,
  /** the log file of the last update run */
  log: (id: string) => ["log", id] as const,
  /** environment diagnostics, only fetched on demand */
  doctor: ["doctor"] as const,
};

export type QueryKeys = typeof queryKeys;

/** Roots of the queries that read `config.toml` / `state.json` / logs. */
const LOCAL_DATA_ROOTS = new Set([
  "apps",
  "app",
  "history",
  "settings",
  "log",
  "cli",
  "backups",
  "self-update",
  "source-storage",
  "installation",
]);

/**
 * Matches the queries to refetch when local files change. Queries that call
 * GitHub (token status) or are expensive (doctor) are left alone.
 */
export function isLocalDataQuery(query: {
  queryKey: readonly unknown[];
}): boolean {
  return LOCAL_DATA_ROOTS.has(String(query.queryKey[0]));
}
