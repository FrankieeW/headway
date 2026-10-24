import { RevisionDialog } from "./RevisionDialog";
import {
  act,
  fireEvent,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AppFormDialog } from "./AppFormDialog";
import { appFromForm, useAppForm } from "./useAppForm";
import {
  appsApi,
  settingsApi,
  type RepoInspection,
  type TrackedApp,
} from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
const sha = "d".repeat(40);
const child: TrackedApp = {
  source: "github",
  id: "asset-child",
  name: "Asset child",
  kind: "app",
  repo: "official/tool",
  track: { mode: "commit", branch: sha },
  auto_update: false,
  install: {
    method: "asset",
    pattern: "*.zip",
    binary: "Tool.app",
    dest: "/Applications/Headway Versions/asset-child.app",
    verify_command: "verify asset",
  },
  source_build: {
    method: "source",
    build: "make saved-source",
    workdir: "desktop",
    artifact: "out/Tool.app",
    dest: "/Applications/Saved source.app",
    verify_command: "verify source",
    storage: { keep: false, root: "~/Saved Sources" },
  },
  release_asset: {
    method: "asset",
    pattern: "*arm64.zip",
    binary: "Saved.app",
    dest: "/Applications/Saved asset.app",
    verify_command: "verify saved asset",
  },
  variant: {
    parent_id: "parent",
    reference: sha,
    release_tag: "v1",
    branch: "main",
  },
};
const inspection: RepoInspection = {
  repo: "official/tool",
  default_branch: "main",
  private: false,
  archived: false,
  html_url: "https://github.com/official/tool",
  binaries: [],
  notes: [],
  latest_release: {
    tag: "v1",
    assets: ["Tool.zip"],
    suggested_kind: "app",
    suggested_asset: "Tool.zip",
    prerelease: false,
  },
  suggestion: {
    source: "github",
    id: "tool",
    name: "Tool",
    kind: "app",
    repo: "official/tool",
    track: { mode: "commit" },
    auto_update: false,
    install: { method: "source", build: "make suggested" },
  },
};
beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
  vi.spyOn(appsApi, "presets").mockResolvedValue([]);
  vi.spyOn(settingsApi, "get").mockResolvedValue({
    check_interval_minutes: 60,
    notify: true,
    bin_dir: "/tmp/bin",
    apps_dir: "/tmp/apps",
    gh_fallback: false,
  });
});

