import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  applyUpdateEvent,
  clearRun,
  FINISHED_RUN_TTL_MS,
  getUpdateRun,
  markRunFinished,
  markRunStarted,
  MAX_CACHED_RUNS,
  MAX_RUN_LINES,
  reduceRun,
  resetRuns,
  type UpdateRun,
} from "./updateRuns";

const emptyRun = (): UpdateRun => ({ lines: [], startedAt: 1000 });

describe("reduceRun", () => {
  it("records the stage and its message", () => {
    const run = reduceRun(emptyRun(), {
      type: "stage",
      app_id: "app",
      stage: "build",
      message: "cargo build --release",
    });

    expect(run.stage).toBe("build");
    expect(run.stageMessage).toBe("cargo build --release");
    expect(run.lines.at(-1)).toEqual({
      stream: "info",
      line: "cargo build --release",
    });
  });

  it("appends log lines", () => {
    let run = emptyRun();
    run = reduceRun(run, {
      type: "log",
      app_id: "app",
      stream: "stdout",
      line: "first",
    });
    run = reduceRun(run, {
      type: "log",
      app_id: "app",
      stream: "stderr",
      line: "second",
    });

    expect(run.lines).toEqual([
      { stream: "stdout", line: "first" },
      { stream: "stderr", line: "second" },
    ]);
  });

  it("keeps the stream of a line", () => {
    const run = reduceRun(emptyRun(), {
      type: "log",
      app_id: "app",
      stream: "info",
      line: "hello",
    });

    expect(run.lines[0].stream).toBe("info");
  });

  it("drops the oldest lines when the cap is reached", () => {
    let run = emptyRun();
    for (let index = 0; index < MAX_RUN_LINES + 10; index += 1) {
      run = reduceRun(run, {
        type: "log",
        app_id: "app",
        stream: "stdout",
        line: `line ${index}`,
      });
    }

    expect(run.lines).toHaveLength(MAX_RUN_LINES);
    expect(run.lines[0].line).toBe("line 10");
    expect(run.lines[MAX_RUN_LINES - 1].line).toBe(
      `line ${MAX_RUN_LINES + 9}`,
    );
  });

  it("records the progress", () => {
    const run = reduceRun(emptyRun(), {
      type: "progress",
      app_id: "app",
      done: 10,
      total: 100,
    });

    expect(run.progress).toEqual({ done: 10, total: 100 });
  });

  it("records progress without a total", () => {
    const run = reduceRun(emptyRun(), {
      type: "progress",
      app_id: "app",
      done: 10,
      total: null,
    });

    expect(run.progress).toEqual({ done: 10 });
  });

  it("does not change the run for an unknown event", () => {
    const run = emptyRun();
    // @ts-expect-error deliberately wrong payload
    expect(reduceRun(run, { type: "nope" })).toBe(run);
  });
});

describe("the run store", () => {
  beforeEach(() => {
    resetRuns();
  });

  it("starts a run", () => {
    markRunStarted("app");
    const run = getUpdateRun("app");
    expect(run).toBeDefined();
    expect(run?.lines).toEqual([]);
    expect(run?.finished).toBeUndefined();
  });

  it("collects the events of one app", () => {
    applyUpdateEvent({
      type: "stage",
      app_id: "app",
      stage: "fetch",
      message: "cloning",
    });
    applyUpdateEvent({
      type: "log",
      app_id: "app",
      stream: "stdout",
      line: "hello",
    });

    const run = getUpdateRun("app");
    expect(run?.stage).toBe("fetch");
    expect(run?.lines).toEqual([
      { stream: "info", line: "cloning" },
      { stream: "stdout", line: "hello" },
    ]);
  });

  it("keeps the apps apart", () => {
    applyUpdateEvent({
      type: "log",
      app_id: "one",
      stream: "stdout",
      line: "first",
    });
    applyUpdateEvent({
      type: "log",
      app_id: "two",
      stream: "stdout",
      line: "second",
    });

    expect(getUpdateRun("one")?.lines).toHaveLength(1);
    expect(getUpdateRun("two")?.lines).toHaveLength(1);
    expect(getUpdateRun("one")?.lines[0].line).toBe("first");
  });

  it("marks a run as finished", () => {
    markRunStarted("app");
    markRunFinished({ app_id: "app", ok: true });

    const run = getUpdateRun("app");
    expect(run?.finished?.ok).toBe(true);
  });

  it("keeps the error message of a failed run", () => {
    markRunStarted("app");
    markRunFinished({
      app_id: "app",
      ok: false,
      error: { code: "build_failed", message: "cargo failed" },
    });

    const run = getUpdateRun("app");
    expect(run?.finished?.ok).toBe(false);
    expect(run?.finished?.message).toBe("cargo failed");
    expect(run?.lines[run.lines.length - 1]).toEqual({
      stream: "stderr",
      line: "cargo failed",
    });
  });

  it("clears a run", () => {
    markRunStarted("app");
    clearRun("app");
    expect(getUpdateRun("app")).toBeUndefined();
  });
});

describe("finished run lifecycle", () => {
  beforeEach(() => {
    resetRuns();
    vi.useFakeTimers();
  });

  afterEach(() => {
    resetRuns();
    vi.useRealTimers();
  });

  it("evicts a finished run after its TTL but keeps an active one", () => {
    markRunStarted("done");
    markRunFinished({ app_id: "done", ok: true });
    markRunStarted("active");

    // When the finished run is older than the TTL.
    vi.advanceTimersByTime(FINISHED_RUN_TTL_MS + 1);

    // Then its output buffer is released, while the active run stays.
    expect(getUpdateRun("done")).toBeUndefined();
    expect(getUpdateRun("active")).toBeDefined();
  });

  it("keeps a finished run visible before the TTL", () => {
    markRunStarted("done");
    markRunFinished({ app_id: "done", ok: true });
    vi.advanceTimersByTime(FINISHED_RUN_TTL_MS - 1);
    expect(getUpdateRun("done")?.finished?.ok).toBe(true);
  });

  it("bounds the number of cached runs by evicting the oldest finished one", () => {
    for (let index = 0; index < MAX_CACHED_RUNS + 5; index += 1) {
      const id = `app-${index}`;
      markRunStarted(id);
      markRunFinished({ app_id: id, ok: true });
    }

    const cached = Array.from({ length: MAX_CACHED_RUNS + 5 }, (_, index) =>
      getUpdateRun(`app-${index}`),
    ).filter(Boolean);
    expect(cached.length).toBeLessThanOrEqual(MAX_CACHED_RUNS);
    // The oldest finished runs are the ones dropped.
    expect(getUpdateRun("app-0")).toBeUndefined();
  });
});
