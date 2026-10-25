import { emit as tauriEmit } from "@tauri-apps/api/event";

import {
  EVENTS,
  type AppView,
  type Candidate,
  type InstalledRef,
  type Target,
  type UpdateFinished,
} from "@/lib/api";

/**
 * A faithful preview of the real update run for `?mock`:
 * stage/log/progress events, then `update-finished` and `apps-changed`, with
 * the same cancellation semantics (a cancelled run reports `cancelled`).
 *
 * `emit` is injectable so the behaviour can be tested without Tauri.
 */
export interface MockEmitter {
  emit(name: string, payload?: unknown): void;
}

const defaultEmitter: MockEmitter = {
  emit: (name, payload) => {
    void tauriEmit(name, payload);
  },
};

const scenario = () =>
  new URLSearchParams(location.search).get("scenario");

/** Events the card needs to show a live run, in the backend's order. */
const STAGES = [
  { stage: "resolve", message: "Resolving the latest version" },
  { stage: "fetch", message: "Fetching the repository" },
  { stage: "build", message: "Building the app" },
  { stage: "install", message: "Installing the new version" },
] as const;

function candidateFor(view: AppView, target: Target): Candidate {
  if (typeof target === "object") {
    return { id: target.ref, title: target.ref, prerelease: false };
  }
  const pending = view.last_check?.pending ?? [];
  // `next` is the oldest pending change, `latest` the newest.
  const chosen =
    target === "next" ? pending[pending.length - 1] : view.last_check?.latest;
  return chosen ?? { id: "preview", title: "Preview build", prerelease: false };
}

function installedFor(view: AppView, candidate: Candidate): InstalledRef {
  const configuredPath =
    view.app.source === "github" ? view.app.install.dest : null;
  return {
    id: candidate.id,
    sha: candidate.sha ?? candidate.id,
    version: `preview-${candidate.id.slice(0, 7)}`,
    installed_at: new Date().toISOString(),
    path:
      view.installed?.path ??
      configuredPath ??
      (view.app.kind === "app"
        ? `/Applications/${view.app.name}.app`
        : `/Users/preview/.local/bin/${view.app.id}`),
  };
}

export interface MockUpdateOptions {
  emitter?: MockEmitter;
  /** Called once the run is over (success, failure or cancel). */
  onFinished?: () => void;
}

export interface MockUpdateHandle {
  /** Stop the run and report it as cancelled, like the backend does. */
  cancel(): void;
}

/**
 * Start a mock update for one app. Timers are real timeouts so the preview
 * animates; tests drive them with fake timers.
 */
export function startMockUpdate(
  view: AppView,
  target: Target,
  options: MockUpdateOptions = {},
): MockUpdateHandle {
  const emitter = options.emitter ?? defaultEmitter;
  const appId = view.app.id;
  const from = view.installed ?? null;
  const candidate = candidateFor(view, target);
  const timers: ReturnType<typeof setTimeout>[] = [];
  let finished = false;

  view.busy = true;
  view.status = "busy";

  const later = (ms: number, action: () => void): void => {
    timers.push(setTimeout(action, ms));
  };

  const finish = (payload: UpdateFinished): void => {
    if (finished) return;
    finished = true;
    for (const timer of timers) clearTimeout(timer);
    view.busy = false;
    options.onFinished?.();
    emitter.emit(EVENTS.appsChanged);
    emitter.emit(EVENTS.updateFinished, payload);
  };

  const fail = (message: string): void => {
    view.status = "error";
    view.last_check = null;
    view.last_error = {
      code: "command_failed",
      message,
      at: new Date().toISOString(),
      during: "update",
    };
    finish({
      app_id: appId,
      ok: false,
      error: { code: "command_failed", message, exit_code: 5 },
    });
  };

  STAGES.forEach((entry, index) => {
    later(50 + index * 150, () => {
      emitter.emit(EVENTS.updateEvent, {
        type: "stage",
        app_id: appId,
        stage: entry.stage,
        message: entry.message,
      });
    });
  });
  later(120, () => {
    emitter.emit(EVENTS.updateEvent, {
      type: "log",
      app_id: appId,
      stream: "stdout",
      line: "Compiling preview fixture…",
    });
  });
  later(300, () => {
    emitter.emit(EVENTS.updateEvent, {
      type: "progress",
      app_id: appId,
      done: 1,
      total: 2,
    });
  });

  later(900, () => {
    if (scenario() === "update-error") {
      fail("Mock build failed; the prior installation was preserved.");
      return;
    }
    const to = installedFor(view, candidate);
    view.installed = to;
    view.status = "up_to_date";
    view.last_error = null;
    view.pending_count = 0;
    view.last_check = {
      checked_at: new Date().toISOString(),
      status: "up_to_date",
      latest: candidate,
      pending: [],
      pending_total: 0,
    };
    Object.assign(view, {
      installation: {
        presence: "present",
        path: to.path,
        baseline_matches: true,
        recorded:
          view.app.source === "github" && to.path
            ? { repo: view.app.repo, installed: to }
            : null,
      },
    });
    // `?scenario=update-warning` previews a run where GitHub published no
    // digest: the install succeeds, but it must not look like verified success.
    const warnings =
      scenario() === "update-warning"
        ? [
            "GitHub published no supported SHA-256 digest for this asset; the downloaded SHA-256 is 0123456789abcdef. No published checksum was verified; this is not publisher authentication.",
          ]
        : [];
    finish({
      app_id: appId,
      ok: true,
      outcome: {
        app_id: appId,
        changed: true,
        from,
        to,
        installed_path: to.path,
        log_path: "~/Library/Logs/Headway/preview.log",
        duration_ms: 900,
        ...(warnings.length > 0 ? { warnings } : {}),
      },
    });
  });

  return {
    cancel: () => {
      if (finished) return;
      view.status = "unchecked";
      finish({
        app_id: appId,
        ok: false,
        error: { code: "cancelled", message: "cancelled", exit_code: 7 },
      });
    },
  };
}
