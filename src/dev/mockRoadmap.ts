import type {
  AppRecipe,
  AppView,
  BackupEntry,
  BuildCandidate,
  CliStatus,
  InventoryReport,
  SelfUpdateStatus,
  TrackedApp,
} from "@/lib/api";
import { DEFAULT_UPDATE_POLICY } from "@/lib/api";

export const buildCandidates: BuildCandidate[] = [
  {
    id: "readme-app",
    source: "readme",
    file: "README.md",
    line: 42,
    build: "./scripts/build-headway.sh app",
    artifact: ".headway-out/Headway.app",
    workdir: ".",
    kind: "app",
    reason: "macOS application build",
    recommended: true,
  },
  {
    id: "manifest-cli",
    source: "manifest",
    file: "Cargo.toml",
    line: 12,
    build: "cargo build --release --locked",
    artifact: "target/release/headway",
    kind: "cli",
    reason: "Workspace binary target",
    recommended: false,
  },
  {
    id: "readme-text",
    source: "readme",
    file: "docs/build.md",
    line: 8,
    build: 'echo "<img src=x onerror=alert(1)>"',
    artifact: "dist/tool",
    kind: "cli",
    reason: "README text, review before running",
    recommended: false,
  },
];
const recipe: TrackedApp = {
  source: "github",
  id: "headway-app",
  name: "Headway",
  repo: "FrankieeW/headway",
  kind: "app",
  auto_update: false,
  update_policy: DEFAULT_UPDATE_POLICY,
  track: { mode: "commit" },
  install: {
    method: "source",
    build: "./scripts/build-headway.sh app",
    artifact: ".headway-out/Headway.app",
    verify_command:
      "codesign --verify --deep --strict /Applications/Headway.app",
  },
};
const recipes: AppRecipe[] = [
  {
    id: "headway-app",
    name: "Headway app",
    description: "Build the application and its matching CLI from source.",
    app: recipe,
  },
];
const inventoryApps: TrackedApp[] = [
  {
    id: "jq",
    name: "jq",
    source: "brew",
    package: "jq",
    cask: false,
    kind: "cli",
    auto_update: false,
  },
  {
    id: "typescript",
    name: "TypeScript",
    source: "npm",
    package: "typescript",
    dist_tag: "latest",
    kind: "cli",
    auto_update: false,
  },
  {
    id: "zed",
    name: "Zed",
    source: "brew",
    package: "zed",
    cask: true,
    install_path: "/Applications/Homebrew Casks/Zed.app",
    kind: "app",
    auto_update: false,
  },
  {
    id: "conflict",
    name: "Conflicting package",
    source: "npm",
    package: "conflict",
    dist_tag: "latest",
    kind: "cli",
    auto_update: false,
  },
  {
    id: "firefox-import",
    name: "Firefox",
    source: "brew",
    package: "firefox",
    cask: true,
    kind: "app",
    auto_update: false,
    install_path: "/Applications/Homebrew Casks/Firefox.app",
  },
];
let cli: CliStatus = {
  bundled_path: "/Applications/Headway.app/Contents/MacOS/headway",
  link_path: "/Users/preview/.local/bin/headway",
  installed: false,
  points_to: "/Volumes/Old/Headway.app/Contents/MacOS/headway",
  on_path: true,
  needs_repair: true,
  shadowed: true,
  resolved_path: "/opt/homebrew/bin/headway",
};
let failedRestart = true;
let self: SelfUpdateStatus = {
  running: {
    version: "0.2.0",
    commit: "1111111",
    dirty: false,
    target: "aarch64-apple-darwin",
  },
  installed: {
    version: "0.2.1",
    commit: "2222222",
    dirty: false,
    target: "aarch64-apple-darwin",
  },
  pending_restart: true,
  installed_path: "/Applications/Headway.app",
  app_path: "/Applications/Headway.app",
  restart_requested: false,
  confirmed: false,
};

