import { emit } from "@tauri-apps/api/event";
import { EVENTS, type AppView, type RevisionRequest } from "@/lib/api";
import { mockPull } from "./mockPulls";
const scenario = () => new URLSearchParams(location.search).get("scenario");
export function run(view: AppView, revision: RevisionRequest): void {
  view.busy = true;
  view.status = "busy";
  const from = view.installed;
  const method =
    revision.method === "configured" && view.app.source === "github"
      ? view.app.install.method
      : revision.method;
  setTimeout(() => {
    void emit(EVENTS.updateEvent, {
      type: "stage",
      app_id: view.app.id,
      stage: method === "asset" ? "download" : "build",
      message: "Mock managed revision run",
    });
  }, 50);
  setTimeout(() => {
    const failed = scenario() === "install-error";
    view.busy = false;
    view.status = failed ? "error" : "unchecked";
    view.last_error = failed
      ? {
          code: "build_failed",
          message: "Mock build failed; the prior installation was preserved.",
          at: new Date().toISOString(),
          during: "update",
        }
      : null;
    const configuredPath =
      view.app.source === "github" ? view.app.install.dest : null;
    const reference =
      revision.reference.split("@").at(-1) ?? revision.reference;
    const to = {
      id: reference,
      sha: reference,
      version: `preview-${reference.slice(0, 7)}`,
      installed_at: new Date().toISOString(),
      path:
        view.installed?.path ??
        configuredPath ??
        (view.app.kind === "app"
          ? `/Applications/${view.app.name}.app`
          : `/Users/preview/.local/bin/${view.app.id}`),
    };
    if (!failed) {
      view.installed = to;
      Object.assign(view, {
        installation: {
          presence: "present",
          path: to.path,
          baseline_matches: true,
          recorded:
            view.app.source === "github"
              ? { repo: view.app.repo, installed: to }
              : null,
        },
      });
      if (
        view.app.source === "github" &&
        revision.reference.startsWith("pr:")
      ) {
        view.app = { ...view.app, auto_update: false };
        Object.assign(view, {
          pull_request: mockPull(
            view.app.repo,
            Number(revision.reference.slice(3).split("@")[0]),
          ),
        });
      }
    }
    void emit(EVENTS.appsChanged);
    void emit(EVENTS.updateFinished, {
      app_id: view.app.id,
      ok: !failed,
      ...(failed
        ? {
            error: {
              code: "build_failed",
              message:
                "Mock build failed; the prior installation was preserved.",
            },
          }
        : {
            outcome: {
              app_id: view.app.id,
              changed: true,
              from,
              to,
              installed_path: to.path,
              duration_ms: 1200,
            },
          }),
    });
  }, 1200);
}
