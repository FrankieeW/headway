import type { TrackedApp } from "./api";

export function sourceDetail(app: TrackedApp): string {
  switch (app.source) {
    case "github":
      return app.repo;
    case "npm":
      return `${app.package} · ${app.dist_tag}`;
    case "brew":
      return `${app.package} · ${app.cask ? "cask" : "formula"}`;
    case "script":
      return app.update_command;
  }
}

export function supportsRefs(app: TrackedApp): boolean {
  return app.source === "github" || app.source === "npm";
}
