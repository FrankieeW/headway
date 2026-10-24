import { handleRevisions, seedRevisionMock } from "./mockRevisions";
import { mockCommitMetadata } from "./mockCommitMetadata";
import { buildCandidates, handleRoadmap } from "./mockRoadmap";
// Browser preview of the UI without the Rust backend: `pnpm dev:renderer`, then open
// http://localhost:1420/?mock (add `&theme=dark` or `&lang=en` to switch).
// Loaded only in development builds; the fixtures cover every app status.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import type {
  AppView,
  BuildInfo,
  TrackedApp,
  SourceLookup,
} from "@/lib/api/types";

import {
  apps,
  history,
  settings,
  token,
  presets,
  doctor,
  paths,
  cli,
  hoursAgo,
} from "./mockFixtures";

function handle(
  cmd: string,
  args: Record<string, unknown> | undefined,
): unknown {
  const revision = handleRevisions(cmd, args, apps);
  if (revision !== undefined) return revision;
  const roadmap = handleRoadmap(cmd, args, apps);
  if (roadmap !== undefined) return roadmap;
  const id = typeof args?.id === "string" ? args.id : "";
  const find = () => apps.find((view) => view.app.id === id) ?? apps[0];
  switch (cmd) {
    case "commit_metadata": {
      const metadata = mockCommitMetadata(args);
      const active = find();
      const repo =
        active?.app.source === "github"
          ? active.app.repo
          : "zed-industries/zed";
      return {
        ...metadata,
        release_url: metadata.release_url?.replace("zed-industries/zed", repo),
        ci_url: metadata.ci_url?.replace("zed-industries/zed", repo),
      };
    }
    case "list_apps":
      return structuredClone(
        new URLSearchParams(location.search).has("empty") ? [] : apps,
      );
    case "get_app":
    case "check_app":
    case "mark_installed":
      return structuredClone(find());
    case "app_history":
      return id === "zed"
        ? history
        : [
            {
              at: hoursAgo(2),
              action: "update",
              from: "0.9.0",
              to: "1.0.0",
              ok: true,
            },
          ];
    case "lookup_source": {
      const input = typeof args?.input === "string" ? args.input : "";
      const [prefix, ...parts] = input.split(":");
      const name = parts.join(":");
      if (name === "missing")
        throw { code: "not_found", message: "Package not found" };
      const common = {
        id: name.replace(/[^a-z0-9-]/g, "-").replace(/^-+/, "") || "package",
        name,
        kind: "cli",
        auto_update: false,
      } as const;
      const suggestion: TrackedApp =
        prefix === "npm"
          ? { ...common, source: "npm", package: name, dist_tag: "latest" }
          : {
              ...common,
              source: "brew",
              package: name,
              cask: prefix === "cask",
            };
      return {
        source: suggestion.source,
        package: name,
        latest: "2.0.0",
        installed: "1.0.0",
        description: "Preview package",
        homepage: null,
        notes: [],
        suggestion,
      } satisfies SourceLookup;
    }
    case "inspect_repo": {
      const repo = typeof args?.repo === "string" ? args.repo : "example/tool";
      return {
        repo,
        default_branch: "main",
        private: false,
        archived: false,
        html_url: "https://github.com/" + repo,
        binaries: [],
        build_candidates: buildCandidates,
        notes: [],
        suggestion: {
          source: "github",
          id: "preview-repo",
          name: repo,
          repo,
          kind: "cli",
          auto_update: false,
          track: { mode: "commit" },
          install: { method: "source", build: "make" },
        },
      };
    }
    case "add_app":
    case "save_app": {
      // The IPC mock receives the same typed payload as the real adapter.
      const app = args?.app as TrackedApp;
      const view: AppView = {
        ...apps.find((v) => v.app.id === app.id),
        app,
        busy: false,
        status: "unchecked",
        pending_count: 0,
      };
      const index = apps.findIndex((v) => v.app.id === app.id);
      if (index < 0) apps.push(view);
      else apps[index] = view;
      return view;
    }
    case "check_all":
      return { checked: apps.length, with_updates: 2, failed: [] };
    case "read_log":
      return {
        path: "~/Library/Logs/Headway/zed/20261023-101500.000.log",
        content:
          "10:15:00 [headway] Fetching https://github.com/zed-industries/zed.git\n10:15:04 [headway] Running `cargo build --release --locked -p zed`\n10:21:52 [headway] Installed v0.161.0-pre-42-g6c1f8b9",
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
    case "get_build_info":
      return {
        version: "0.1.0",
        commit: null,
        dirty: false,
        target: null,
      } satisfies BuildInfo;
    case "app_version":
      return "0.1.0";
    default:
      return null;
  }
}

export function installMockBackend(): void {
  mockWindows("main");
  seedRevisionMock(apps);
  const scenario = new URLSearchParams(location.search).get("scenario");
  handleRoadmap("ensure_self_subscription", undefined, apps);
  const first = apps.find((view) => view.app.id === "zed");
  const selfApp = apps.find((view) => view.app.id === "headway");
  if (selfApp && scenario === "metadata-self") {
    selfApp.status = "up_to_date";
    selfApp.pending_count = 0;
    selfApp.last_check = {
      status: "up_to_date",
      checked_at: "2026-10-24T08:00:00Z",
      pending: [],
      pending_total: 0,
    };
  }
  if (first && scenario === "paused") first.auto_update_paused = true;
  if (first && scenario === "busy") first.busy = true;
  if (first && scenario === "installed-unconfirmed") {
    first.status = "unknown";
    first.installed = null;
    first.pending_count = 0;
    if (first.last_check)
      first.last_check = {
        ...first.last_check,
        status: "unknown",
        pending: [],
        pending_total: 0,
      };
  }
  if (first?.last_check && scenario === "metadata-many") {
    first.last_check.pending = Array.from({ length: 12 }, (_, index) => ({
      id: (index + 1).toString(16).padStart(40, "0"),
      sha: (index + 1).toString(16).padStart(40, "0"),
      title: `Bounded metadata row ${index + 1}`,
      prerelease: false,
    }));
    first.last_check.pending_total = 12;
    first.pending_count = 12;
  }
  mockIPC(
    (cmd, args) => {
      if (cmd === "list_commits" && new URLSearchParams(location.search).get("scenario") === "commits-loading") return new Promise(resolve => setTimeout(() => resolve(handle(cmd, args as Record<string, unknown> | undefined)), 1500));
      if (cmd === "inspect_repo")
        return new Promise((resolve) =>
          setTimeout(
            () =>
              resolve(handle(cmd, args as Record<string, unknown> | undefined)),
            600,
          ),
        );
      return handle(cmd, args as Record<string, unknown> | undefined);
    },
    {
      shouldMockEvents: true,
    },
  );
}
