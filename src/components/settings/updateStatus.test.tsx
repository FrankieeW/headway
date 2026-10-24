import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { SelfUpdateStatus } from "./SelfUpdateStatus";
import { CliSection } from "./CliSection";
import {
  appsApi,
  settingsApi,
  systemApi,
  type SelfUpdateStatus as Status,
} from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
});
const status: Status = {
  running: { version: "1", commit: "old-sha", dirty: false, target: null },
  installed: { version: "2", commit: "new-sha", dirty: false, target: null },
  pending_restart: true,
  restart_requested: false,
  confirmed: false,
};
it("queues restart while jobs are busy without claiming the new build is running", async () => {
  // Given different running/installed builds and an active job.
  vi.spyOn(systemApi, "selfUpdateStatus").mockResolvedValue(status);
  vi.spyOn(appsApi, "list").mockResolvedValue([
    {
      app: {
        source: "npm",
        package: "tool",
        dist_tag: "latest",
        id: "tool",
        name: "Tool",
        kind: "cli",
        auto_update: false,
      },
      busy: true,
      status: "busy",
      pending_count: 0,
    },
  ]);
  const restart = vi
    .spyOn(systemApi, "requestSelfRestart")
    .mockResolvedValue({ ...status, restart_requested: true });
  renderWithProviders(<SelfUpdateStatus />);
  // When the user requests a queued restart.
  fireEvent.click(await screen.findByRole("button", { name: "Queue restart" }));
  // Then a request is recorded, while the running commit remains unchanged.
  await waitFor(() => expect(restart).toHaveBeenCalledOnce());
  expect(screen.getByText("1 · old-sha")).toBeVisible();
  expect(screen.getByText("2 · new-sha")).toBeVisible();
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Queue restart" }),
    ).toBeDisabled(),
  );
});
it("repairs the owned link while retaining the PATH shadowing warning", async () => {
  // Given a moved application link and a Homebrew command first on PATH.
  const cli = {
    link_path: "/local/bin/headway",
    installed: false,
    on_path: true,
    bundled_path: "/Applications/Headway.app/headway",
    needs_repair: true,
    shadowed: true,
    resolved_path: "/opt/homebrew/bin/headway",
  };
  vi.spyOn(settingsApi, "cliStatus").mockResolvedValue(cli);
  const repair = vi.spyOn(settingsApi, "fixCliLink").mockResolvedValue({
    ...cli,
    installed: true,
    needs_repair: false,
    points_to: cli.bundled_path,
  });
  renderWithProviders(<CliSection />);
  // When the owned link is repaired.
  fireEvent.click(await screen.findByRole("button", { name: "Fix CLI link" }));
  // Then status updates without presenting PATH resolution as repaired.
  await waitFor(() => expect(repair).toHaveBeenCalledOnce());
  expect(screen.getByRole("status")).toHaveTextContent(
    "/opt/homebrew/bin/headway",
  );
  expect(
    await screen.findByRole("button", { name: "Reinstall" }),
  ).toBeVisible();
});

it("allows retry after failed restart confirmation instead of showing a permanent queue", async () => {
  // Given a terminal backend restart mismatch after rollback clears pending_restart.
  vi.spyOn(appsApi, "list").mockResolvedValue([]);
  vi.spyOn(systemApi, "selfUpdateStatus").mockResolvedValue({
    ...status,
    pending_restart: false,
    restart_requested: false,
    restart_error: "Expected installed commit does not match running commit.",
  });
  const retry = vi.spyOn(systemApi, "requestSelfRestart").mockResolvedValue({
    ...status,
    restart_requested: true,
    restart_error: null,
  });
  renderWithProviders(<SelfUpdateStatus />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Expected installed commit",
  );
  const button = await screen.findByRole("button", { name: "Retry" });
  expect(button).toBeEnabled();
  // When the user retries the explicit restart request.
  fireEvent.click(button);
  // Then the cleared error returns to the ordinary queued state.
  await waitFor(() => expect(retry).toHaveBeenCalledOnce());
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(
    screen.getByRole("button", { name: "Restart Headway" }),
  ).toBeDisabled();
});
