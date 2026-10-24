// Types mirroring the serde shapes of `headway-core` (snake_case on purpose:
// they are the same JSON the `headway --json` CLI prints).

export type AppKind = "cli" | "app";

export type Track =
  | { mode: "commit"; branch?: string | null }
  | { mode: "release"; prerelease: boolean; tag_filter?: string | null }
  | { mode: "tag"; prerelease: boolean; tag_filter?: string | null };

export type TrackMode = Track["mode"];

export interface SourceInstall {
  method: "source";
  build: string;
  workdir?: string | null;
  artifact?: string | null;
  dest?: string | null;
}

export interface AssetInstall {
  method: "asset";
  pattern?: string | null;
  binary?: string | null;
  dest?: string | null;
}

export type Install = SourceInstall | AssetInstall;

export interface TrackedApp {
  id: string;
  name: string;
  /** `owner/name` */
  repo: string;
  kind: AppKind;
  auto_update: boolean;
  git_url?: string | null;
  track: Track;
  install: Install;
}

export interface Settings {
  /** 0 disables background checks */
  check_interval_minutes: number;
  notify: boolean;
  bin_dir: string;
  apps_dir: string;
  gh_fallback: boolean;
}

export interface InstalledRef {
  /** commit SHA (commit mode) or tag (release/tag mode) */
  id: string;
  sha?: string | null;
  version?: string | null;
  installed_at: string;
  path?: string | null;
}

export type UpdateStatus =
  | "up_to_date"
  | "update_available"
  | "not_installed"
  | "diverged";

export interface Candidate {
  /** commit SHA or tag */
  id: string;
  sha?: string | null;
  title: string;
  body?: string | null;
  author?: string | null;
  date?: string | null;
  url?: string | null;
  prerelease: boolean;
}

export interface CheckResult {
  checked_at: string;
  status: UpdateStatus;
  branch?: string | null;
  latest?: Candidate | null;
  /** newest first; may be truncated, see `pending_total` */
  pending: Candidate[];
  pending_total: number;
}

export interface LastError {
  code: string;
  message: string;
  at: string;
  during: "check" | "update";
}

/** Single status for badges, derived by the backend. */
export type AppStatus =
  | "unchecked"
  | "up_to_date"
  | "update_available"
  | "not_installed"
  | "diverged"
  | "error"
  | "busy";

export interface AppView {
  app: TrackedApp;
  installed?: InstalledRef | null;
  last_check?: CheckResult | null;
  last_error?: LastError | null;
  /** an update is running (in this app or in the CLI) */
  busy: boolean;
  status: AppStatus;
  pending_count: number;
}

export interface HistoryEntry {
  at: string;
  action: "update" | "mark";
  from?: string | null;
  to: string;
  version?: string | null;
  ok: boolean;
  message?: string | null;
  log_path?: string | null;
  duration_ms?: number | null;
}

/** What `update` installs: newest, the one right after the installed one, or a ref. */
export type Target = "latest" | "next" | { ref: string };

export interface AssetChoice {
  name: string;
  size: number;
  digest?: string | null;
}

export interface UpdatePlan {
  app_id: string;
  from?: InstalledRef | null;
  to: Candidate;
  /** the target is already installed (nothing would change) */
  up_to_date: boolean;
  /** `--to` the installed version: a real run installs it again */
  reinstall: boolean;
  method: Install["method"];
  steps: string[];
  asset?: AssetChoice | null;
}

export interface UpdateOutcome {
  app_id: string;
  changed: boolean;
  from?: InstalledRef | null;
  to: InstalledRef;
  installed_path?: string | null;
  log_path?: string | null;
  duration_ms: number;
}

export type Stage =
  | "resolve"
  | "fetch"
  | "checkout"
  | "build"
  | "download"
  | "verify"
  | "unpack"
  | "install"
  | "done";

export type LogStream = "stdout" | "stderr" | "info";

export type UpdateEvent =
  | { type: "stage"; app_id: string; stage: Stage; message: string }
  | { type: "log"; app_id: string; stream: LogStream; line: string }
  | { type: "progress"; app_id: string; done: number; total?: number | null };

/** Rejection value of every command. `code` is stable (see the CLI docs). */
export interface CommandError {
  code: string;
  message: string;
  hint?: string | null;
  /** last output lines of a failed build or install */
  details?: string[] | null;
}

export interface UpdateFinished {
  app_id: string;
  ok: boolean;
  outcome?: UpdateOutcome | null;
  error?: CommandError | null;
}

export type TokenSource =
  | "headway_env"
  | "keychain"
  | "github_env"
  | "gh_env"
  | "gh_cli"
  | "none";

export interface RateLimit {
  limit: number;
  remaining: number;
  /** unix seconds */
  reset: number;
}

export interface TokenStatus {
  source: TokenSource;
  masked?: string | null;
  login?: string | null;
  rate?: RateLimit | null;
  /** set when the token could not be verified */
  error?: CommandError | null;
}

export interface ReleaseSummary {
  tag: string;
  name?: string | null;
  published_at?: string | null;
  prerelease: boolean;
  assets: string[];
  suggested_asset?: string | null;
  /** whether that asset holds an app bundle or a CLI binary */
  suggested_kind?: AppKind | null;
}

export interface RepoInspection {
  repo: string;
  description?: string | null;
  default_branch: string;
  private: boolean;
  archived: boolean;
  html_url: string;
  /** detected preset id (cargo, go, swiftpm, …) */
  build_system?: string | null;
  /** binary names found in the build manifest */
  binaries: string[];
  latest_release?: ReleaseSummary | null;
  /** ready-to-save config for this repo */
  suggestion: TrackedApp;
  /** why the suggestion looks the way it does */
  notes: string[];
}

export interface Preset {
  id: string;
  label: string;
  kind: AppKind;
  /** may contain `{bin}` / `{name}` placeholders */
  build: string;
  artifact?: string | null;
  description: string;
}

export interface DoctorCheck {
  name: string;
  status: "ok" | "warn" | "error";
  message: string;
  hint?: string | null;
}

export interface DoctorReport {
  ok: boolean;
  checks: DoctorCheck[];
}

export interface PathsInfo {
  config_file: string;
  state_file: string;
  cache_dir: string;
  log_dir: string;
}

export interface LogFile {
  path: string;
  content: string;
}

export interface CliStatus {
  /** the `headway` binary shipped next to the app, if any */
  bundled_path?: string | null;
  /** where the symlink goes (`<bin_dir>/headway`) */
  link_path: string;
  installed: boolean;
  /** what `<bin_dir>/headway` currently points to */
  points_to?: string | null;
  /** `bin_dir` is on the login shell PATH */
  on_path: boolean;
}

export interface RemovedApp {
  id: string;
  purged: boolean;
}

export interface CheckAllSummary {
  checked: number;
  with_updates: number;
  failed: { id: string; message: string }[];
}
