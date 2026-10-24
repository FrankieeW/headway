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
  expect(list).toHaveBeenNthCalledWith(2, "tool", 2, 20, anchor);
  expect(list).toHaveBeenNthCalledWith(3, "tool", 1, 20, anchor);
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
  expect(list).toHaveBeenLastCalledWith("tool", 1, 20, undefined);
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
