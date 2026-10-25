import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AddSources } from "./AddSources";
import {
  appsApi,
  DEFAULT_UPDATE_POLICY,
  type InventoryCandidate,
} from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";

const items: InventoryCandidate[] = [
  {
    key: "brew editor",
    app: {
      id: "formula-editor",
      name: "Editor",
      source: "brew",
      package: "editor",
      cask: false,
      kind: "cli",
      auto_update: true,
    },
    installed_version: "1.0",
    already_tracked: false,
    conflicts: [],
  },
  {
    key: "brew cask vendor/apps/editor",
    app: {
      id: "cask-editor",
      name: "Editor",
      source: "brew",
      package: "vendor/apps/editor",
      cask: true,
      kind: "app",
      auto_update: true,
      install_path: "/Applications/Custom Apps/Editor.app",
    },
    installed_version: "2.0",
    already_tracked: false,
    conflicts: [],
  },
  {
    key: "npm @example/typescript",
    app: {
      id: "typescript",
      name: "TypeScript",
      source: "npm",
      package: "@example/typescript",
      dist_tag: "latest",
      kind: "cli",
      auto_update: false,
    },
    installed_version: "3.0",
    already_tracked: false,
    conflicts: [],
  },
  {
    key: "brew cask tracked",
    app: {
      id: "tracked",
      name: "Tracked",
      source: "brew",
      package: "tracked",
      cask: true,
      kind: "app",
      auto_update: false,
    },
    already_tracked: true,
    conflicts: [],
  },
  {
    key: "brew conflict",
    app: {
      id: "conflict",
      name: "Conflict",
      source: "brew",
      package: "conflict",
      cask: false,
      kind: "cli",
      auto_update: false,
    },
    already_tracked: false,
    conflicts: ["Managed by another subscription"],
  },
];

beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
});

async function inventory() {
  const discover = vi
    .spyOn(appsApi, "discover")
    .mockResolvedValue({ candidates: items, errors: [] });
  renderWithProviders(<AddSources onRecipe={() => {}} onImported={() => {}} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  await screen.findByRole("checkbox", { name: "Editor · Homebrew Cask" });
  return discover;
}

it("distinguishes equally named casks and formulae while keeping ineligible items disabled", async () => {
  // Given a mixed installed inventory with duplicate display names.
  await inventory();
  // When choosing only casks.
  fireEvent.click(screen.getByRole("button", { name: /^Cask/ }));
  // Then formulae disappear and the already-tracked cask stays disabled.
  expect(
    screen.getByRole("checkbox", { name: "Editor · Homebrew Cask" }),
  ).toBeEnabled();
  expect(
    screen.queryByRole("checkbox", { name: "Editor · Homebrew Formula" }),
  ).toBeNull();
  expect(
    screen.getByRole("checkbox", { name: "Tracked · Homebrew Cask" }),
  ).toBeDisabled();
});

it("searches names, scoped packages and reported paths locally", async () => {
  // Given one fetched inventory.
  const discover = await inventory();
  const search = screen.getByRole("searchbox");
  // When searching across a source, package namespace and installation directory.
  fireEvent.change(search, { target: { value: "  CASK vendor CUSTOM  " } });
  // Then the matching cask remains without fetching or importing anything.
  expect(screen.getAllByRole("checkbox")).toHaveLength(1);
  expect(
    screen.getByRole("checkbox", { name: "Editor · Homebrew Cask" }),
  ).toBeVisible();
  expect(discover).toHaveBeenCalledTimes(1);
});

it("finds a global npm package by its scoped identifier", async () => {
  // Given a mixed inventory.
  await inventory();
  // When searching the package scope rather than its display name.
  fireEvent.change(screen.getByRole("searchbox"), {
    target: { value: "NPM @EXAMPLE" },
  });
  // Then the npm entry is the sole result and its source has a human label.
  expect(screen.getAllByRole("checkbox")).toHaveLength(1);
  expect(
    screen.getByRole("checkbox", { name: "TypeScript · npm" }),
  ).toBeVisible();
});

it("imports selections hidden by later filters with their cask metadata intact", async () => {
  // Given a selected formula and a cask selected after filtering.
  const imported = vi.spyOn(appsApi, "import").mockResolvedValue([]);
  await inventory();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Editor · Homebrew Formula" }),
  );
  fireEvent.click(screen.getByRole("button", { name: /^Cask/ }));
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Editor · Homebrew Cask" }),
  );
  expect(imported).not.toHaveBeenCalled();
  // When the user explicitly imports the entire selection.
  fireEvent.click(screen.getByRole("button", { name: "Import selected (2)" }));
  // Then both selected identities survive, and neither gains automatic updates.
  await waitFor(() =>
    expect(imported).toHaveBeenCalledWith(
      items.slice(0, 2).map(({ app }) => ({
        ...app,
        auto_update: false,
        update_policy: DEFAULT_UPDATE_POLICY,
      })),
    ),
  );
});

it("selects only eligible matching results and can clear a no-match view", async () => {
  // Given a cask-only view with both eligible and already tracked entries.
  await inventory();
  fireEvent.click(screen.getByRole("button", { name: /^Cask/ }));
  fireEvent.click(screen.getByRole("button", { name: "Select matching" }));
  expect(
    screen.getByRole("checkbox", { name: "Editor · Homebrew Cask" }),
  ).toBeChecked();
  expect(
    screen.getByRole("checkbox", { name: "Tracked · Homebrew Cask" }),
  ).not.toBeChecked();
  // When the search has no results and its filters are cleared.
  fireEvent.change(screen.getByRole("searchbox"), {
    target: { value: "no-such-package" },
  });
  expect(screen.queryAllByRole("checkbox")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
  // Then all entries return and the selected cask is retained.
  expect(screen.getAllByRole("checkbox")).toHaveLength(5);
  expect(
    screen.getByRole("checkbox", { name: "Editor · Homebrew Cask" }),
  ).toBeChecked();
  expect(
    screen.getByRole("checkbox", { name: "Conflict · Homebrew Formula" }),
  ).toBeDisabled();
});
