import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AllCommits } from "./AllCommits";
import { appsApi, type AppView, type CommitPage } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
const anchor = "c".repeat(40);
const view: AppView = {
  app: {
    id: "tool",
    name: "Tool",
    source: "github",
    repo: "official/tool",
    kind: "cli",
    auto_update: false,
    track: { mode: "commit", branch: "main" },
    install: { method: "source", build: "make" },
  },
  status: "up_to_date",
  busy: false,
  pending_count: 0,
};
const page = (number: number): CommitPage => ({
  repo: "official/tool",
  branch: "main",
  anchor,
  page: number,
  per_page: 20,
  commits: [
    {
      id: `${number}`.repeat(40),
      sha: `${number}`.repeat(40),
      title: `Snapshot page ${number}`,
      prerelease: false,
    },
  ],
  has_more: number === 1,
  next_page: number === 1 ? 2 : null,
});
const noFilter = { releases_only: false, tag: null, ci: null };
beforeEach(async () => {
  vi.restoreAllMocks();
  vi.stubGlobal("IntersectionObserver", undefined);
  await setupI18n();
  vi.spyOn(appsApi, "commitMetadata").mockResolvedValue({
    sha: anchor,
    version: null,
    tags: [],
    is_release: null,
    ci_state: "unknown",
    checks_count: 0,
    checked_at: "2026-10-24T08:00:00Z",
    notes: [],
  });
});
afterEach(() => vi.unstubAllGlobals());

it("requests one bounded page and preserves its exact anchor on next and previous", async () => {
  // Given a branch whose tip may move after the first request.
  const list = vi
    .spyOn(appsApi, "listCommits")
    .mockImplementation(async (_id, number, size, pinned) => {
      expect(size).toBe(20);
      if (number > 1) expect(pinned).toBe(anchor);
      return page(number);
    });
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  expect(await screen.findByText("Snapshot page 1")).toBeVisible();
  expect(list).toHaveBeenCalledTimes(1);
  // When the user advances and returns.
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  await screen.findByText("Snapshot page 2");
  fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
  // Then both page changes use the original immutable anchor.
  await screen.findByText("Snapshot page 1");
  expect(list).toHaveBeenNthCalledWith(2, "tool", 2, 20, anchor, noFilter);
  expect(list).toHaveBeenNthCalledWith(3, "tool", 1, 20, anchor, noFilter);
});

it("can return to the prior anchored page after a later page fails", async () => {
  // Given a later-page API failure.
  vi.spyOn(appsApi, "listCommits").mockImplementation(async (_id, number) => {
    if (number === 2)
      throw { code: "github_rate_limited", message: "Quota exhausted" };
    return page(1);
  });
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  await screen.findByText("Snapshot page 1");
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  await screen.findByRole("alert");
  // When navigating back from the error state.
  fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
  // Then the user is not trapped on a failed page.
  expect(await screen.findByText("Snapshot page 1")).toBeVisible();
});

it("refreshes the branch tip only after an explicit restart of browsing", async () => {
  // Given an already loaded stable history snapshot.
  const list = vi.spyOn(appsApi, "listCommits").mockResolvedValue(page(1));
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  await screen.findByText("Snapshot page 1");
  // When explicitly restarting from the latest commit.
  fireEvent.click(
    screen.getByRole("button", { name: "Start from latest commit" }),
  );
  // Then the new first-page request deliberately omits the old anchor.
  await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  expect(list).toHaveBeenLastCalledWith("tool", 1, 20, undefined, noFilter);
});

it("asks only for the commits that published a release", async () => {
  // Given the history browser.
  const list = vi.spyOn(appsApi, "listCommits").mockResolvedValue(page(1));
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  await screen.findByText("Snapshot page 1");
  // When the user narrows the history to published releases.
  fireEvent.click(screen.getByRole("switch", { name: "Only releases" }));
  // Then the request carries the release filter.
  await waitFor(() =>
    expect(list).toHaveBeenLastCalledWith("tool", 1, 20, anchor, {
      releases_only: true,
      tag: null,
      ci: null,
    }),
  );
});

