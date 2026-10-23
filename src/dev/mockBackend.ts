// Browser preview of the UI without the Rust backend: `pnpm dev:renderer`, then open
// http://localhost:1420/?mock (add `&theme=dark` or `&lang=en` to switch).
// Loaded only in development builds; the fixtures cover every app status.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import type {
  AppView,
  Candidate,
  CliStatus,
  DoctorReport,
  HistoryEntry,
  PathsInfo,
  Preset,
  Settings,
  TokenStatus,
} from "@/lib/api/types";

const hoursAgo = (hours: number) =>
  new Date(Date.now() - hours * 3_600_000).toISOString();

const commit = (sha: string, title: string, hours: number): Candidate => ({
  id: sha,
  sha,
  title,
  author: "Nathan Sobo",
  date: hoursAgo(hours),
  url: `https://github.com/zed-industries/zed/commit/${sha}`,
  prerelease: false,
});

const zedPending = [
  commit("9f4c1e2a7b3d5f6081a2b3c4d5e6f7a8b9c0d1e2", "Fix cursor jumping after multi-buffer edits", 2),
  commit("8e3b0d19f6a2c4e5d7b8a9c0d1e2f3a4b5c6d7e8", "Speed up project search on large repositories", 5),
  commit("7d2a9c08e5f1b3d4c6a7b8c9d0e1f2a3b4c5d6e7", "agent: show token usage per thread", 9),
];

const apps: AppView[] = [
  {
    app: {
      id: "zed",
      name: "Zed",
      repo: "zed-industries/zed",
      kind: "app",
      auto_update: false,
      track: { mode: "commit", branch: "main" },
      install: {
        method: "source",
        build: "cargo build --release --locked -p zed",
        artifact: "target/release/bundle/osx/*.app",
      },
    },
    installed: {
      id: "6c1f8b97d4e0a2c3b5d6e7f8a9b0c1d2e3f4a5b6",
      sha: "6c1f8b97d4e0a2c3b5d6e7f8a9b0c1d2e3f4a5b6",
      version: "v0.161.0-pre-42-g6c1f8b9",
      installed_at: hoursAgo(26),
      path: "/Applications/Zed.app",
    },
    last_check: {
      checked_at: hoursAgo(0.2),
      status: "update_available",
      branch: "main",
      latest: zedPending[0],
      pending: zedPending,
      pending_total: 3,
    },
    busy: false,
    status: "update_available",
    pending_count: 3,
  },
  {
    app: {
      id: "ripgrep",
      name: "ripgrep",
      repo: "BurntSushi/ripgrep",
      kind: "cli",
      auto_update: true,
      track: { mode: "release", prerelease: false },
      install: { method: "asset", binary: "rg" },
    },
    installed: {
      id: "15.2.0",
      version: "15.2.0",
      installed_at: hoursAgo(50),
      path: "~/.local/bin/rg",
    },
    last_check: {
      checked_at: hoursAgo(0.2),
      status: "up_to_date",
      latest: {
        id: "15.2.0",
        title: "15.2.0",
        body: "Bug fixes and performance improvements.",
        date: hoursAgo(2400),
        prerelease: false,
      },
      pending: [],
      pending_total: 0,
    },
    busy: false,
    status: "up_to_date",
    pending_count: 0,
  },
  {
    app: {
      id: "lazygit",
      name: "lazygit",
      repo: "jesseduffield/lazygit",
      kind: "cli",
      auto_update: false,
      track: { mode: "tag", prerelease: false },
      install: {
        method: "source",
        build: "go build -trimpath -o .headway-out/lazygit .",
        artifact: ".headway-out/lazygit",
      },
    },
    last_check: {
      checked_at: hoursAgo(1),
      status: "not_installed",
      latest: { id: "v0.55.1", sha: "abc1234", title: "v0.55.1", prerelease: false },
      pending: [
        { id: "v0.55.1", sha: "abc1234", title: "v0.55.1", prerelease: false },
        { id: "v0.55.0", sha: "def5678", title: "v0.55.0", prerelease: false },
      ],
      pending_total: 0,
    },
    busy: false,
    status: "not_installed",
    pending_count: 0,
  },
  {
    app: {
      id: "skill-bridge",
      name: "skill-bridge",
      repo: "FrankieeW/skill-bridge",
      kind: "cli",
      auto_update: false,
      track: { mode: "commit" },
      install: { method: "source", build: "uv tool install --force ." },
    },
    installed: { id: "1a2b3c4d5e6f7a8b9c0d", sha: "1a2b3c4d5e6f7a8b9c0d", installed_at: hoursAgo(80) },
    last_error: {
      code: "command_failed",
      message: "`/bin/sh -c 'uv tool install --force .'` failed with exit code 2",
      at: hoursAgo(3),
      during: "update",
    },
    busy: false,
    status: "error",
    pending_count: 1,
  },
];

