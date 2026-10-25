import type { InventoryCandidate, TrackedApp } from "@/lib/api";

export type InventoryCategory =
  "cask" | "formula" | "npm" | "github" | "script";
export type InventoryFilter = "all" | "cask" | "formula" | "npm";

export const inventorySourceKeys = {
  cask: "roadmap.inventoryCask",
  formula: "roadmap.inventoryFormula",
  npm: "sources.npm",
  github: "sources.github",
  script: "sources.script",
} as const satisfies Record<InventoryCategory, string>;

export function inventoryCategory(app: TrackedApp): InventoryCategory {
  switch (app.source) {
    case "brew":
      return app.cask ? "cask" : "formula";
    case "npm":
    case "github":
    case "script":
      return app.source;
    default:
      return unreachable(app);
  }
}

export function inventoryPackage(app: TrackedApp): string {
  switch (app.source) {
    case "brew":
    case "npm":
      return app.package;
    case "github":
      return app.repo;
    case "script":
      return app.update_command;
    default:
      return unreachable(app);
  }
}

export function filterInventory(
  candidates: readonly InventoryCandidate[],
  filter: InventoryFilter,
  query: string,
  label: (category: InventoryCategory) => string,
): readonly InventoryCandidate[] {
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return candidates.filter((candidate) => {
    const category = inventoryCategory(candidate.app);
    if (filter !== "all" && filter !== category) return false;
    if (words.length === 0) return true;
    const path =
      candidate.app.source === "brew" ? (candidate.app.install_path ?? "") : "";
    const text = [
      candidate.app.name,
      candidate.app.id,
      candidate.key,
      inventoryPackage(candidate.app),
      label(category),
      category,
      path,
    ]
      .join(" ")
      .toLocaleLowerCase();
    return words.every((word) => text.includes(word));
  });
}

function unreachable(value: never): never {
  throw new TypeError(`Unsupported inventory source: ${String(value)}`);
}
