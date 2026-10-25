import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AppView } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { AppSummary } from "./AppSummary";

function view(autoUpdate: boolean): AppView {
  return {
    app: {
      source: "github",
      id: "tool",
      name: "Tool",
      repo: "owner/tool",
      kind: "cli",
      auto_update: autoUpdate,
      track: { mode: "release", prerelease: false },
      install: { method: "asset", binary: "tool" },
    },
    installed: {
      id: "1.0.0",
      version: "1.0.0",
      installed_at: new Date().toISOString(),
    },
    last_check: {
      checked_at: new Date().toISOString(),
      status: "up_to_date",
      latest: { id: "1.0.0", title: "1.0.0", prerelease: false },
      pending: [],
      pending_total: 0,
    },
    status: "up_to_date",
    pending_count: 0,
    busy: false,
  };
}

function render(app: AppView, onToggleAutoUpdate: (value: boolean) => Promise<unknown>) {
  return renderWithProviders(
    <AppSummary
      app={app}
      run={undefined}
      starting={false}
      cancelling={false}
      onUpdateLatest={() => {}}
      onUpdateNext={() => {}}
      onCancel={() => {}}
      onToggleAutoUpdate={onToggleAutoUpdate}
    />,
  );
}

describe("auto-update switch", () => {
  beforeEach(async () => {
    vi.restoreAllMocks();
    await setupI18n();
  });

  it("rolls the switch back when saving the preference fails", async () => {
    // Given a rejected save (backend refused the config write).
    const save = vi.fn(async () => {
      throw { code: "invalid_config", message: "cannot write config" };
    });
    render(view(false), save);

    const toggle = screen.getByRole("switch", {
      name: "Install updates automatically",
    });
    expect(toggle).toHaveAttribute("data-state", "unchecked");

    // When the user turns it on.
    fireEvent.click(toggle);

    // Then the optimistic state appears immediately…
    await waitFor(() => expect(save).toHaveBeenCalledWith(true));
    // …and is rolled back after the rejection instead of lying about the
    // persisted value.
    await waitFor(() => expect(toggle).toHaveAttribute("data-state", "unchecked"));
    expect(toggle).not.toBeDisabled();
  });

  it("keeps the switch on after a successful save", async () => {
    const save = vi.fn(async () => ({ ...view(true).app }));
    render(view(false), save);

    const toggle = screen.getByRole("switch", {
      name: "Install updates automatically",
    });
    fireEvent.click(toggle);

    await waitFor(() => expect(save).toHaveBeenCalledOnce());
    await waitFor(() => expect(toggle).toHaveAttribute("data-state", "checked"));
  });
});
