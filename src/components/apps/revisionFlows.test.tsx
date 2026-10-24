import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { RevisionDialog } from "./RevisionDialog";
import { RepositorySelector } from "./RepositorySelector";
import { VariantSection } from "./VariantSection";
import { SourceStorageSection } from "./SourceStorageSection";
import { appsApi, type AppView, type RevisionOptions } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { clearRun } from "@/lib/updateRuns";
const sha = "b".repeat(40);
const view: AppView = {
  app: {
    id: "tool",
    name: "Tool",
    kind: "app",
    source: "github",
    repo: "official/tool",
    repositories: [{ repo: "fork/tool", label: "My fork", branch: "dev" }],
    auto_update: false,
    track: { mode: "commit" },
    install: { method: "source", build: "make", artifact: "out/Tool.app" },
  },
  installed: {
    id: "a".repeat(40),
    path: "/Applications/Tool.app",
    installed_at: "2026-10-24T08:00:00Z",
  },
  status: "up_to_date",
  pending_count: 0,
  busy: false,
};
const options: RevisionOptions = {
  reference: "old-tag",
  commit: sha,
  candidate: { id: sha, sha, title: "Target commit", prerelease: false },
  release_tag: "v1",
  release_url: null,
  can_download: true,
  source_build: { method: "source", build: "make", artifact: "out/Tool.app" },
  release_asset: { method: "asset", pattern: "*.zip" },
};
beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
  vi.spyOn(appsApi, "revisionOptions").mockResolvedValue(options);
  const plan = {
    app_id: "tool",
    to: options.candidate,
    up_to_date: false,
    reinstall: false,
    method: "source" as const,
    steps: ["Install → /Applications/Tool.app"],
  };
  vi.spyOn(appsApi, "planRevision").mockResolvedValue(plan);
  vi.spyOn(appsApi, "planVariant").mockResolvedValue({
    ...plan,
    app_id: "side-version",
    steps: ["Install → /Applications/Headway Versions/side-version.app"],
  });
});
afterEach(() => {
  clearRun("tool");
  clearRun("side-version");
});

it("requires replacement confirmation and pins an in-place install to the previewed exact SHA", async () => {
  // Given resolved options for a user-entered tag.
  const install = vi.spyOn(appsApi, "installRevision").mockResolvedValue();
  renderWithProviders(
    <RevisionDialog
      view={view}
      reference="old-tag"
      onClose={() => {}}
      onVariant={() => {}}
    />,
  );
  await screen.findByText("Target commit");
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  expect(install).not.toHaveBeenCalled();
  // When explicitly confirming the replacement.
  fireEvent.click(
    await screen.findByRole("button", { name: "Start installation" }),
  );
  // Then no moving tag or profile flag is passed to the managed runner.
  await waitFor(() =>
    expect(install).toHaveBeenCalledWith("tool", {
      reference: sha,
      method: "configured",
      release_tag: "v1",
    }),
  );
});

it("keeps download disabled for an unreleased commit", async () => {
  // Given source options without a verified release download.
  vi.spyOn(appsApi, "revisionOptions").mockResolvedValue({
    ...options,
    release_tag: null,
    can_download: false,
  });
  // When previewing this commit.
  renderWithProviders(
    <RevisionDialog
      view={view}
      reference={sha}
      onClose={() => {}}
      onVariant={() => {}}
    />,
  );
  // Then the asset method is unavailable independently of the version text.
  expect(
    await screen.findByRole("button", { name: "Download release asset" }),
  ).toBeDisabled();
});

