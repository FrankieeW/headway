import { describe, expect, it } from "vitest";

import { normalizeCheckSummary } from "./checkSummary";

describe("normalizeCheckSummary", () => {
  it("reads the shared results shape", () => {
    const normalized = normalizeCheckSummary({
      checked: 3,
      with_updates: 1,
      results: [
        {
          id: "zed",
          ok: true,
          status: "update_available",
          pending_count: 2,
          latest: { id: "abc", title: "abc" },
        },
        {
          id: "rg",
          ok: true,
          status: "up_to_date",
          pending_count: 0,
          latest: null,
        },
        {
          id: "bad",
          ok: false,
          error: { code: "network_error", message: "offline", exit_code: 4 },
        },
      ],
    });

    expect(normalized.checked).toBe(3);
    expect(normalized.with_updates).toBe(1);
    expect(normalized.failures).toEqual([
      {
        id: "bad",
        message: "offline",
        error: { code: "network_error", message: "offline", exit_code: 4 },
      },
    ]);
  });

  it("prefers the backward-compatible failed aggregate when present", () => {
    const normalized = normalizeCheckSummary({
      checked: 1,
      with_updates: 0,
      results: [],
      failed: [{ id: "rg", message: "check failed" }],
    });

    expect(normalized.failures).toEqual([
      { id: "rg", message: "check failed", error: null },
    ]);
  });

  it("still understands an older backend that only returns failed", () => {
    const normalized = normalizeCheckSummary({
      checked: 2,
      with_updates: 0,
      failed: [{ id: "a", message: "first" }],
    } as Parameters<typeof normalizeCheckSummary>[0]);

    expect(normalized.results).toEqual([]);
    expect(normalized.failures).toHaveLength(1);
  });

  it("reports no failures for an all-ok summary", () => {
    const normalized = normalizeCheckSummary({
      checked: 1,
      with_updates: 0,
      results: [{ id: "a", ok: true, status: "up_to_date", pending_count: 0 }],
    });
    expect(normalized.failures).toEqual([]);
  });
});
