import { beforeEach, describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";

import { StatusBadge } from "@/components/apps/StatusBadge";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";

describe("StatusBadge", () => {
  beforeEach(async () => {
    await setupI18n();
  });

  it("shows the number of pending updates in the sidebar", () => {
    renderWithProviders(
      <StatusBadge status="update_available" pendingCount={3} variant="compact" />,
    );

    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.queryByText("3 new")).not.toBeInTheDocument();
  });

  it("shows the number of pending updates in the detail header", () => {
    renderWithProviders(
      <StatusBadge status="update_available" pendingCount={3} variant="full" />,
    );

    expect(screen.getByText("3 new")).toBeInTheDocument();
  });

  it("marks an up to date app", () => {
    renderWithProviders(<StatusBadge status="up_to_date" variant="full" />);

    expect(screen.getByText("Up to date")).toBeInTheDocument();
  });

  it("marks an app that is not installed", () => {
    renderWithProviders(<StatusBadge status="not_installed" variant="full" />);

    expect(screen.getByText("Not installed")).toBeInTheDocument();
  });

  it("marks a diverged app", () => {
    renderWithProviders(<StatusBadge status="diverged" variant="full" />);

    expect(screen.getByText("Diverged")).toBeInTheDocument();
  });

  it("marks a failed app", () => {
    renderWithProviders(<StatusBadge status="error" variant="full" />);

    expect(screen.getByText("Error")).toBeInTheDocument();
  });

  it("marks a busy app", () => {
    renderWithProviders(<StatusBadge status="busy" variant="full" />);

    expect(screen.getByText("Working…")).toBeInTheDocument();
  });

  it("marks an unchecked app", () => {
    renderWithProviders(<StatusBadge status="unchecked" variant="full" />);

    expect(screen.getByText("—")).toBeInTheDocument();
  });
});
