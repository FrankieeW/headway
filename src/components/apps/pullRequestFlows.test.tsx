import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PullRequests } from "./PullRequests";
import { RevisionDialog } from "./RevisionDialog";
import {
  appsApi,
  type AppView,
  type PullRequest,
  type RevisionOptions,
} from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { clearRun } from "@/lib/updateRuns";

const sha = "c".repeat(40);
const view: AppView = {
  app: {
    id: "tool",
    name: "Tool",
    kind: "cli",
    source: "github",
    repo: "owner/tool",
    auto_update: false,
    track: { mode: "commit" },
    install: { method: "source", build: "make", artifact: "tool" },
  },
  status: "unchecked",
  busy: false,
  pending_count: 0,
};
const pull: PullRequest = {
  number: 7,
  title: "PR contribution",
  state: "open",
  html_url: "https://github.com/owner/tool/pull/7",
  draft: false,
  head: { ref: "feature", sha, repo: { full_name: "fork/tool" } },
  base: { ref: "main", sha: "a".repeat(40), repo: { full_name: "owner/tool" } },
};
const options: RevisionOptions = {
  reference: `pr:7@${sha}`,
  commit: sha,
  candidate: { id: sha, sha, title: pull.title, prerelease: false },
  release_tag: null,
  release_url: null,
  can_download: false,
  source_build: { method: "source", build: "make", artifact: "tool" },
  release_asset: null,
  pull_request: pull,
};
beforeEach(async () => {
  await setupI18n();
});
afterEach(() => {
  vi.restoreAllMocks();
  clearRun("tool-ccccccc");
});

it("selects the exact PR head shown in the list without installing", async () => {
  // Given a fetched PR with a known head.
  vi.spyOn(appsApi, "listPullRequests").mockResolvedValue({
    repo: "owner/tool",
    page: 1,
    per_page: 20,
    has_more: false,
    pull_requests: [pull],
  });
  const choose = vi.fn();
  renderWithProviders(
    <PullRequests view={view} onChoose={choose} onBack={() => {}} />,
  );
  // When choosing its installation options.
  fireEvent.click(
    await screen.findByRole("button", { name: "Install options" }),
  );
  // Then the request contains the immutable head shown to the user.
  expect(choose).toHaveBeenCalledWith(`pr:7@${sha}`);
});

it("keeps the PR pin through review and confirmation for a separate installation", async () => {
  // Given resolved PR options and an independent destination.
  vi.spyOn(appsApi, "revisionOptions").mockResolvedValue(options);
  const plan = vi
    .spyOn(appsApi, "planVariant")
    .mockResolvedValue({
      app_id: "tool-ccccccc",
      to: options.candidate,
      up_to_date: false,
      reinstall: false,
      method: "source",
      steps: ["Install into separate binary"],
    });
  const install = vi.spyOn(appsApi, "installVariant").mockResolvedValue();
  renderWithProviders(
    <RevisionDialog
      view={view}
      reference="pr:7"
      onClose={() => {}}
      onVariant={() => {}}
    />,
  );
  await screen.findByRole("button", { name: "Review installation" });
  expect(
    await screen.findByRole("button", { name: "Download release asset" }),
  ).toBeDisabled();
  // When reviewing and confirming the default separate PR installation.
  fireEvent.click(screen.getByRole("button", { name: "Review installation" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Start installation" }),
  );
  // Then both stages retain the PR identity and head pin.
  await waitFor(() => expect(install).toHaveBeenCalled());
  expect(plan.mock.calls[0]?.[1].revision.reference).toBe(`pr:7@${sha}`);
  expect(install.mock.calls[0]?.[1].revision.reference).toBe(`pr:7@${sha}`);
});