const history: HistoryEntry[] = [
  {
    at: hoursAgo(26),
    action: "update",
    from: "5b0e7a86c3d9f1b2a4c5d6e7f8a9b0c1d2e3f4a5",
    to: "6c1f8b97d4e0a2c3b5d6e7f8a9b0c1d2e3f4a5b6",
    version: "v0.161.0-pre-42-g6c1f8b9",
    ok: true,
    duration_ms: 412_000,
  },
  {
    at: hoursAgo(30),
    action: "update",
    from: "4a9d6975b2c8e0f1a3b4c5d6e7f8a9b0c1d2e3f4",
    to: "5b0e7a86c3d9f1b2a4c5d6e7f8a9b0c1d2e3f4a5",
    ok: false,
    message: "`cargo build --release --locked -p zed` failed with exit code 101",
    duration_ms: 95_000,
  },
  { at: hoursAgo(72), action: "mark", to: "4a9d6975b2c8e0f1a3b4c5d6e7f8a9b0c1d2e3f4", ok: true },
];

const settings: Settings = {
  check_interval_minutes: 60,
  notify: true,
  bin_dir: "~/.local/bin",
  apps_dir: "/Applications",
  gh_fallback: true,
};

const token: TokenStatus = {
  source: "keychain",
  masked: "ghp_…9xQ2",
  login: "FrankieeW",
  rate: { limit: 5000, remaining: 4973, reset: Math.floor(Date.now() / 1000) + 2400 },
};

const presets: Preset[] = [
  { id: "cargo", label: "Cargo", kind: "cli", build: "cargo build --release --locked", artifact: "target/release/{bin}", description: "Rust binary" },
  { id: "go", label: "Go", kind: "cli", build: "go build -trimpath -o .headway-out/{bin} .", artifact: ".headway-out/{bin}", description: "Go module" },
  { id: "custom", label: "Custom", kind: "cli", build: "", description: "Write your own build command." },
];

const doctor: DoctorReport = {
  ok: true,
  checks: [
    { name: "config", status: "ok", message: "4 app(s) tracked" },
    { name: "git", status: "ok", message: "git version 2.56.0" },
    { name: "github", status: "ok", message: "authenticated as @FrankieeW via the keychain" },
    { name: "bin_dir", status: "warn", message: "~/.local/bin is not on PATH", hint: "add `export PATH=\"$HOME/.local/bin:$PATH\"` to ~/.zprofile" },
  ],
};

const paths: PathsInfo = {
  config_file: "~/Library/Application Support/Headway/config.toml",
  state_file: "~/Library/Application Support/Headway/state.json",
  cache_dir: "~/Library/Caches/Headway",
  log_dir: "~/Library/Logs/Headway",
};

const cli: CliStatus = {
  bundled_path: "/Applications/Headway.app/Contents/MacOS/headway",
  link_path: "~/.local/bin/headway",
  installed: true,
  points_to: "/Applications/Headway.app/Contents/MacOS/headway",
  on_path: true,
};

function handle(cmd: string, args: Record<string, unknown> | undefined): unknown {
  const id = typeof args?.id === "string" ? args.id : "";
  const find = () => apps.find((view) => view.app.id === id) ?? apps[0];
  switch (cmd) {
    case "list_apps":
      return apps;
    case "get_app":
    case "check_app":
    case "mark_installed":
      return find();
    case "app_history":
      return id === "zed" ? history : [];
    case "check_all":
      return { checked: apps.length, with_updates: 2, failed: [] };
    case "read_log":
      return {
        path: "~/Library/Logs/Headway/zed/20261023-101500.000.log",
        content: "10:15:00 [headway] Fetching https://github.com/zed-industries/zed.git\n10:15:04 [headway] Running `cargo build --release --locked -p zed`\n10:21:52 [headway] Installed v0.161.0-pre-42-g6c1f8b9",
      };
    case "get_settings":
    case "save_settings":
      return settings;
    case "token_status":
    case "set_token":
      return token;
    case "list_presets":
      return presets;
    case "doctor":
      return doctor;
    case "get_paths":
      return paths;
    case "cli_status":
    case "install_cli":
      return cli;
    case "get_autostart":
    case "set_autostart":
      return true;
    case "app_version":
      return "0.1.0";
    default:
      return null;
  }
}

export function installMockBackend(): void {
  mockWindows("main");
  mockIPC((cmd, args) => handle(cmd, args as Record<string, unknown> | undefined), {
    shouldMockEvents: true,
  });
}
