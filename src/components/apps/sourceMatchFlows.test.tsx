import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { appsApi, settingsApi, type SourceMatches } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import {
  matchInspection,
  matchInventory,
  matchLookup,
  packageMatches,
  repositoryMatches,
} from "@/test/matchFixtures";
import { AppFormDialog } from "./AppFormDialog";
import { AddSources } from "./AddSources";

beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
  mockIPC((cmd, args) => {
    if (cmd === "match_source")
      return String(args && "input" in args ? args.input : "").startsWith(
        "cask:",
      )
        ? repositoryMatches
        : packageMatches;
    return undefined;
  });
  vi.spyOn(appsApi, "presets").mockResolvedValue([]);
  vi.spyOn(settingsApi, "get").mockResolvedValue({
    check_interval_minutes: 60,
    notify: true,
    bin_dir: "/tmp/bin",
    apps_dir: "/tmp/apps",
    gh_fallback: false,
  });
  vi.spyOn(appsApi, "inspect").mockResolvedValue(matchInspection);
  vi.spyOn(appsApi, "lookup").mockResolvedValue(matchLookup);
});
afterEach(clearMocks);

function inspectRepository() {
  fireEvent.change(screen.getByRole("textbox", { name: "Repository" }), {
    target: { value: "acme/editor" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Inspect" }));
}

it("replaces recipe X identity when importing Zed through Use GitHub", async () => {
  vi.spyOn(appsApi, "recipes").mockResolvedValue([
    {
      id: "recipe-x",
      name: "Recipe X",
      description: "A different recipe",
      app: {
        ...matchLookup.suggestion,
        id: "recipe-x",
        name: "Recipe X",
        kind: "cli",
      },
    },
  ]);
  vi.spyOn(appsApi, "discover").mockResolvedValue({
    candidates: [
      {
        ...matchInventory,
        repositories: repositoryMatches.repositories.map((match) => ({
          ...match,
          repo: "zed-industries/zed",
        })),
      },
    ],
    errors: [],
  });
  vi.mocked(appsApi.inspect).mockResolvedValue({
    ...matchInspection,
    repo: "zed-industries/zed",
    suggestion: {
      source: "github",
      repo: "zed-industries/zed",
      id: "zed",
      name: "Zed",
      kind: "app",
      auto_update: false,
      track: { mode: "commit" },
      install: { method: "source", build: "make zed" },
    },
  });
  renderWithProviders(<AppFormDialog open onOpenChange={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Source recipes" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Edit this recipe" }),
  );
  expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Recipe X");
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Use GitHub: zed-industries/zed",
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Zed"),
  );
  expect(screen.getByRole("textbox", { name: "Id" })).toHaveValue("zed");
  expect(appsApi.inspect).toHaveBeenCalledWith("zed-industries/zed");
});

it("switches an inspected GitHub repository to the chosen Homebrew cask and immediately looks it up", async () => {
  renderWithProviders(<AppFormDialog open onOpenChange={vi.fn()} />);
  inspectRepository();
  // When the Homebrew suggestion is explicitly selected.
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Use Homebrew: Cask · vendor/apps/editor",
    }),
  );
  // Then the new source, package and cask are used by lookup and the form.
  await waitFor(() =>
    expect(appsApi.lookup).toHaveBeenCalledWith("cask:vendor/apps/editor"),
  );
  expect(screen.getByRole("textbox", { name: "Package" })).toHaveValue(
    "vendor/apps/editor",
  );
  expect(screen.getByRole("switch", { name: /Cask/ })).toBeChecked();
  expect(await screen.findByText("Package lookup")).toBeVisible();
  expect(screen.queryByText("Repository inspection")).toBeNull();
});

