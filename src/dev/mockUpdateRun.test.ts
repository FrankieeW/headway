import type { AppView } from "@/lib/api";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { startMockUpdate, type MockEmitter } from "./mockUpdateRun";

function view(): AppView {
  return {
    app: {
      source: "github",
      id: "tool",
      name: "Tool",
      repo: "owner/tool",
      kind: "cli",
      auto_update: false,
      track: { mode: "commit", branch: "main" },
      install: { method: "source", build: "make", artifact: "tool" },
    },
    installed: {
      id: "a".repeat(40),
      sha: "a".repeat(40),
      version: "1.0.0",
      installed_at: new Date().toISOString(),
      path: "/tmp/bin/tool",
    },
    last_check: {
      checked_at: new Date().toISOString(),
      status: "update_available",
      latest: {
        id: "b".repeat(40),
        sha: "b".repeat(40),
        title: "New version",
        prerelease: false,
      },
      pending: [
        {
          id: "b".repeat(40),
          sha: "b".repeat(40),
          title: "New version",
          prerelease: false,
        },
      ],
      pending_total: 1,
    },
    busy: false,
    status: "update_available",
    pending_count: 1,
  };
}

describe("mock update run", () => {
  let events: { name: string; payload: unknown }[];
  let emitter: MockEmitter;

  beforeEach(() => {
    vi.useFakeTimers();
    events = [];
    emitter = { emit: (name, payload) => events.push({ name, payload }) };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("streams stage/log/progress events and finishes with an outcome", () => {
    const app = view();
    const finished = vi.fn();
    startMockUpdate(app, "latest", { emitter, onFinished: finished });

    expect(app.busy).toBe(true);
    expect(app.status).toBe("busy");

    vi.advanceTimersByTime(1000);

    const names = events.map((event) => event.name);
    expect(names).toContain("headway://update-event");
    expect(names).toContain("headway://apps-changed");
    expect(names[names.length - 1]).toBe("headway://update-finished");

    const types = events
      .filter((event) => event.name === "headway://update-event")
      .map((event) => (event.payload as { type: string }).type);
    expect(types).toContain("stage");
    expect(types).toContain("log");
    expect(types).toContain("progress");

    const finishedEvent = events[events.length - 1]?.payload as {
      ok: boolean;
      outcome: { to: { id: string } };
    };
    expect(finishedEvent.ok).toBe(true);
    expect(finishedEvent.outcome.to.id).toBe("b".repeat(40));
    expect(
      (finishedEvent.outcome as { warnings?: string[] }).warnings,
    ).toBeUndefined();
    expect(app.installed?.id).toBe("b".repeat(40));
    expect(app.busy).toBe(false);
    expect(finished).toHaveBeenCalledOnce();
  });

  it("cancels a running mock update with the backend's cancelled error", () => {
    const app = view();
    const handle = startMockUpdate(app, "latest", { emitter });
    vi.advanceTimersByTime(200);

    handle.cancel();
    vi.advanceTimersByTime(2000);

    const finished = events.filter(
      (event) => event.name === "headway://update-finished",
    );
    const finishedEvent = finished[finished.length - 1]?.payload as {
      ok: boolean;
      error: { code: string; exit_code: number };
    };
    expect(finishedEvent.ok).toBe(false);
    expect(finishedEvent.error.code).toBe("cancelled");
    expect(finishedEvent.error.exit_code).toBe(7);
    expect(app.busy).toBe(false);
  });

  it("exposes the checksum-missing warning in the ?scenario=update-warning preview", () => {
    window.history.replaceState({}, "", "/?mock&scenario=update-warning");
    const app = view();
    startMockUpdate(app, "latest", { emitter });
    vi.advanceTimersByTime(1000);

    const finishedEvent = events[events.length - 1]?.payload as {
      ok: boolean;
      outcome: { warnings?: string[] };
    };
    expect(finishedEvent.ok).toBe(true);
    expect(finishedEvent.outcome.warnings).toHaveLength(1);
    expect(finishedEvent.outcome.warnings?.[0]).toContain(
      "no supported SHA-256 digest",
    );
    expect(finishedEvent.outcome.warnings?.[0]).toContain(
      "not publisher authentication",
    );
    window.history.replaceState({}, "", "/");
  });

  it("reports a failed run in the ?scenario=update-error preview", () => {
    // The scenario comes from the preview URL, which jsdom lets us set.
    window.history.replaceState({}, "", "/?mock&scenario=update-error");
    const app = view();
    startMockUpdate(app, "latest", { emitter });
    vi.advanceTimersByTime(1000);

    const finishedEvent = events[events.length - 1]?.payload as {
      ok: boolean;
      error: { code: string };
    };
    expect(finishedEvent.ok).toBe(false);
    expect(finishedEvent.error.code).toBe("command_failed");
    expect(app.status).toBe("error");
    expect(app.last_error?.during).toBe("update");
    window.history.replaceState({}, "", "/");
  });
});
