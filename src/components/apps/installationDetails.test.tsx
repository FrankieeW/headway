import { screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { InstallationDetails } from "./InstallationDetails";
import { appsApi, type AppView, type InstallationDetection } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";

function view(installation: InstallationDetection): AppView {
  return {
    app: {
      id: "tool",
      name: "Tool",
      kind: "app",
      source: "github",
      repo: "owner/tool",
      auto_update: false,
      track: { mode: "commit" },
      install: { method: "source", build: "make", dest: installation.path },
    },
    status: "unknown",
    busy: false,
    pending_count: 0,
    installation,
  };
}

beforeEach(setupI18n);
afterEach(() => vi.restoreAllMocks());

it("shows the new installation path when the destination changes without a status change", async () => {
  // Given a settled detection for the previous destination.
  const original: InstallationDetection = {
    presence: "present",
    path: "/Applications/Old.app",
    baseline_matches: false,
  };
  const changed = { ...original, path: "/Applications/New.app" };
  const detect = vi
    .spyOn(appsApi, "detectInstallation")
    .mockResolvedValueOnce(original)
    .mockResolvedValue(changed);
  const { rerender, queryClient } = renderWithProviders(
    <InstallationDetails view={view(original)} />,
  );
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(detect).toHaveBeenCalledTimes(1);
  // When a refreshed app view reports a different configured destination.
  rerender(<InstallationDetails view={view(changed)} />);
  // Then the new location replaces the old one without a manual refresh.
  expect(await screen.findByText(changed.path)).toBeVisible();
  expect(screen.queryByText(original.path ?? "")).toBeNull();
});

it("replaces invalid payload evidence when a refreshed view reports a repaired installation", async () => {
  // Given settled invalid evidence with an unknown version.
  const original: InstallationDetection = {
    presence: "invalid",
    path: "/Applications/Tool.app",
    baseline_matches: false,
  };
  const repaired: InstallationDetection = { ...original, presence: "present" };
  vi.spyOn(appsApi, "detectInstallation")
    .mockResolvedValueOnce(original)
    .mockResolvedValue(repaired);
  const { rerender, queryClient } = renderWithProviders(
    <InstallationDetails view={view(original)} />,
  );
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(
    screen.getByText("Unexpected payload type or executable permissions"),
  ).toBeVisible();
  // When new evidence reports a valid payload at the same path and status.
  rerender(<InstallationDetails view={view(repaired)} />);
  // Then stale invalid evidence no longer obscures the repaired installation.
  expect(await screen.findByText("Installed payload found")).toBeVisible();
  expect(
    screen.queryByText("Unexpected payload type or executable permissions"),
  ).toBeNull();
});
