import { screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { appsApi, type AppView, type CommitMetadata } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { AppSummary } from "./AppSummary";
import { PendingList } from "./PendingList";
const sha = "a".repeat(40);
const metadata: CommitMetadata = {
  sha,
  version: "0.2.1",
  version_source: "manifest",
  tags: [],
  is_release: false,
  ci_state: "success",
  checks_count: 3,
  checked_at: "2026-10-24T08:00:00Z",
  notes: [],
};
const view: AppView = {
  app: {
    id: "headway",
    name: "Headway",
    kind: "app",
    source: "github",
    repo: "FrankieeW/headway",
    track: { mode: "commit" },
    install: {
      method: "source",
      build: "./scripts/build-headway.sh app",
      artifact: ".headway-out/Headway.app",
    },
    auto_update: false,
  },
  installed: {
    id: sha,
    sha,
    version: "0.2.1",
    installed_at: metadata.checked_at,
  },
  status: "up_to_date",
  pending_count: 0,
  busy: false,
};
const summary = (app: AppView) => (
  <AppSummary
    app={app}
    run={undefined}
    starting={false}
    cancelling={false}
    onUpdateLatest={() => {}}
    onUpdateNext={() => {}}
    onCancel={() => {}}
    onToggleAutoUpdate={() => {}}
  />
);
beforeEach(async () => {
  vi.restoreAllMocks();
  vi.stubGlobal("IntersectionObserver", undefined);
  await setupI18n();
});
afterEach(() => vi.unstubAllGlobals());

it("shows manifest-version badges on an up-to-date installed commit with no pending changes", async () => {
  // Given an installed exact SHA and no pending rows.
  const fetch = vi.spyOn(appsApi, "commitMetadata").mockResolvedValue(metadata);
  // When rendering its installed summary.
  renderWithProviders(summary(view));
  // Then the installed SHA is queried and a manifest version does not become a release.
  expect(
    await screen.findByLabelText("Version 0.2.1 · Source version"),
  ).toBeVisible();
  expect(fetch).toHaveBeenCalledWith("headway", sha, false);
  expect(screen.getByText("CI passed")).toBeVisible();
  expect(screen.getByText("Not a release")).toBeVisible();
  expect(screen.queryByText("Release")).toBeNull();
});

it.each([false, true])(
  "bounds the combined installed/pending query graph with duplicate=%s",
  async (duplicate) => {
    // Given an installed commit and eight pending commits, optionally sharing its SHA.
    const pending = Array.from({ length: 8 }, (_, i) => ({
      id: duplicate && i === 0 ? sha : (i + 1).toString(16).padStart(40, "0"),
      title: `Commit ${i}`,
      prerelease: false,
    }));
    const app: AppView = {
      ...view,
      status: "update_available",
      pending_count: pending.length,
      last_check: {
        checked_at: metadata.checked_at,
        status: "update_available",
        pending,
        pending_total: pending.length,
      },
    };
    const fetch = vi
      .spyOn(appsApi, "commitMetadata")
      .mockImplementation(async (_id, target) => ({
        ...metadata,
        sha: target,
      }));
    // When both installed and pending surfaces become visible together.
    renderWithProviders(
      <>
        {summary(app)}
        <PendingList
          app={app}
          installing={false}
          onCheck={() => {}}
          onInstall={() => {}}
          onMarkInstalled={() => {}}
        />
      </>,
    );
    // Then there are at most five automatic requests and shared SHAs reuse the query.
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(duplicate ? 4 : 5));
    expect(fetch.mock.calls.filter((call) => call[1] === sha)).toHaveLength(1);
  },
);
