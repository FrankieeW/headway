import { describe, expect, it } from "vitest";

import type { SelfUpdateStatus } from "@/lib/api";
import {
  SELF_UPDATE_ACTIVE_POLL_MS,
  selfUpdateIsActive,
  selfUpdateRefetchInterval,
} from "./polling";

const reviewStatus = (overrides: Partial<SelfUpdateStatus>): SelfUpdateStatus =>
  ({
    running: { version: "1", commit: "a", dirty: false, target: null },
    installed: null,
    pending_restart: false,
    restart_requested: false,
    confirmed: false,
    ...overrides,
  }) as SelfUpdateStatus;

describe("self-update polling", () => {
  it("does not poll while idle (no subprocess churn)", () => {
    expect(selfUpdateRefetchInterval(undefined)).toBe(false);
    expect(selfUpdateRefetchInterval(reviewStatus({}))).toBe(false);
    expect(selfUpdateIsActive(reviewStatus({}))).toBe(false);
  });

  it("polls quickly while a restart is pending", () => {
    const pending = reviewStatus({ pending_restart: true });
    expect(selfUpdateIsActive(pending)).toBe(true);
    expect(selfUpdateRefetchInterval(pending)).toBe(
      SELF_UPDATE_ACTIVE_POLL_MS,
    );
  });

  it("polls quickly while a restart request is queued", () => {
    const queued = reviewStatus({ restart_requested: true });
    expect(selfUpdateRefetchInterval(queued)).toBe(
      SELF_UPDATE_ACTIVE_POLL_MS,
    );
  });

  it("stops polling after a terminal restart error", () => {
    const failed = reviewStatus({
      pending_restart: true,
      restart_error: "commit mismatch",
    });
    // `restart_error` is terminal until the user retries, so polling it would
    // only spawn subprocesses without a state change.
    expect(selfUpdateRefetchInterval(failed)).toBe(false);
  });
});
