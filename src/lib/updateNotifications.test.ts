import { describe, expect, it } from "vitest";

import type { UpdateFinished } from "@/lib/api";
import type { TranslateFn } from "@/lib/format";
import {
  completedOutcomeNote,
  outcomeWarnings,
  updateNotification,
  warningDescription,
  WARNING_PREVIEW_LIMIT,
} from "./updateNotifications";

/** Minimal translator double: echoes the key and interpolation data. */
const t: TranslateFn = (key, options) =>
  options && Object.keys(options).length > 0
    ? `${key}:${JSON.stringify(options)}`
    : key;

const finished = (
  warnings?: string[],
  overrides: Partial<UpdateFinished> = {},
): UpdateFinished => ({
  app_id: "tool",
  ok: true,
  outcome: {
    app_id: "tool",
    changed: true,
    to: {
      id: "abc1234",
      version: "1.2.3",
      installed_at: "2026-10-25T00:00:00Z",
    },
    duration_ms: 1200,
    ...(warnings ? { warnings } : {}),
  },
  ...overrides,
});

describe("update notifications", () => {
  it("shows a normal verified success when no warnings were recorded", () => {
    const notice = updateNotification(finished(), "Tool", t);
    expect(notice.tone).toBe("success");
    expect(notice.description).toBeUndefined();
    expect(notice.title).toBe(
      'toast.updatedTo:{"name":"Tool","version":"1.2.3"}',
    );
    expect(
      outcomeWarnings({ app_id: "tool", ok: true }),
    ).toEqual([]);
  });

  it("never announces a checksum-missing run as normal verified success", () => {
    // Given a release installed without a published digest (backend warning).
    const warning =
      "GitHub published no supported SHA-256 digest for this asset; the downloaded SHA-256 is 0123456789abcdef. No published checksum was verified; this is not publisher authentication.";
    // When the finished run is announced.
    const notice = updateNotification(finished([warning]), "Tool", t);
    // Then it is a warning toast, the backend text is preserved verbatim, and
    // the log hint is added.
    expect(notice.tone).toBe("warning");
    expect(notice.title).toBe(
      'toast.updatedToWithWarnings:{"name":"Tool","version":"1.2.3"}',
    );
    expect(notice.warnings).toEqual([warning]);
    expect(notice.description).toContain(warning);
    expect(notice.description).toContain("toast.warningsLogHint");
    expect(notice.description).not.toContain("toast.updatedTo:");
  });

  it("keeps untrusted warning text as data instead of interpreting it", () => {
    const notice = updateNotification(
      finished(['<img src=x onerror="alert(1)"> & "quoted"']),
      "Tool",
      t,
    );
    expect(notice.description).toContain('<img src=x onerror="alert(1)">');
    expect(notice.description).toContain('& "quoted"');
  });

  it("previews the first warnings and counts the rest", () => {
    const warnings = ["one", "two", "three", "four", "five"];
    const description = warningDescription(warnings, t);
    expect(description).toContain("one");
    expect(description).toContain("two");
    expect(description).toContain("three");
    expect(description).not.toContain("four");
    expect(description).toContain(
      `toast.warningsMore:{"count":${warnings.length - WARNING_PREVIEW_LIMIT}}`,
    );
    expect(description).toContain("toast.warningsLogHint");
  });

  it("does not add a warning description without warnings", () => {
    expect(warningDescription([], t)).toBeUndefined();
  });

  it("names the last completed outcome in a failure toast", () => {
    const payload = finished(undefined, {
      ok: false,
      error: { code: "command_failed", message: "build failed" },
    });
    expect(completedOutcomeNote(payload, t)).toBe(
      'toast.completedBeforeFailure:{"version":"1.2.3"}',
    );
    expect(completedOutcomeNote(finished(), t)).toBeUndefined();
    expect(
      completedOutcomeNote({ app_id: "tool", ok: false }, t),
    ).toBeUndefined();
  });
});
