import { invoke } from "@tauri-apps/api/core";
import type {
  AppView,
  BackupEntry,
  RestoreOutcome,
  InventoryReport,
  AppRecipe,
  SelfUpdateStatus,
  BuildInfo,
  CheckAllSummary,
  CliStatus,
  CommandError,
  CommitMetadata,
  DoctorReport,
  HistoryEntry,
  LogFile,
  PathsInfo,
  Preset,
  RemovedApp,
  RepoInspection,
  SourceLookup,
  Settings,
  Target,
  TokenStatus,
  TrackedApp,
  UpdatePlan,
} from "./types";

export * from "./types";
import { DEFAULT_UPDATE_POLICY } from "./roadmapTypes";

export function withPolicyDefaults(app: TrackedApp): TrackedApp {
  return { ...app, update_policy: app.update_policy ?? DEFAULT_UPDATE_POLICY };
}

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
  commitMetadata: (id: string, sha: string, refresh = false) =>
    invoke<CommitMetadata>("commit_metadata", { id, sha, refresh }),
  ensureSelfSubscription: () => invoke<AppView>("ensure_self_subscription"),
  list: () => invoke<AppView[]>("list_apps"),
  get: (id: string) => invoke<AppView>("get_app", { id }),
  history: (id: string) => invoke<HistoryEntry[]>("app_history", { id }),
  add: (app: TrackedApp) =>
    invoke<AppView>("add_app", { app: withPolicyDefaults(app) }),
  save: (app: TrackedApp) =>
    invoke<AppView>("save_app", { app: withPolicyDefaults(app) }),
  remove: (id: string, purge: boolean) =>
    invoke<RemovedApp>("remove_app", { id, purge }),
  inspect: (repo: string) => invoke<RepoInspection>("inspect_repo", { repo }),
  lookup: (input: string) => invoke<SourceLookup>("lookup_source", { input }),
  discover: (sources?: string[]) =>
    invoke<InventoryReport>("discover_apps", { sources }),
  import: (apps: TrackedApp[]) =>
    invoke<AppView[]>("import_apps", { apps: apps.map(withPolicyDefaults) }),
  recipes: () => invoke<AppRecipe[]>("list_recipes"),
  presets: () => invoke<Preset[]>("list_presets"),
};

export const opsApi = {
  resumeAutoUpdate: (id: string) =>
    invoke<AppView>("resume_auto_update", { id }),
  backups: (id: string) => invoke<BackupEntry[]>("list_backups", { id }),
  restoreBackup: (id: string, backupId: string) =>
    invoke<RestoreOutcome>("restore_backup", { id, backupId }),
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
  fixCliLink: (appPath?: string | null) =>
    invoke<CliStatus>("fix_cli_link", { appPath: appPath ?? null }),
  cliStatus: () => invoke<CliStatus>("cli_status"),
  installCli: () => invoke<CliStatus>("install_cli"),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) =>
    invoke<boolean>("set_autostart", { enabled }),
};

export const systemApi = {
  selfUpdateStatus: () => invoke<SelfUpdateStatus>("self_update_status"),
  requestSelfRestart: () => invoke<SelfUpdateStatus>("request_self_restart"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  appVersion: () => invoke<string>("app_version"),
  buildInfo: () => invoke<BuildInfo>("get_build_info"),
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
