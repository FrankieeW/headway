import { DEFAULT_UPDATE_POLICY } from "@/lib/api";
import {
  act,
  fireEvent,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  appsApi,
  settingsApi,
  type SourceLookup,
  type TrackedApp,
} from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { appFromForm, emptyForm, formFromApp, useAppForm } from "./useAppForm";
import { AppFormDialog } from "./AppFormDialog";

beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
});
const common = {
  id: "tool",
  name: "Tool",
  kind: "cli",
  auto_update: false,
  update_policy: DEFAULT_UPDATE_POLICY,
} as const;
const sources: TrackedApp[] = [
  { ...common, source: "npm", package: "@vendor/tool", dist_tag: "beta" },
  { ...common, source: "brew", package: "vendor/tap/tool", cask: true },
  {
    ...common,
    source: "script",
    update_command: "tool update",
    version_command: "tool --version",
    latest_command: "https://example.org/version",
  },
];

describe("source form payloads", () => {
  it.each(sources)("preserves $source fields on edit", (app) => {
    // Given an existing local-source app, when editing without changes.
    const saved = appFromForm(formFromApp(app));
    // Then no GitHub configuration leaks into the saved app.
    expect(saved).toEqual(app);
    expect(saved).not.toHaveProperty("repo");
    expect(saved).not.toHaveProperty("track");
    expect(saved).not.toHaveProperty("install");
  });
  it("clears inactive GitHub fields when switching to script", () => {
    const form = {
      ...emptyForm(),
      ...common,
      source: "script" as const,
      repo: "owner/tool",
      build: "make",
      updateCommand: " tool update ",
      latestCommand: " ",
    };
    const saved = appFromForm(form);
    expect(saved).toEqual({
      ...common,
      source: "script",
      update_command: "tool update",
      version_command: null,
      latest_command: null,
    });
  });
});

describe("inspection invalidation", () => {
  it.each(["source", "package", "distTag", "cask"] as const)(
    "ignores a pending response when %s changes",
    async (key) => {
      let resolve: ((value: SourceLookup) => void) | undefined;
      vi.spyOn(appsApi, "lookup").mockImplementation(
        () =>
          new Promise((r) => {
            resolve = r;
          }),
      );
      const { result } = renderHook(() => useAppForm());
      act(() => {
        result.current.update("source", "npm");
        result.current.update("package", "old");
      });
      let pending: Promise<void> | undefined;
      act(() => {
        pending = result.current.inspect("old");
      });
      act(() => {
        switch (key) {
          case "source":
            result.current.update(key, "script");
            break;
          case "package":
            result.current.update(key, "new");
            break;
          case "distTag":
            result.current.update(key, "beta");
            break;
          case "cask":
            result.current.update(key, true);
            break;
        }
      });
      await act(async () => {
        resolve?.({
          source: "npm",
          package: "old",
          suggestion: {
            ...common,
            source: "npm",
            package: "old",
            dist_tag: "latest",
          },
          notes: [],
        });
        await pending;
      });
      expect(result.current.lookup).toBeNull();
      expect(result.current.form.name).toBe("");
      expect(result.current.inspecting).toBe(false);
    },
  );
});

it("blocks an empty script command and submits a valid command without GitHub fields", async () => {
  // Given the real dialog and an IPC-boundary fake.
  vi.spyOn(appsApi, "presets").mockResolvedValue([]);
  vi.spyOn(settingsApi, "get").mockResolvedValue({
    check_interval_minutes: 60,
    notify: true,
    bin_dir: "/tmp/bin",
    apps_dir: "/tmp/apps",
    gh_fallback: false,
  });
  const add = vi.spyOn(appsApi, "add").mockImplementation(async (app) => ({
    app,
    busy: false,
    status: "unchecked",
    pending_count: 0,
  }));
  renderWithProviders(<AppFormDialog open onOpenChange={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Script" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Name" }), {
    target: { value: "Tool" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(add).not.toHaveBeenCalled();
  expect(screen.getByText("Enter an update command.")).toBeVisible();
  // When the required command is supplied and saved.
  fireEvent.change(
    screen.getByRole("textbox", { name: "Update command (required)" }),
    { target: { value: "tool update" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  // Then the backend receives the script variant only.
  await waitFor(() =>
    expect(add).toHaveBeenCalledWith({
      ...common,
      source: "script",
      update_command: "tool update",
      version_command: null,
      latest_command: null,
    }),
  );
});

it("preserves user identity and automation choices while accepting canonical package metadata", async () => {
  // Given deliberately distinct user choices and registry defaults.
  let resolve: ((value: SourceLookup) => void) | undefined;
  vi.spyOn(appsApi, "lookup").mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const { result } = renderHook(() => useAppForm());
  act(() => {
    result.current.update("source", "npm");
    result.current.update("distTag", "beta");
    result.current.update("kind", "app");
    result.current.update("autoUpdate", true);
    result.current.setName("My custom name");
  });
  let pending: Promise<void> | undefined;
  act(() => {
    pending = result.current.inspect("alias");
  });
  act(() => {
    result.current.setId("custom-id");
  });
  // When registry metadata arrives after an additional identity edit.
  await act(async () => {
    resolve?.({
      source: "npm",
      package: "@vendor/canonical",
      notes: [],
      suggestion: {
        ...common,
        source: "npm",
        package: "@vendor/canonical",
        dist_tag: "latest",
      },
    });
    await pending;
  });
  // Then explicit choices win and canonical package data still fills in.
  expect(appFromForm(result.current.form)).toEqual({
    id: "custom-id",
    name: "My custom name",
    source: "npm",
    package: "@vendor/canonical",
    dist_tag: "beta",
    kind: "app",
    auto_update: true,
    update_policy: DEFAULT_UPDATE_POLICY,
  });
});

it("preserves an unfinished manual draft while visiting inventory and removes the competing Save action", async () => {
  // Given an unfinished manual subscription and an empty discovered inventory.
  vi.spyOn(appsApi, "presets").mockResolvedValue([]);
  vi.spyOn(appsApi, "discover").mockResolvedValue({
    candidates: [],
    errors: [],
  });
  vi.spyOn(settingsApi, "get").mockResolvedValue({
    check_interval_minutes: 60,
    notify: true,
    bin_dir: "/tmp/bin",
    apps_dir: "/tmp/apps",
    gh_fallback: false,
  });
  renderWithProviders(<AppFormDialog open onOpenChange={() => {}} />);
  fireEvent.change(screen.getByRole("textbox", { name: "Name" }), {
    target: { value: "Unfinished draft" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  await screen.findByText("No installed packages found.");
  // When returning from the inventory subflow.
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  // Then the draft remains editable and Save returns as the primary action.
  expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue(
    "Unfinished draft",
  );
  expect(screen.getByRole("button", { name: "Save" })).toBeVisible();
});
