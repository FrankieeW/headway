import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  appsApi,
  type AppView,
  type CommitMetadata,
  type TrackedApp,
} from "@/lib/api";
import {
  createTestQueryClient,
  renderWithProviders,
  setupI18n,
} from "@/test/renderWithProviders";
import { CommitBadges } from "./CommitBadges";
import { PendingList } from "./PendingList";

const sha = "a".repeat(40);
const metadata: CommitMetadata = {
  sha,
  version: "v1.2.3",
  tags: ["v1.2.3"],
  is_release: true,
  ci_state: "success",
  checks_count: 2,
  checked_at: "2026-10-24T08:00:00Z",
  notes: [],
};
beforeEach(async () => {
  vi.restoreAllMocks();
  vi.stubGlobal("IntersectionObserver", undefined);
  await setupI18n();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("loads at most the first five commit rows and requires an explicit request for later rows", async () => {
  // Given twelve visible GitHub commit rows.
  const fetch = vi
    .spyOn(appsApi, "commitMetadata")
    .mockImplementation(async (_id, target) => ({ ...metadata, sha: target }));
  const app: AppView = {
    app: {
      id: "tool",
      name: "Tool",
      kind: "cli",
      source: "github",
      repo: "owner/tool",
      track: { mode: "commit" },
      install: { method: "source", build: "make" },
      auto_update: false,
    },
    status: "update_available",
    busy: false,
    pending_count: 12,
    last_check: {
      status: "update_available",
      checked_at: metadata.checked_at,
      pending_total: 12,
      pending: Array.from({ length: 12 }, (_, i) => ({
        id: (i + 1).toString(16).padStart(40, "0"),
        sha: (i + 1).toString(16).padStart(40, "0"),
        title: `Commit ${i + 1}`,
        prerelease: false,
      })),
    },
  };
  renderWithProviders(
    <PendingList
      app={app}
      installing={false}
      onCheck={() => {}}
      onInstall={() => {}}
      onMarkInstalled={() => {}}
    />,
  );
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(5));
  const refresh = screen.getAllByRole("button", {
    name: /^Refresh commit metadata/,
  })[7];
  if (!refresh) throw new Error("Eighth row missing");
  // When explicitly requesting metadata for a later row.
  fireEvent.click(refresh);
  // Then exactly that SHA is fetched, without fetching the entire backlog.
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(6));
  expect(fetch).toHaveBeenLastCalledWith("tool", "8".padStart(40, "0"), true);
});

it("does not request metadata before a row intersects the visible area", async () => {
  // Given a row outside the viewport.
  let observe: ((entries: { isIntersecting: boolean }[]) => void) | undefined;
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: (entries: { isIntersecting: boolean }[]) => void) {
        observe = callback;
      }
      observe() {}
      disconnect() {}
    },
  );
  const fetch = vi.spyOn(appsApi, "commitMetadata").mockResolvedValue(metadata);
  renderWithProviders(
    <CommitBadges appId="tool" repo="owner/tool" sha={sha} autoLoad />,
  );
  expect(fetch).not.toHaveBeenCalled();
  // When it becomes visible.
  act(() => observe?.([{ isIntersecting: true }]));
  // Then the exact commit is requested.
  await waitFor(() => expect(fetch).toHaveBeenCalledWith("tool", sha, false));
});

it("reuses cached metadata when a row remounts", async () => {
  // Given a cached immutable commit result.
  const client = createTestQueryClient();
  client.setQueryDefaults(["commit-metadata"], { gcTime: Infinity });
  client.setQueryData(["commit-metadata", "tool", "owner/tool", sha], metadata);
  const fetch = vi.spyOn(appsApi, "commitMetadata").mockResolvedValue(metadata);
  // When mounting its row again.
  renderWithProviders(
    <CommitBadges appId="tool" repo="owner/tool" sha={sha} autoLoad />,
    { queryClient: client },
  );
  // Then cache is displayed with no quota-consuming request.
  expect(await screen.findByText("Version v1.2.3")).toBeVisible();
  expect(fetch).not.toHaveBeenCalled();
});

