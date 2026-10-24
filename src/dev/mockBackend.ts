// Browser preview of the UI without the Rust backend: `pnpm dev:renderer`, then open
// http://localhost:1420/?mock (add `&theme=dark` or `&lang=en` to switch).
// Loaded only in development builds; the fixtures cover every app status.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import type { AppView, BuildInfo, TrackedApp, SourceLookup } from "@/lib/api/types";

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
  const id = typeof args?.id === "string" ? args.id : "";
  const find = () => apps.find((view) => view.app.id === id) ?? apps[0];
  switch (cmd) {
    case "list_apps":
      return new URLSearchParams(location.search).has("empty") ? [] : apps;
    case "get_app":
    case "check_app":
    case "mark_installed":
      return find();
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
      return { version: "0.1.0", commit: null, dirty: false, target: null } satisfies BuildInfo;
    case "app_version":
      return "0.1.0";
    default:
      return null;
  }
}

export function installMockBackend(): void {
  mockWindows("main");
  mockIPC(
    (cmd, args) => handle(cmd, args as Record<string, unknown> | undefined),
    {
      shouldMockEvents: true,
    },
  );
}