export function handleRoadmap(
  cmd: string,
  args: Record<string, unknown> | undefined,
  apps: AppView[],
): unknown {
  const scenario = new URLSearchParams(location.search).get("scenario");
  const id = typeof args?.id === "string" ? args.id : "";
  switch (cmd) {
    case "ensure_self_subscription": {
      const index = apps.findIndex(
        (view) =>
          view.app.source === "github" &&
          view.app.repo === "FrankieeW/headway" &&
          view.app.kind === "app",
      );
      const existing = index >= 0 ? apps.splice(index, 1)[0] : undefined;
      const view: AppView = existing ?? {
        app: {
          ...recipe,
          id: "headway",
          auto_update: false,
          update_policy: { ...DEFAULT_UPDATE_POLICY, mode: "notify" },
          source: "github",
          repo: "FrankieeW/headway",
          track: { mode: "commit", branch: "main" },
          install: {
            method: "source",
            build: "./scripts/build-headway.sh app",
            artifact: ".headway-out/Headway.app",
            dest: "/Applications/Headway.app",
            verify_command:
              '"$HEADWAY_INSTALLED_PATH/Contents/MacOS/headway" --json version',
          },
        },
        installed: {
          id: "2222222222222222222222222222222222222222",
          sha: "2222222222222222222222222222222222222222",
          version: "0.2.1",
          installed_at: "2026-10-24T08:00:00Z",
          path: "/Applications/Headway.app",
        },
        status: "unchecked",
        busy: false,
        pending_count: 0,
      };
      apps.unshift(view);
      return view;
    }
    case "list_recipes":
      return scenario === "empty" ? [] : recipes;
    case "discover_apps":
      return {
        candidates:
          scenario === "empty"
            ? []
            : inventoryApps.map((app, index) => ({
                key: `${app.source}:${app.id}`,
                app,
                installed_version: "1.2.3",
                already_tracked: apps.some((view) => view.app.id === app.id),
                conflicts:
                  index === 3
                    ? ["Destination is already managed by another app"]
                    : [],
              })),
        errors:
          scenario === "error"
            ? [
                {
                  source: "npm",
                  error: {
                    code: "not_found",
                    message: "npm is unavailable in this preview scenario",
                  },
                },
              ]
            : [],
      } satisfies InventoryReport;
    case "import_apps": {
      const imported = args?.apps as TrackedApp[];
      for (const app of imported)
        if (!apps.some((view) => view.app.id === app.id))
          apps.push({
            app,
            busy: false,
            status: "unchecked",
            pending_count: 0,
          });
      return apps.filter((view) =>
        imported.some((app) => app.id === view.app.id),
      );
    }
    case "resume_auto_update": {
      const view = apps.find((entry) => entry.app.id === id);
      if (view) view.auto_update_paused = false;
      return view;
    }
    case "list_backups": {
      if (scenario === "error")
        throw { code: "io", message: "Backup cache unavailable" };
      if (scenario === "empty") return [];
      return [1, 2].map((n) => ({
        id: `backup-${n}`,
        app_id: id,
        created_at: `2026-10-${25 - n}T08:00:00Z`,
        installed: {
          id: `revision-${n}`,
          version: `0.16${2 - n}.0`,
          installed_at: `2026-10-${25 - n}T08:00:00Z`,
        },
        artifact_path: `/Users/preview/Library/Caches/Headway/backups/${id}/${n}/Artifact.app`,
      })) satisfies BackupEntry[];
    }
    case "restore_backup": {
      if (scenario === "restore-error")
        throw {
          code: "verify_failed",
          message:
            "Post-install verification failed; the prior artifact was retained.",
        };
      const backupId = typeof args?.backupId === "string" ? args.backupId : "";
      const view = apps.find((entry) => entry.app.id === id);
      const installed = {
        id: backupId === "backup-1" ? "revision-1" : "revision-2",
        version: backupId === "backup-1" ? "0.161.0" : "0.160.0",
        installed_at: new Date().toISOString(),
        path: `/Applications/${view?.app.name ?? id}.app`,
      };
      if (view) view.installed = installed;
      return {
        app_id: id,
        backup_id: backupId,
        installed,
        restored_path: installed.path,
      };
    }
    case "cli_status":
      return cli;
    case "fix_cli_link":
      if (scenario === "error")
        throw {
          code: "conflict",
          message: "The link path contains an unrelated executable.",
        };
      cli = {
        ...cli,
        installed: true,
        needs_repair: false,
        points_to: cli.bundled_path,
      };
      return cli;
    case "self_update_status":
      if (scenario === "restart-error" && failedRestart)
        return {
          ...self,
          pending_restart: false,
          restart_requested: false,
          restart_error:
            "Restarted desktop does not match the requested installed build.",
        };
      if (scenario === "confirmed")
        return {
          ...self,
          running: self.installed,
          pending_restart: false,
          confirmed: true,
        };
      return self;
    case "request_self_restart":
      failedRestart = false;
      self = { ...self, restart_requested: true };
      return self;
    default:
      return undefined;
  }
}
