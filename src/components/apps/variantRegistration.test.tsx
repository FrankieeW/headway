import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AppPage } from "./AppPage";
import { appsApi, settingsApi, type AppView } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { clearRun } from "@/lib/updateRuns";
const sha = "c".repeat(40);
const parent: AppView = {
  app: {
    source: "github",
    id: "parent",
    name: "Parent",
    repo: "official/tool",
    kind: "app",
    track: { mode: "commit" },
    auto_update: false,
    install: { method: "source", build: "make", artifact: "out/Tool.app" },
  },
  status: "unchecked",
  busy: false,
  pending_count: 0,
};
beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
});
afterEach(() => clearRun("parent-ccccccc"));

it("waits for the child to appear in the tracked list before selecting its page", async () => {
  // Given a background variant command that returns before registration completes.
  let list = [parent];
  vi.spyOn(appsApi, "list").mockImplementation(async () => list);
  vi.spyOn(appsApi, "get").mockResolvedValue(parent);
  vi.spyOn(appsApi, "history").mockResolvedValue([]);
  vi.spyOn(appsApi, "presets").mockResolvedValue([]);
  vi.spyOn(settingsApi, "get").mockResolvedValue({
    check_interval_minutes: 60,
    notify: true,
    bin_dir: "/tmp/bin",
    apps_dir: "/tmp/apps",
    gh_fallback: false,
  });
  const candidate = { id: sha, sha, title: "Target", prerelease: false };
  vi.spyOn(appsApi, "listCommits").mockResolvedValue({
    repo: "official/tool",
    branch: "main",
    anchor: sha,
    page: 1,
    per_page: 20,
    commits: [candidate],
    has_more: false,
    next_page: null,
  });
  vi.spyOn(appsApi, "revisionOptions").mockResolvedValue({
    reference: sha,
    commit: sha,
    candidate,
    source_build: { method: "source", build: "make", artifact: "out/Tool.app" },
    release_asset: null,
    release_tag: null,
    release_url: null,
    can_download: false,
  });
  vi.spyOn(appsApi, "planVariant").mockResolvedValue({
    app_id: "parent-ccccccc",
    to: candidate,
    up_to_date: false,
    reinstall: false,
    method: "source",
    steps: ["Independent destination"],
  });
  vi.spyOn(appsApi, "installVariant").mockResolvedValue();
  const select = vi.fn();
  const { queryClient } = renderWithProviders(
    <AppPage
      appId="parent"
      tab="commits"
      onTabChange={() => {}}
      onSelectApp={select}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Install options" }),
  );
  fireEvent.click(
    await screen.findByRole("switch", {
      name: "Install as an independent version",
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Start installation" }),
  );
  await screen.findByText(/waiting for its subscription to be registered/);
  expect(select).not.toHaveBeenCalled();
  // When backend registration is observed through the real tracked-list query.
  list = [
    parent,
    {
      ...parent,
      app: { ...parent.app, id: "parent-ccccccc", name: "Independent" },
    },
  ];
  await act(async () => {
    await queryClient.invalidateQueries({ queryKey: ["apps"] });
  });
  // Then navigation occurs once the page can resolve a normal tracked child.
  await waitFor(() => expect(select).toHaveBeenCalledWith("parent-ccccccc"));
});
