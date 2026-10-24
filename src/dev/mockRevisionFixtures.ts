import type { AppView } from "@/lib/api";
const anchor = "9f4c1e2a7b3d5f6081a2b3c4d5e6f7a8b9c0d1e2";
const scenario = () => new URLSearchParams(location.search).get("scenario");
export function seedRevisionMock(apps: AppView[]): void {
  const view = apps.find((item) => item.app.id === "zed");
  if (view?.app.source !== "github") return;
  view.app = {
    ...view.app,
    repositories: [
      {
        repo: "preview-fork/zed",
        label: "My fork",
        branch: "develop",
        git_url: "https://github.com/preview-fork/zed.git",
      },
    ],
  };
  if (scenario() === "cli-variant") {
    view.app = {
      ...view.app,
      kind: "cli",
      track: { mode: "commit", branch: anchor },
      install: {
        method: "source",
        build: "cargo build --release",
        artifact: "target/release/tool",
        dest: "/Users/preview/.local/bin/zed",
      },
      variant: { parent_id: "parent-tool", reference: anchor },
    };
    view.installed = {
      id: anchor,
      sha: anchor,
      version: "v0.162.0",
      installed_at: "2026-10-24T08:00:00Z",
      path: "/Users/preview/.local/bin/zed",
    };
    view.status = "up_to_date";
    view.pending_count = 0;
    view.last_check = {
      checked_at: "2026-10-24T08:00:00Z",
      status: "up_to_date",
      pending: [],
      pending_total: 0,
    };
  }
  if (scenario() === "no-recipe")
    view.app = {
      ...view.app,
      track: { mode: "release", prerelease: false },
      install: { method: "asset", pattern: "*.zip" },
      source_build: null,
    };
}