it("edits and saves an existing commit-tracked asset child without coercing its method", async () => {
  // Given the actual backend shape of a downloaded independent child.
  const save = vi.spyOn(appsApi, "save").mockImplementation(async (app) => ({
    app,
    busy: false,
    status: "unchecked",
    pending_count: 0,
  }));
  renderWithProviders(
    <AppFormDialog app={child} open onOpenChange={() => {}} />,
  );
  const asset = await screen.findByRole("button", {
    name: "Download release asset",
  });
  expect(asset).toBeEnabled();
  expect(asset).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByDisplayValue("*.zip")).toBeVisible();
  // When an unrelated display name edit is saved.
  fireEvent.change(screen.getByRole("textbox", { name: "Name" }), {
    target: { value: "Renamed child" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  // Then method, pinned tracking, destination, optional recipes and variant metadata survive.
  await waitFor(() => expect(save).toHaveBeenCalledOnce());
  expect(save.mock.calls[0]?.[0]).toMatchObject({
    ...child,
    name: "Renamed child",
  });
});

it("preserves commit asset configuration while re-inspecting the repository", async () => {
  // Given saved asset configuration and a source-oriented inspection recommendation.
  vi.spyOn(appsApi, "inspect").mockResolvedValue(inspection);
  const { result } = renderHook(() => useAppForm(child));
  // When repository metadata is refreshed.
  await act(async () => result.current.inspect("official/tool"));
  // Then inspection cannot coerce the saved install method.
  expect(appFromForm(result.current.form)).toMatchObject(child);
});

it("round-trips asset/source choices with saved recipes, source storage and the current destination intact", async () => {
  // Given an asset child with distinct saved source/asset recipes.
  const save = vi.spyOn(appsApi, "save").mockImplementation(async (app) => ({
    app,
    busy: false,
    status: "unchecked",
    pending_count: 0,
  }));
  renderWithProviders(
    <AppFormDialog app={child} open onOpenChange={() => {}} />,
  );
  // When switching to its saved source recipe and back, across tracking modes.
  fireEvent.click(screen.getByRole("button", { name: "Build from source" }));
  expect(screen.getByRole("textbox", { name: "Build command" })).toHaveValue(
    "make saved-source",
  );
  expect(
    screen.getByRole("textbox", { name: "Source base directory (optional)" }),
  ).toHaveValue("~/Saved Sources");
  expect(
    screen.getByRole("switch", { name: "Keep source checkout" }),
  ).not.toBeChecked();
  expect(
    screen.getByDisplayValue("/Applications/Headway Versions/asset-child.app"),
  ).toBeVisible();
  fireEvent.click(
    screen.getByRole("button", { name: "Download release asset" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Tags" }));
  expect(
    screen.getByRole("button", { name: "Download release asset" }),
  ).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(screen.getByRole("button", { name: "Every commit" }));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  // Then neither inactive recipes nor verification/storage/destination are discarded by toggling.
  await waitFor(() => expect(save).toHaveBeenCalledOnce());
  expect(save.mock.calls[0]?.[0]).toMatchObject(child);
});

it("keeps the default/user-chosen source method when tracking mode changes after inspection", async () => {
  // Given inspection that includes an available release asset.
  vi.spyOn(appsApi, "inspect").mockResolvedValue(inspection);
  renderWithProviders(<AppFormDialog open onOpenChange={() => {}} />);
  fireEvent.change(screen.getByRole("textbox", { name: "Repository" }), {
    target: { value: "official/tool" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Inspect" }));
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Tool"),
  );
  fireEvent.click(screen.getByRole("button", { name: "Build from source" }));
  // When selecting release tracking.
  fireEvent.click(screen.getByRole("button", { name: "Releases" }));
  // Then available assets do not silently replace the source choice.
  expect(
    screen.getByRole("button", { name: "Build from source" }),
  ).toHaveAttribute("aria-pressed", "true");
});

it("round-trips source/asset/source without replacing explicit alternate recipes or the current source storage", () => {
  // Given an active source recipe distinct from both explicitly retained alternatives.
  const source: TrackedApp = {
    ...child,
    source: "github",
    install: {
      method: "source",
      build: "make active-source",
      workdir: "src",
      artifact: "out/Active.app",
      dest: "/Applications/Active.app",
      verify_command: "verify active",
      storage: { keep: false, root: "/Volumes/Active Sources" },
    },
  };
  const { result } = renderHook(() => useAppForm(source));
  // When switching to the asset method and back.
  act(() => result.current.update("installMethod", "asset"));
  expect(result.current.form.pattern).toBe("*arm64.zip");
  expect(result.current.form.verifyCommand).toBe("verify saved asset");
  act(() => result.current.update("installMethod", "source"));
  // Then the active source and both explicit alternatives survive unchanged.
  expect(appFromForm(result.current.form)).toMatchObject(source);
});

it("retains a previously active source recipe when saving asset mode and switching back after reopening", () => {
  // Given a source install without an explicit stored alternate recipe.
  const source: TrackedApp = {
    ...child,
    source: "github",
    install: {
      method: "source",
      build: "make retained",
      artifact: "out/Retained.app",
      dest: "/Applications/Retained.app",
      verify_command: "verify retained",
      storage: { keep: false, root: "~/Retained Sources" },
    },
    source_build: null,
  };
  const first = renderHook(() => useAppForm(source));
  act(() => first.result.current.update("installMethod", "asset"));
  const asset = appFromForm(first.result.current.form);
  first.unmount();
  // When reopening the asset configuration and switching back to source.
  const reopened = renderHook(() => useAppForm(asset));
  act(() => reopened.result.current.update("installMethod", "source"));
  // Then its build, verification, retention and destination remain available.
  expect(appFromForm(reopened.result.current.form)).toMatchObject({
    install: source.source === "github" ? source.install : {},
  });
});

it("rejects an overlong independent ID before planning or starting a job", async () => {
  // Given a valid revision preview and a programmatically supplied 65-character ID.
  vi.spyOn(appsApi, "revisionOptions").mockResolvedValue({
    reference: sha,
    commit: sha,
    candidate: { id: sha, sha, title: "Target", prerelease: false },
    release_tag: "v1",
    release_url: null,
    can_download: true,
    source_build:
      child.source === "github" ? (child.source_build ?? null) : null,
    release_asset: { method: "asset" },
  });
  const plan = vi.spyOn(appsApi, "planVariant");
  const install = vi.spyOn(appsApi, "installVariant");
  renderWithProviders(
    <RevisionDialog
      view={{ app: child, busy: false, status: "unchecked", pending_count: 0 }}
      reference={sha}
      onClose={() => {}}
      onVariant={() => {}}
    />,
  );
  fireEvent.click(
    await screen.findByRole("switch", {
      name: "Install as an independent version",
    }),
  );
  fireEvent.change(
    screen.getByRole("textbox", { name: "Independent installation ID" }),
    { target: { value: "x".repeat(65) } },
  );
  // When the user requests review.
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  // Then the boundary is enforced before any executable plan/job is submitted.
  expect(await screen.findByRole("alert")).toHaveTextContent("1–64");
  expect(plan).not.toHaveBeenCalled();
  expect(install).not.toHaveBeenCalled();
});