it("shows the release a commit published", async () => {
  // Given a page whose commit published a release with an installable asset.
  const released = page(1);
  vi.spyOn(appsApi, "listCommits").mockResolvedValue({
    ...released,
    commits: [
      {
        ...released.commits[0],
        release_tag: "v1.2.3",
        release_url: "https://example.test/releases/v1.2.3",
        can_download: true,
      },
    ],
  });
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  // Then the tag is visible right away, without a metadata request.
  expect(await screen.findByText("v1.2.3 ↓")).toBeVisible();
  expect(
    screen.getByTitle("Release v1.2.3 has an installable asset"),
  ).toBeVisible();
});

it("applies a tag pattern without dropping the anchor or other filters", async () => {
  // Given page two of an anchored history listing.
  const list = vi
    .spyOn(appsApi, "listCommits")
    .mockImplementation(async (_id, number) => page(number));
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  await screen.findByText("Snapshot page 1");
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  await screen.findByText("Snapshot page 2");
  fireEvent.click(screen.getByRole("switch", { name: "Only releases" }));
  await screen.findByText("Snapshot page 1");
  fireEvent.change(screen.getByRole("textbox", { name: "Tag pattern" }), {
    target: { value: "^v1\\." },
  });

  // When applying a new pattern.
  fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));

  // Then the combined filter starts at page one of the same snapshot.
  await waitFor(() =>
    expect(list).toHaveBeenLastCalledWith("tool", 1, 20, anchor, {
      releases_only: true,
      tag: "^v1\\.",
      ci: null,
    }),
  );
});

it("keeps next-page navigation usable when filtering removes every row", async () => {
  // Given a filtered page with more history and incomplete metadata.
  vi.spyOn(appsApi, "listCommits").mockResolvedValue({
    ...page(1),
    commits: [],
    filtered_out: 20,
    notes: ["Release metadata unavailable"],
  });
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  await screen.findByText("Release metadata unavailable");

  // When enabling release filtering.
  fireEvent.click(screen.getByRole("switch", { name: "Only releases" }));

  // Then no-match feedback and a usable next-page action remain visible.
  expect(await screen.findByText(/No matches on this page/)).toBeVisible();
  expect(screen.getByRole("button", { name: "Next page" })).toBeEnabled();
});

it("resolves arbitrary references without triggering an installation", async () => {
  // Given the history browser and an arbitrary user-entered ref.
  vi.spyOn(appsApi, "listCommits").mockResolvedValue(page(1));
  const choose = vi.fn();
  const install = vi.spyOn(appsApi, "installRevision");
  renderWithProviders(<AllCommits view={view} onChoose={choose} />);
  // When asking to resolve a commit, tag or branch.
  fireEvent.change(
    screen.getByRole("textbox", { name: "Commit, tag or branch" }),
    { target: { value: " old-tag " } },
  );
  fireEvent.click(screen.getByRole("button", { name: "Resolve revision" }));
  // Then only the explicit preview route opens.
  expect(choose).toHaveBeenCalledWith("old-tag");
  expect(install).not.toHaveBeenCalled();
});

it("recovers from an invalid tag pattern by clearing it on the same anchored page", async () => {
  // Given an otherwise readable history whose tag filter is invalid.
  const list = vi
    .spyOn(appsApi, "listCommits")
    .mockImplementation(async (_id, number, _size, _anchor, filter) => {
      if (filter?.tag === "[")
        throw { code: "invalid_input", message: "Invalid tag pattern" };
      return page(number);
    });
  renderWithProviders(<AllCommits view={view} onChoose={() => {}} />);
  await screen.findByText("Snapshot page 1");
  fireEvent.change(screen.getByRole("textbox", { name: "Tag pattern" }), {
    target: { value: "[" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
  await screen.findByRole("alert");
  expect(
    screen.queryByRole("button", { name: "Retry" }),
  ).not.toBeInTheDocument();

  // When clearing the invalid filter using the adjacent recovery action.
  fireEvent.click(screen.getByRole("button", { name: "Clear tag filter" }));

  // Then history is readable again without re-resolving a moving branch tip.
  expect(await screen.findByText("Snapshot page 1")).toBeVisible();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "Tag pattern" })).toHaveValue("");
  expect(list).toHaveBeenLastCalledWith("tool", 1, 20, anchor, noFilter);
});
