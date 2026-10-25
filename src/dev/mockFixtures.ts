import type {
  AppView,
  TrackedApp,
  Candidate,
  CliStatus,
  DoctorReport,
  HistoryEntry,
  PathsInfo,
  Preset,
  Settings,
  TokenStatus,
} from "@/lib/api/types";

export const hoursAgo = (hours: number) =>
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
  commit(
    "9f4c1e2a7b3d5f6081a2b3c4d5e6f7a8b9c0d1e2",
    "Fix cursor jumping after multi-buffer edits",
    2,
  ),
  commit(
    "8e3b0d19f6a2c4e5d7b8a9c0d1e2f3a4b5c6d7e8",
    "Speed up project search on large repositories",
    5,
  ),
  commit(
    "7d2a9c08e5f1b3d4c6a7b8c9d0e1f2a3b4c5d6e7",
    "agent: show token usage per thread",
    9,
  ),
];

export const apps: AppView[] = [
  {
    app: {
      source: "github",
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
      source: "github",
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
      source: "github",
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
      latest: {
        id: "v0.55.1",
        sha: "abc1234",
        title: "v0.55.1",
        prerelease: false,
      },
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
      source: "github",
      id: "skill-bridge",
      name: "skill-bridge",
      repo: "FrankieeW/skill-bridge",
      kind: "cli",
      auto_update: false,
      track: { mode: "commit" },
      install: { method: "source", build: "uv tool install --force ." },
    },
    installed: {
      id: "1a2b3c4d5e6f7a8b9c0d",
      sha: "1a2b3c4d5e6f7a8b9c0d",
      installed_at: hoursAgo(80),
    },
    last_error: {
      code: "command_failed",
      message:
        "`/bin/sh -c 'uv tool install --force .'` failed with exit code 2",
      at: hoursAgo(3),
      during: "update",
    },
    busy: false,
    status: "error",
    pending_count: 1,
  },
];

for (const app of [
  {
    source: "npm",
    id: "npm-tool",
    name: "npm tool",
    package: "@example/tool",
    dist_tag: "latest",
    kind: "cli",
    auto_update: false,
  },
  {
    source: "brew",
    id: "brew-tool",
    name: "Homebrew tool",
    package: "wget",
    cask: false,
    kind: "cli",
    auto_update: false,
  },
  {
    source: "script",
    id: "script-tool",
    name: "Official script",
    update_command: "vendor-tool update",
    version_command: "vendor-tool --version",
    latest_command: null,
    kind: "cli",
    auto_update: false,
  },
] satisfies TrackedApp[]) {
  const latest = { id: "2.0.0", title: "2.0.0", prerelease: false };
  const status = app.source === "script" ? "unknown" : "update_available";
  apps.push({
    app,
    installed: { id: "1.0.0", version: "1.0.0", installed_at: hoursAgo(1) },
    busy: false,
    status,
    pending_count: app.source === "script" ? 0 : 2,
    last_check: {
      checked_at: hoursAgo(1),
      status,
      latest: app.source === "script" ? null : latest,
      pending:
        app.source === "script"
          ? []
          : [latest, { ...latest, id: "1.5.0", title: "1.5.0" }],
      pending_total: app.source === "script" ? 0 : 2,
    },
  });
}

export const history: HistoryEntry[] = [
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
    message:
      "`cargo build --release --locked -p zed` failed with exit code 101",
    duration_ms: 95_000,
  },
  {
    at: hoursAgo(72),
    action: "mark",
    to: "4a9d6975b2c8e0f1a3b4c5d6e7f8a9b0c1d2e3f4",
    ok: true,
  },
];

export const settings: Settings = {
  check_interval_minutes: 60,
  notify: true,
  bin_dir: "~/.local/bin",
  apps_dir: "/Applications",
  gh_fallback: true,
  language: "system",
};

export const token: TokenStatus = {
  source: "keychain",
  masked: "ghp_…9xQ2",
  login: "FrankieeW",
  rate: {
    limit: 5000,
    remaining: 4973,
    reset: Math.floor(Date.now() / 1000) + 2400,
  },
};

export const presets: Preset[] = [
  {
    id: "cargo",
    label: "Cargo",
    kind: "cli",
    build: "cargo build --release --locked",
    artifact: "target/release/{bin}",
    description: "Rust binary",
  },
  {
    id: "go",
    label: "Go",
    kind: "cli",
    build: "go build -trimpath -o .headway-out/{bin} .",
    artifact: ".headway-out/{bin}",
    description: "Go module",
  },
  {
    id: "custom",
    label: "Custom",
    kind: "cli",
    build: "",
    description: "Write your own build command.",
  },
];

export const doctor: DoctorReport = {
  ok: true,
  checks: [
    { name: "config", status: "ok", message: "4 app(s) tracked" },
    { name: "git", status: "ok", message: "git version 2.56.0" },
    {
      name: "github",
      status: "ok",
      message: "authenticated as @FrankieeW via the keychain",
    },
    {
      name: "bin_dir",
      status: "warn",
      message: "~/.local/bin is not on PATH",
      hint: 'add `export PATH="$HOME/.local/bin:$PATH"` to ~/.zprofile',
    },
  ],
};

export const paths: PathsInfo = {
  config_file: "~/Library/Application Support/Headway/config.toml",
  state_file: "~/Library/Application Support/Headway/state.json",
  cache_dir: "~/Library/Caches/Headway",
  log_dir: "~/Library/Logs/Headway",
};

export const cli: CliStatus = {
  bundled_path: "/Applications/Headway.app/Contents/MacOS/headway",
  link_path: "~/.local/bin/headway",
  installed: true,
  points_to: "/Applications/Headway.app/Contents/MacOS/headway",
  on_path: true,
};
