import { invoke } from "@tauri-apps/api/core";
import type {
  AppView,
  CheckAllSummary,
  CliStatus,
  CommandError,
  DoctorReport,
  HistoryEntry,
  LogFile,
  PathsInfo,
  Preset,
  RemovedApp,
  RepoInspection,
  Settings,
  Target,
  TokenStatus,
  TrackedApp,
  UpdatePlan,
} from "./types";

export * from "./types";

/** Event names emitted by the desktop backend. */
export const EVENTS = {
  /** config or state changed (this app, the CLI, or the scheduler) — refetch */
  appsChanged: "headway://apps-changed",
  /** payload: UpdateEvent */
  updateEvent: "headway://update-event",
  /** payload: UpdateFinished */
  updateFinished: "headway://update-finished",
  /** payload: { app_id: string } — the tray asked to show an app */
  navigate: "headway://navigate",
} as const;

export const appsApi = {
  list: () => invoke<AppView[]>("list_apps"),
  get: (id: string) => invoke<AppView>("get_app", { id }),
  history: (id: string) => invoke<HistoryEntry[]>("app_history", { id }),
  add: (app: TrackedApp) => invoke<AppView>("add_app", { app }),
  save: (app: TrackedApp) => invoke<AppView>("save_app", { app }),
  remove: (id: string, purge: boolean) =>
    invoke<RemovedApp>("remove_app", { id, purge }),
  inspect: (repo: string) => invoke<RepoInspection>("inspect_repo", { repo }),
  presets: () => invoke<Preset[]>("list_presets"),
};

export const opsApi = {
  check: (id: string) => invoke<AppView>("check_app", { id }),
  checkAll: () => invoke<CheckAllSummary>("check_all"),
  plan: (id: string, target: Target) =>
    invoke<UpdatePlan>("plan_update", { id, target }),
  /** Starts in the background; follow EVENTS.updateEvent / updateFinished. */
  startUpdate: (id: string, target: Target) =>
    invoke<void>("start_update", { id, target }),
  cancel: (id: string) => invoke<boolean>("cancel_update", { id }),
  /** `reference`: "latest", a commit SHA, or a tag */
  markInstalled: (id: string, reference: string) =>
    invoke<AppView>("mark_installed", { id, reference }),
  readLog: (id: string) => invoke<LogFile | null>("read_log", { id }),
};

export const settingsApi = {
  get: () => invoke<Settings>("get_settings"),
  save: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  tokenStatus: () => invoke<TokenStatus>("token_status"),
  setToken: (token: string) => invoke<TokenStatus>("set_token", { token }),
  clearToken: () => invoke<TokenStatus>("clear_token"),
  doctor: () => invoke<DoctorReport>("doctor"),
  paths: () => invoke<PathsInfo>("get_paths"),
  cliStatus: () => invoke<CliStatus>("cli_status"),
  installCli: () => invoke<CliStatus>("install_cli"),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) =>
    invoke<boolean>("set_autostart", { enabled }),
};

export const systemApi = {
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  appVersion: () => invoke<string>("app_version"),
};

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

/** Human-readable message for anything a command rejected with. */
export function errorMessage(error: unknown): string {
  if (isCommandError(error)) {
    return error.hint ? `${error.message} — ${error.hint}` : error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