it("switches a looked-up Homebrew cask to GitHub and immediately inspects it", async () => {
  renderWithProviders(<AppFormDialog open onOpenChange={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Homebrew" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Package" }), {
    target: { value: "editor" },
  });
  fireEvent.click(screen.getByRole("switch", { name: /Cask/ }));
  fireEvent.click(screen.getByRole("button", { name: "Inspect" }));
  // When the repository suggestion is chosen.
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Track from GitHub: acme/editor",
    }),
  );
  // Then stale package results disappear and the repository is inspected.
  await waitFor(() =>
    expect(appsApi.inspect).toHaveBeenCalledWith("acme/editor"),
  );
  expect(screen.getByRole("textbox", { name: "Repository" })).toHaveValue(
    "acme/editor",
  );
  expect(await screen.findByText("Repository inspection")).toBeVisible();
  expect(screen.queryByText("Package lookup")).toBeNull();
});

it("uses an imported GitHub repository without toggling its checkbox", async () => {
  vi.spyOn(appsApi, "discover").mockResolvedValue({
    candidates: [matchInventory],
    errors: [],
  });
  const onUseRepository = vi.fn();
  renderWithProviders(
    <AddSources
      onRecipe={vi.fn()}
      onImported={vi.fn()}
      onUseRepository={onUseRepository}
    />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  const checkbox = await screen.findByRole("checkbox");
  // When choosing the repository action outside the checkbox label.
  fireEvent.click(
    screen.getByRole("button", { name: "Use GitHub: acme/editor" }),
  );
  expect(onUseRepository).toHaveBeenCalledWith("acme/editor");
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  // Then reopening preserves the original unchecked selection.
  expect(await screen.findByRole("checkbox")).not.toBeChecked();
  expect(checkbox).not.toBeChecked();
});

it("closes inventory and inspects the selected GitHub repository in the add dialog", async () => {
  vi.spyOn(appsApi, "discover").mockResolvedValue({
    candidates: [matchInventory],
    errors: [],
  });
  renderWithProviders(<AppFormDialog open onOpenChange={vi.fn()} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Use GitHub: acme/editor" }),
  );
  await waitFor(() =>
    expect(appsApi.inspect).toHaveBeenCalledWith("acme/editor"),
  );
  expect(screen.queryByRole("searchbox")).toBeNull();
  expect(screen.getByRole("textbox", { name: "Repository" })).toHaveValue(
    "acme/editor",
  );
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

it("keeps editing usable while matches load and discards suggestions after the input changes", async () => {
  let resolve: ((matches: SourceMatches) => void) | undefined;
  mockIPC((cmd) =>
    cmd === "match_source"
      ? new Promise<SourceMatches>((done) => {
          resolve = done;
        })
      : undefined,
  );
  renderWithProviders(<AppFormDialog open onOpenChange={vi.fn()} />);
  inspectRepository();
  expect(await screen.findByText("Searching for matches…")).toBeVisible();
  // When the user edits while the advisory request is pending.
  fireEvent.change(screen.getByRole("textbox", { name: "Name" }), {
    target: { value: "Custom editor" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Repository" }), {
    target: { value: "other/editor" },
  });
  await act(async () => {
    resolve?.(packageMatches);
  });
  // Then stale suggestions cannot replace the new input or block saving.
  expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue(
    "Custom editor",
  );
  expect(
    screen.queryByRole("button", {
      name: "Use Homebrew: Cask · vendor/apps/editor",
    }),
  ).toBeNull();
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

it("shows match failures inline without blocking the inspected form", async () => {
  mockIPC((cmd) => {
    if (cmd === "match_source")
      throw { code: "command_failed", message: "Homebrew is unavailable" };
  });
  renderWithProviders(<AppFormDialog open onOpenChange={vi.fn()} />);
  inspectRepository();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Homebrew is unavailable",
  );
  expect(screen.getByRole("textbox", { name: "Name" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

it.each([matchInspection.suggestion, matchLookup.suggestion])(
  "never fetches matches while editing a $source app",
  async (app) => {
    const invoke = vi.fn();
    mockIPC(invoke);
    renderWithProviders(
      <AppFormDialog open app={app} onOpenChange={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Inspect" }));
    await screen.findByText(
      app.source === "github" ? "Repository inspection" : "Package lookup",
    );
    expect(invoke.mock.calls.some(([cmd]) => cmd === "match_source")).toBe(
      false,
    );
  },
);
