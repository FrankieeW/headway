import { useCallback, useSyncExternalStore } from "react";

import type {
  LogStream,
  Stage,
  UpdateEvent,
  UpdateFinished,
} from "./api/types";

/** One line of the running command's output. */
export interface RunLine {
  stream: LogStream;
  line: string;
}

/** Download / copy progress of the current stage. */
export interface RunProgress {
  done: number;
  total?: number;
}

/** The live (or just finished) update run of one app. */
export interface UpdateRun {
  /** output lines, newest last; capped, oldest lines are dropped first */
  lines: RunLine[];
  stage?: Stage;
  stageMessage?: string;
  progress?: RunProgress;
  /** epoch ms when the run started */
  startedAt: number;
  /** set once the backend reported the outcome */
  finished?: { ok: boolean; message?: string };
}

/** Keep at most this many log lines per app in memory. */
export const MAX_RUN_LINES = 2000;

/**
 * A finished run keeps its output so the user can still read it; after this
 * long it is dropped so a long-lived window does not accumulate one 2 000-line
 * buffer per app it ever updated.
 */
export const FINISHED_RUN_TTL_MS = 5 * 60_000;

/** Upper bound on cached runs, even while many updates are active. */
export const MAX_CACHED_RUNS = 40;

const runs = new Map<string, UpdateRun>();
const listeners = new Set<() => void>();
const evictionTimers = new Map<string, ReturnType<typeof setTimeout>>();

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribeRuns(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getRun(id: string): UpdateRun | undefined {
  return runs.get(id);
}

function setRun(id: string, run: UpdateRun | undefined): void {
  if (run === undefined) {
    runs.delete(id);
  } else {
    runs.set(id, run);
  }
  notify();
}

function clearEviction(id: string): void {
  const timer = evictionTimers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    evictionTimers.delete(id);
  }
}

/** Drop a finished run after the TTL; an unfinished run is never evicted. */
function scheduleEviction(id: string): void {
  clearEviction(id);
  const timer = setTimeout(() => {
    evictionTimers.delete(id);
    const run = runs.get(id);
    if (run?.finished) setRun(id, undefined);
  }, FINISHED_RUN_TTL_MS);
  // Do not keep a test process or a hidden window alive for a log buffer.
  (timer as unknown as { unref?: () => void }).unref?.();
  evictionTimers.set(id, timer);
}

/** Evict the oldest finished runs when the cache grows past its bound. */
function evictOverflow(): void {
  if (runs.size <= MAX_CACHED_RUNS) return;
  const finished = [...runs.entries()]
    .filter(([, run]) => run.finished)
    .sort(([, a], [, b]) => a.startedAt - b.startedAt);
  for (const [id] of finished) {
    if (runs.size <= MAX_CACHED_RUNS) break;
    clearEviction(id);
    runs.delete(id);
  }
  notify();
}

function createRun(startedAt: number): UpdateRun {
  return { lines: [], startedAt };
}

/** Appends a line, dropping the oldest ones beyond MAX_RUN_LINES. */
function appendLine(run: UpdateRun, line: RunLine): UpdateRun {
  const lines = [...run.lines, line];
  return {
    ...run,
    lines:
      lines.length > MAX_RUN_LINES
        ? lines.slice(lines.length - MAX_RUN_LINES)
        : lines,
  };
}

/**
 * Pure reducer: applies one backend event to a run.
 *
 * The caller decides which app the event belongs to, so this function
 * only looks at the event payload.
 */
export function reduceRun(run: UpdateRun, event: UpdateEvent): UpdateRun {
  switch (event.type) {
    case "stage":
      // Stage messages are sent once, as stage events; they also belong in the log.
      return {
        ...appendLine(run, { stream: "info", line: event.message }),
        stage: event.stage,
        stageMessage: event.message,
        progress: undefined,
      };

    case "log":
      return appendLine(run, { stream: event.stream, line: event.line });

    case "progress":
      return {
        ...run,
        progress:
          event.total === null || event.total === undefined
            ? { done: event.done }
            : { done: event.done, total: event.total },
      };

    default:
      return run;
  }
}

/** Feed one event into the store. */
export function applyUpdateEvent(event: UpdateEvent): void {
  const id = event.app_id;
  const current = runs.get(id) ?? createRun(Date.now());
  setRun(id, reduceRun(current, event));
}

/** A run just started: drop the lines of the previous one. */
export function markRunStarted(id: string): void {
  clearEviction(id);
  setRun(id, createRun(Date.now()));
  evictOverflow();
}

/** The backend reported the outcome of a run. */
export function markRunFinished(payload: UpdateFinished): void {
  const id = payload.app_id;
  const current = runs.get(id) ?? createRun(Date.now());
  const message = payload.error?.message;

  const lines = message
    ? [...current.lines, { stream: "stderr" as LogStream, line: message }]
    : current.lines;

  setRun(id, {
    ...current,
    lines,
    finished: { ok: payload.ok, message },
  });
  scheduleEviction(id);
  evictOverflow();
}

/** Forget a run (used when a start attempt fails immediately). */
export function clearRun(id: string): void {
  clearEviction(id);
  setRun(id, undefined);
}

/** Read a run outside of React. */
export function getUpdateRun(id: string): UpdateRun | undefined {
  return getRun(id);
}

/** Subscribe to the live run of one app. */
export function useUpdateRun(id: string | undefined): UpdateRun | undefined {
  const subscribe = useCallback(
    (onStoreChange: () => void) => subscribeRuns(onStoreChange),
    [],
  );
  const getSnapshot = useCallback(
    () => (id ? getRun(id) : undefined),
    [id],
  );

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Test helper: drop every run and its eviction timer. */
export function resetRuns(): void {
  for (const timer of evictionTimers.values()) clearTimeout(timer);
  evictionTimers.clear();
  runs.clear();
  notify();
}
