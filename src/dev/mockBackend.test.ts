import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => undefined) }));

import { apps, settings } from "./mockFixtures";
import { handleMockCommand } from "./mockBackend";

const zed = () => apps.find((view) => view.app.id === "zed");
const initialAppCount = apps.length;

describe("mock backend command fidelity", () => {
  beforeEach(() => {
    // Token state is module-local in the mock; restore a keychain token.
    handleMockCommand("set_token", { token: "token" });
  });

  it("binds list_repositories for GitHub apps and returns nothing for others", () => {
    const app = zed()?.app;
    const expected =
      app && app.source === "github" ? (app.repositories ?? []) : [];
    expect(handleMockCommand("list_repositories", { id: "zed" })).toEqual(
      expected,
    );
    expect(handleMockCommand("list_repositories", { id: "npm-tool" })).toEqual(
      [],
    );
  });

  it("returns a plan document for latest, next and an explicit ref", () => {
    const latest = handleMockCommand("plan_update", {
      id: "zed",
      target: "latest",
    }) as { to: { id: string }; up_to_date: boolean; method: string };
    expect(latest.to.id).toBe(zed()?.last_check?.latest?.id);
    expect(latest.up_to_date).toBe(false);
    expect(latest.method).toBe("source");

    const next = handleMockCommand("plan_update", {
      id: "zed",
      target: "next",
    }) as { to: { id: string } };
    const pending = zed()?.last_check?.pending ?? [];
    expect(next.to.id).toBe(pending[pending.length - 1]?.id);

    const explicit = handleMockCommand("plan_update", {
      id: "zed",
      target: { ref: "abc123" },
    }) as { to: { id: string } };
    expect(explicit.to.id).toBe("abc123");
  });

  it("persists saved settings, including the shared language", () => {
    const saved = handleMockCommand("save_settings", {
      settings: { ...settings, language: "en" },
    }) as { language?: string };
    expect(saved.language).toBe("en");
    expect(
      (handleMockCommand("get_settings", undefined) as { language?: string })
        .language,
    ).toBe("en");
    handleMockCommand("save_settings", { settings: { ...settings, language: "system" } });
  });

  it("reports the shared check_all shape", () => {
    const summary = handleMockCommand("check_all", undefined) as {
      results: { id: string; ok: boolean; status?: string }[];
      checked: number;
      with_updates: number;
      failed: unknown[];
    };
    expect(summary.results).toHaveLength(apps.length);
    expect(summary.checked).toBe(apps.length);
    expect(summary.results[0]).toMatchObject({ id: "zed", ok: true });
    expect(summary.failed).toEqual([]);
    expect(summary.with_updates).toBe(
      apps.filter((view) => view.pending_count > 0).length,
    );
  });

  it("clears the token instead of leaving the old fixture", () => {
    expect(handleMockCommand("clear_token", undefined)).toEqual({
      source: "none",
    });
    expect(handleMockCommand("token_status", undefined)).toEqual({
      source: "none",
    });
  });

  it("removes an app from the fixture list last, so earlier cases keep it", () => {
    const before = apps.length;
    const result = handleMockCommand("remove_app", { id: "zed", purge: true });
    expect(result).toEqual({ id: "zed", purged: true });
    expect(apps).toHaveLength(before - 1);
    expect(zed()).toBeUndefined();
    expect(initialAppCount).toBe(before);
  });
});
