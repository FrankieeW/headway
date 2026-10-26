import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AppPage } from "./AppPage";
import { appsApi, opsApi, type AppView } from "@/lib/api";
import { clearRun } from "@/lib/updateRuns";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";

const candidate = {
  id: "b".repeat(40),
  title: "Pending commit",
  prerelease: false,
};
const view: AppView = {
  app: {
    id: "tool",
    name: "Tool",
    source: "github",
    repo: "owner/tool",
    kind: "cli",
    track: { mode: "commit" },
    install: { method: "source", build: "make" },
    auto_update: false,
  },
  status: "update_available",
  busy: false,
  pending_count: 2,
  last_check: {
    checked_at: "2026-10-26T00:00:00Z",
    status: "update_available",
    latest: candidate,
    pending: [candidate],
    pending_total: 2,
  },
};
const scrollIntoView = HTMLElement.prototype.scrollIntoView;
beforeEach(async () => {
  HTMLElement.prototype.scrollIntoView = () => {};
  vi.restoreAllMocks();
  await setupI18n();
  vi.spyOn(appsApi, "list").mockResolvedValue([view]);
  vi.spyOn(appsApi, "get").mockResolvedValue(view);
  vi.spyOn(appsApi, "history").mockResolvedValue([
    { at: "2026-10-25T00:00:00Z", action: "update", to: "old", ok: true },
  ]);
  vi.spyOn(appsApi, "presets").mockResolvedValue([]);
});
afterEach(() => {
  HTMLElement.prototype.scrollIntoView = scrollIntoView;
  clearRun("tool");
});

it.each([
  ["changes", "Update to latest", "latest"],
  ["changes", "Next commit", "next"],
  ["changes", "Install", { ref: candidate.id }],
  ["history", "Reinstall", { ref: "old" }],
] as const)(
  "passes the one-off method from %s / %s to the real API adapter",
  async (tab, label, target) => {
    // Given the real app page with a selected one-off release method.
    const start = vi.spyOn(opsApi, "startUpdate").mockResolvedValue();
    const save = vi.spyOn(appsApi, "save");
    renderWithProviders(
      <AppPage appId="tool" tab={tab} onTabChange={() => {}} />,
    );
    const selector = await screen.findByRole("combobox", {
      name: "Install from GitHub",
    });
    fireEvent.keyDown(selector, { key: "ArrowDown" });
    fireEvent.click(
      await screen.findByRole("option", { name: "Download the release" }),
    );

    // When starting this update through its visible action.
    fireEvent.click(screen.getByRole("button", { name: label }));

    // Then every entry point uses the selected method and resets it without saving configuration.
    await waitFor(() =>
      expect(start).toHaveBeenCalledWith("tool", target, "asset"),
    );
    expect(save).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(selector).toHaveTextContent("Build from source"),
    );
  },
);