it("requires a manual recipe when no source recipe exists and preserves it for the variant request", async () => {
  // Given an asset app whose selected commit has neither release nor source recipe.
  const assetView: AppView = {
    ...view,
    app: {
      ...view.app,
      source: "github",
      repo: "official/tool",
      track: { mode: "release", prerelease: false },
      install: { method: "asset" },
    },
  };
  vi.spyOn(appsApi, "revisionOptions").mockResolvedValue({
    ...options,
    source_build: null,
    can_download: false,
    release_tag: null,
  });
  const variant = vi.spyOn(appsApi, "installVariant").mockResolvedValue();
  const selected = vi.fn();
  renderWithProviders(
    <RevisionDialog
      view={assetView}
      reference={sha}
      onClose={() => {}}
      onVariant={selected}
    />,
  );
  await screen.findByRole("textbox", { name: "Build command" });
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  expect(variant).not.toHaveBeenCalled();
  // When the user supplies a recipe and explicitly chooses an independent install.
  fireEvent.change(screen.getByRole("textbox", { name: "Build command" }), {
    target: { value: "make custom" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Artifact path" }), {
    target: { value: "out/Custom.app" },
  });
  fireEvent.click(
    screen.getByRole("switch", { name: "Install as an independent version" }),
  );
  fireEvent.change(
    screen.getByRole("textbox", { name: "Independent installation ID" }),
    { target: { value: "side-version" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Start installation" }),
  );
  // Then the child job receives the exact SHA/recipe, shares app data and selects the normal tracked child.
  await waitFor(() =>
    expect(variant).toHaveBeenCalledWith("tool", {
      id: "side-version",
      name: "Tool bbbbbbb",
      revision: {
        reference: sha,
        method: "source",
        source_build: {
          method: "source",
          build: "make custom",
          artifact: "out/Custom.app",
          workdir: null,
          storage: { keep: true, root: null },
        },
      },
    }),
  );
  await waitFor(() => expect(selected).toHaveBeenCalledWith("side-version"));
});

it("launches a tracked variant with only its ID and displays the shared-data notice", async () => {
  // Given an installed independent app.
  const variantView: AppView = {
    ...view,
    app: {
      ...view.app,
      source: "github",
      repo: "official/tool",
      track: { mode: "commit" },
      install: { method: "source", build: "make" },
      variant: { parent_id: "parent", reference: sha },
    },
  };
  const launch = vi.spyOn(appsApi, "launchVariant").mockResolvedValue({
    app_id: "tool",
    pid: 42,
    path: "/Applications/Tool.app",
  });
  renderWithProviders(<VariantSection view={variantView} />);
  expect(
    screen.getByText(
      "Independent installations share the same application data with other versions.",
    ),
  ).toBeVisible();
  // When explicitly launching this installation.
  fireEvent.click(
    screen.getByRole("button", { name: "Launch this installation" }),
  );
  // Then no data mode/profile arguments are sent.
  await waitFor(() => expect(launch).toHaveBeenCalledWith("tool"));
  expect(await screen.findByRole("status")).toHaveTextContent("42");
});

it("activates a repository only after explicit confirmation", async () => {
  // Given a fork choice in the real repository selector.
  const activate = vi
    .spyOn(appsApi, "selectRepository")
    .mockResolvedValue(view);
  const scroll = HTMLElement.prototype.scrollIntoView;
  HTMLElement.prototype.scrollIntoView = () => {};
  try {
    renderWithProviders(<RepositorySelector view={view} />);
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "ArrowDown" });
    fireEvent.click(
      await screen.findByRole("option", { name: "My fork · fork/tool" }),
    );
    expect(activate).not.toHaveBeenCalled();
    // When choosing Activate, then confirming its explicit source change.
    fireEvent.click(
      screen.getByRole("button", { name: "Activate repository" }),
    );
    expect(activate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() =>
      expect(activate).toHaveBeenCalledWith("tool", "fork/tool"),
    );
  } finally {
    HTMLElement.prototype.scrollIntoView = scroll;
  }
});

it("uses source storage status returned by the backend without constructing a path", async () => {
  // Given a custom source location reported by the backend.
  vi.spyOn(appsApi, "sourceStorageStatus").mockResolvedValue({
    path: "/Volumes/Exact/checkout",
    keep: false,
    exists: false,
    custom_root: true,
  });
  // When rendering source retention status.
  renderWithProviders(<SourceStorageSection app={view.app} />);
  // Then the exact path is shown and an absent checkout cannot be revealed.
  expect(await screen.findByText("/Volumes/Exact/checkout")).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Show source checkout" }),
  ).toBeDisabled();
});