it("refreshes pending CI after 30 seconds and stops polling on success", async () => {
  // Given a visible pending check followed by a final successful result.
  vi.useFakeTimers();
  const fetch = vi
    .spyOn(appsApi, "commitMetadata")
    .mockResolvedValueOnce({ ...metadata, ci_state: "pending" })
    .mockResolvedValue(metadata);
  renderWithProviders(
    <CommitBadges appId="tool" repo="owner/tool" sha={sha} autoLoad />,
  );
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
  expect(fetch).toHaveBeenCalledTimes(1);
  // When the polling interval elapses.
  await act(async () => {
    await vi.advanceTimersByTimeAsync(30_000);
  });
  expect(fetch).toHaveBeenLastCalledWith("tool", sha, false);
  expect(fetch).toHaveBeenCalledTimes(2);
  // Then final CI state no longer polls.
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(fetch).toHaveBeenCalledTimes(2);
});

it.each(["unknown", "not_run"] as const)(
  "distinguishes %s CI without inventing a version or release",
  async (ci_state) => {
    // Given independently unavailable version/release metadata and a specific CI state.
    vi.spyOn(appsApi, "commitMetadata").mockResolvedValue({
      ...metadata,
      version: null,
      tags: [],
      is_release: ci_state === "unknown" ? null : false,
      ci_state,
    });
    // When rendering metadata returned for this SHA.
    renderWithProviders(
      <CommitBadges appId="tool" repo="owner/tool" sha={sha} autoLoad />,
    );
    // Then each unknown/absent state is represented independently.
    expect(
      await screen.findByText(
        ci_state === "unknown" ? "CI unknown" : "CI not run",
      ),
    ).toBeVisible();
    expect(screen.getByText("Version —")).toBeVisible();
    expect(
      screen.getByText(
        ci_state === "unknown" ? "Release unknown" : "Not a release",
      ),
    ).toBeVisible();
  },
);

it("renders tag-derived versions as literal text", async () => {
  // Given an untrusted version string returned for the commit.
  vi.spyOn(appsApi, "commitMetadata").mockResolvedValue({
    ...metadata,
    version: "<img src=x onerror=alert(1)>",
  });
  // When rendering its version badge.
  const { container } = renderWithProviders(
    <CommitBadges appId="tool" repo="owner/tool" sha={sha} autoLoad />,
  );
  // Then markup is visible text and creates no DOM image.
  expect(
    await screen.findByText("Version <img src=x onerror=alert(1)>"),
  ).toBeVisible();
  expect(container.querySelector("img")).toBeNull();
});

it.each(["npm", "release", "tag"] as const)(
  "does not fetch or render badges for %s rows",
  (mode) => {
    // Given a non-commit source/track containing a SHA-like candidate.
    const common = {
      id: "tool",
      name: "Tool",
      kind: "cli" as const,
      auto_update: false,
    };
    const app: TrackedApp =
      mode === "npm"
        ? { ...common, source: "npm", package: "tool", dist_tag: "latest" }
        : {
            ...common,
            source: "github",
            repo: "owner/tool",
            track: { mode, prerelease: false },
            install: { method: "source", build: "make" },
          };
    const fetch = vi.spyOn(appsApi, "commitMetadata");
    const view: AppView = {
      app,
      status: "update_available",
      busy: false,
      pending_count: 1,
      last_check: {
        status: "update_available",
        checked_at: metadata.checked_at,
        pending_total: 1,
        pending: [{ id: sha, sha, title: "Candidate", prerelease: false }],
      },
    };
    // When the pending list renders.
    renderWithProviders(
      <PendingList
        app={view}
        installing={false}
        onCheck={() => {}}
        onInstall={() => {}}
        onMarkInstalled={() => {}}
      />,
    );
    // Then commit-only metadata neither appears nor consumes a request.
    expect(screen.queryByLabelText("Commit metadata")).toBeNull();
    expect(fetch).not.toHaveBeenCalled();
  },
);

it("stops pending CI polling when its row leaves the visible area", async () => {
  // Given pending CI on a visible commit row.
  vi.useFakeTimers();
  let observe: ((entries: { isIntersecting: boolean }[]) => void) | undefined;
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: (entries: { isIntersecting: boolean }[]) => void) {
        observe = callback;
      }
      observe() {}
      disconnect() {}
    },
  );
  const fetch = vi
    .spyOn(appsApi, "commitMetadata")
    .mockResolvedValue({ ...metadata, ci_state: "pending" });
  renderWithProviders(
    <CommitBadges appId="tool" repo="owner/tool" sha={sha} autoLoad />,
  );
  act(() => observe?.([{ isIntersecting: true }]));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
  expect(fetch).toHaveBeenCalledTimes(1);
  // When scrolling the row out of view.
  act(() => observe?.([{ isIntersecting: false }]));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  // Then pending CI consumes no background polling quota.
  expect(fetch).toHaveBeenCalledTimes(1);
});
