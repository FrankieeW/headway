import { beforeEach, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import type { AppView, TrackedApp } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { AppSummary } from "./AppSummary";
import { PendingList } from "./PendingList";
import { HistoryList } from "./HistoryList";
import { StatusBadge } from "./StatusBadge";
import { supportsRefs } from "@/lib/sources";

beforeEach(setupI18n);
const common = {
  id: "tool",
  name: "Tool",
  kind: "cli",
  auto_update: false,
} as const;
const sources: TrackedApp[] = [
  {
    ...common,
    source: "github",
    repo: "owner/tool",
    track: { mode: "commit" },
    install: { method: "source", build: "make" },
  },
  { ...common, source: "npm", package: "tool", dist_tag: "latest" },
  { ...common, source: "brew", package: "tool", cask: false },
  { ...common, source: "script", update_command: "tool update" },
];
it.each(sources)("exposes only supported operations for $source", (app) => {
  // Given pending changes and successful history for each source.
  const view: AppView = {
    app,
    busy: false,
    status: "update_available",
    pending_count: 2,
    last_check: {
      checked_at: new Date().toISOString(),
      status: "update_available",
      pending: [{ id: "2.0", title: "2.0", prerelease: false }],
      pending_total: 2,
    },
  };
  // When rendering the action surfaces.
  renderWithProviders(
    <>
      <AppSummary
        app={view}
        run={undefined}
        starting={false}
        cancelling={false}
        onUpdateLatest={vi.fn()}
        onUpdateNext={vi.fn()}
        onCancel={vi.fn()}
        onToggleAutoUpdate={vi.fn(async () => undefined)}
        updateMethod="configured"
        onUpdateMethodChange={vi.fn()}
      />
      <PendingList
        app={view}
        installing={false}
        onInstall={vi.fn()}
        onMarkInstalled={vi.fn()}
        onCheck={vi.fn()}
      />
      <HistoryList
        entries={[
          {
            at: new Date().toISOString(),
            action: "update",
            to: "1.0",
            ok: true,
          },
        ]}
        loading={false}
        canReinstall={supportsRefs(app)}
        onReinstall={vi.fn()}
      />
    </>,
  );
  // Then unsupported targeting and marking controls are absent.
  const refs = app.source === "github" || app.source === "npm";
  expect(Boolean(screen.queryByRole("button", { name: /^Next / }))).toBe(refs);
  expect(Boolean(screen.queryByRole("button", { name: /^Reinstall/ }))).toBe(
    refs,
  );
  expect(Boolean(screen.queryByRole("button", { name: "Install" }))).toBe(refs);
  expect(
    Boolean(screen.queryByRole("button", { name: "Mark installed" })),
  ).toBe(app.source === "github");
});
it("renders unknown as visible status rather than an error or unchecked dash", () => {
  // Given a script without a latest probe, when rendering its badge.
  renderWithProviders(<StatusBadge status="unknown" variant="full" />);
  // Then the visible state accurately reports uncertainty.
  expect(screen.getByText("Unknown")).toBeVisible();
  expect(screen.queryByText("Error")).not.toBeInTheDocument();
});

it.each(["github", "script"] as const)(
  "preserves the distinct meaning of unknown for %s",
  (source) => {
    // Given a backend unknown status with no recorded installed reference.
    const app = sources.find((entry) => entry.source === source);
    if (!app) throw new Error("Source fixture missing");
    const view: AppView = {
      app,
      installed: null,
      busy: false,
      status: "unknown",
      pending_count: 0,
    };
    // When the summary and pending list render the backend result.
    renderWithProviders(
      <>
        <AppSummary
          app={view}
          run={undefined}
          starting={false}
          cancelling={false}
          onUpdateLatest={vi.fn()}
          onUpdateNext={vi.fn()}
          onCancel={vi.fn()}
          onToggleAutoUpdate={vi.fn(async () => undefined)}
          updateMethod="configured"
          onUpdateMethodChange={vi.fn()}
        />
        <PendingList
          app={view}
          installing={false}
          onInstall={vi.fn()}
          onMarkInstalled={vi.fn()}
          onCheck={vi.fn()}
        />
      </>,
    );
    // Then GitHub acknowledges detected presence without inventing a ref; scripts keep probe semantics.
    if (source === "github") {
      expect(screen.getByText("Local installation found")).toBeVisible();
      expect(
        screen.getAllByText(
          "Installation found; its version or commit is not confirmed yet.",
        ),
      ).toHaveLength(2);
      expect(screen.queryByText("Not installed yet")).toBeNull();
    } else {
      expect(screen.queryByText("Local installation found")).toBeNull();
      expect(
        screen.queryByText(
          "Installation found; its version or commit is not confirmed yet.",
        ),
      ).toBeNull();
    }
  },
);