it("blocks installation when its read-only plan fails validation", async () => {
  // Given resolved metadata whose plan cannot validate the recipe/destination.
  vi.spyOn(appsApi, "planRevision").mockRejectedValue({
    code: "invalid_input",
    message: "Plan validation failed",
  });
  const install = vi.spyOn(appsApi, "installRevision");
  renderWithProviders(
    <RevisionDialog
      view={view}
      reference={sha}
      onClose={() => {}}
      onVariant={() => {}}
    />,
  );
  await screen.findByText("Target commit");
  // When the user requests installation review.
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  // Then no executable confirmation or background install is exposed.
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Plan validation failed",
  );
  expect(
    screen.queryByRole("button", { name: "Start installation" }),
  ).toBeNull();
  expect(install).not.toHaveBeenCalled();
});

it("does not query source storage for an active asset installation", () => {
  // Given a GitHub app whose active installation downloads release assets.
  const status = vi.spyOn(appsApi, "sourceStorageStatus");
  const { container } = renderWithProviders(
    <SourceStorageSection
      app={{
        ...view.app,
        source: "github",
        repo: "official/tool",
        track: { mode: "release", prerelease: false },
        install: { method: "asset" },
      }}
    />,
  );
  // When rendering its configuration, then the unsupported source-status endpoint stays unused.
  expect(container).toBeEmptyDOMElement();
  expect(status).not.toHaveBeenCalled();
});

it.each(["asset", "configured", "source"] as const)(
  "plans and installs an old published tag with exact SHA using %s asset selection",
  async (method) => {
    // Given a published tag found outside the normal badge metadata window.
    const oldSha = "e".repeat(40);
    const assetView: AppView = {
      ...view,
      app: {
        ...view.app,
        source: "github",
        repo: "official/tool",
        track: { mode: "commit" },
        install: {
          method: "asset",
          pattern: "*.zip",
          dest: "/Applications/Tool.app",
        },
      },
    };
    vi.spyOn(appsApi, "revisionOptions").mockResolvedValue({
      ...options,
      reference: "v0.001.0",
      commit: oldSha,
      candidate: { ...options.candidate, id: oldSha, sha: oldSha },
      release_tag: "v0.001.0",
    });
    const install = vi.spyOn(appsApi, "installRevision").mockResolvedValue();
    const plan = vi.spyOn(appsApi, "planRevision");
    renderWithProviders(
      <RevisionDialog
        view={assetView}
        reference="v0.001.0"
        onClose={() => {}}
        onVariant={() => {}}
      />,
    );
    await screen.findByText("Target commit");
    if (method === "source")
      fireEvent.click(
        screen.getByRole("button", { name: "Build from source" }),
      );
    if (method === "asset")
      fireEvent.click(
        screen.getByRole("button", { name: "Download release asset" }),
      );
    // When previewing and confirming the exact install.
    fireEvent.click(
      screen.getByRole("button", { name: "Review installation" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Start installation" }),
    );
    // Then both calls carry the same pinned SHA and verified release tag.
    const expected = {
      reference: oldSha,
      method,
      release_tag: "v0.001.0",
      ...(method === "asset" ? { release_asset: options.release_asset } : {}),
      ...(method === "source"
        ? {
            source_build: {
              ...options.source_build,
              workdir: null,
              storage: { keep: true, root: null },
            },
          }
        : {}),
    };
    expect(plan).toHaveBeenCalledWith("tool", expected);
    await waitFor(() => expect(install).toHaveBeenCalledWith("tool", expected));
  },
);
