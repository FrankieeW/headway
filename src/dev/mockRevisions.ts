import { run } from "./mockRevisionRun";
import { filterCommitPage } from "./mockCommitPage";
import { emit } from "@tauri-apps/api/event";
import { pullOptions } from "./mockPulls";
import {
  EVENTS,
  type AppView,
  type AssetInstall,
  type Candidate,
  type CommitPage,
  type RevisionOptions,
  type RevisionRequest,
  type SourceInstall,
  type VariantRequest,
} from "@/lib/api";

const anchor = "9f4c1e2a7b3d5f6081a2b3c4d5e6f7a8b9c0d1e2";
const oldReleaseCommit = "e".repeat(40);
const pendingSha = "8e3b0d19f6a2c4e5d7b8a9c0d1e2f3a4b5c6d7e8";
const scenario = () => new URLSearchParams(location.search).get("scenario");
const candidate = (sha: string, index: number): Candidate => ({
  id: sha,
  sha,
  title:
    index === 0
      ? "Verified release build"
      : `Commit ${index + 1}: editable revision`,
  author: "Preview author",
  date: "2026-10-24T08:00:00Z",
  prerelease: false,
});
function validateReleaseTag(request: RevisionRequest): void {
  if (!request.release_tag) return;
  const expected =
    request.release_tag === "v0.162.0"
      ? anchor
      : request.release_tag === "v0.001.0"
        ? oldReleaseCommit
        : null;
  if (scenario() === "release-moved" || request.reference !== expected)
    throw {
      code: "invalid_input",
      message: "Release tag no longer matches the selected commit.",
    };
}
export function handleRevisions(
  cmd: string,
  args: Record<string, unknown> | undefined,
  apps: AppView[],
): unknown {
  const id = typeof args?.id === "string" ? args.id : "";
  const view = apps.find((item) => item.app.id === id);
  switch (cmd) {
    case "source_storage_status": {
      if (scenario() === "storage-error")
        throw {
          code: "io",
          message: "Source checkout location is not owned by Headway.",
        };
      const storage =
        view?.app.source === "github" && view.app.install.method === "source"
          ? view.app.install.storage
          : undefined;
      return {
        path: `${storage?.root || "/Users/preview/Library/Caches/Headway/source"}/${id}`,
        keep: storage?.keep ?? true,
        exists: scenario() !== "storage-empty",
        custom_root: Boolean(storage?.root),
      };
    }
    case "list_commits": {
      if (scenario() === "commits-error")
        throw {
          code: "github_rate_limited",
          message: "Commit history is temporarily rate limited.",
        };
      const page = typeof args?.page === "number" ? args.page : 1;
      const perPage = typeof args?.perPage === "number" ? args.perPage : 20;
      const start = (page - 1) * perPage;
      const commits =
        scenario() === "commits-empty"
          ? []
          : Array.from(
              { length: Math.max(0, Math.min(perPage, 45 - start)) },
              (_, i) => {
                const index = start + i;
                return candidate(
                  index === 0
                    ? anchor
                    : index === 1
                      ? pendingSha
                      : (index + 1).toString(16).padStart(40, "0"),
                  index,
                );
              },
            );
      return filterCommitPage(
        {
          repo: view?.app.source === "github" ? view.app.repo : "owner/tool",
          branch:
            view?.app.source === "github" && view.app.track.mode === "commit"
              ? view.app.track.branch || "main"
              : "main",
          anchor: typeof args?.anchor === "string" ? args.anchor : anchor,
          page,
          per_page: perPage,
          commits,
          has_more: commits.length > 0 && start + commits.length < 45,
          next_page:
            commits.length > 0 && start + commits.length < 45 ? page + 1 : null,
        } satisfies CommitPage,
        args?.filter,
      );
    }
    case "revision_options": {
      const reference =
        typeof args?.reference === "string" ? args.reference : "";
      const pull = view ? pullOptions(view, reference) : null;
      if (pull) return pull;
      if (scenario() === "revision-error" || reference === "bad-ref")
        throw {
          code: "not_found",
          message: "The requested revision could not be resolved.",
        };
      const commit = /^[0-9a-f]{40}$/i.test(reference)
        ? reference
        : reference === "unreleased"
          ? pendingSha
          : reference === "v0.001.0"
            ? oldReleaseCommit
            : anchor;
      const tag =
        commit === anchor
          ? "v0.162.0"
          : commit === oldReleaseCommit
            ? "v0.001.0"
            : null;
      const released = Boolean(tag);
      const source: SourceInstall | null =
        view?.app.source === "github"
          ? view.app.install.method === "source"
            ? view.app.install
            : (view.app.source_build ?? null)
          : null;
      const asset: AssetInstall = {
        method: "asset",
        pattern: "*.zip",
        binary: view?.app.kind === "app" ? "Preview.app" : "tool",
      };
      return {
        reference,
        commit,
        candidate: candidate(commit, released ? 0 : 1),
        release_tag: tag,
        release_url: released
          ? `https://github.com/${view?.app.source === "github" ? view.app.repo : "owner/tool"}/releases/tag/${tag}`
          : null,
        can_download: released,
        source_build: source,
        release_asset: asset,
      } satisfies RevisionOptions;
    }
    case "plan_revision":
    case "plan_variant": {
      if (scenario() === "plan-error")
        throw {
          code: "invalid_input",
          message: "Installation plan could not validate the recipe.",
        };
      const parentId =
        cmd === "plan_variant" && typeof args?.parentId === "string"
          ? args.parentId
          : id;
      const parent = apps.find((item) => item.app.id === parentId);
      const variant =
        cmd === "plan_variant" ? (args?.request as VariantRequest) : null;
      const request = variant
        ? variant.revision
        : (args?.request as RevisionRequest);
      validateReleaseTag(request);
      const destination = variant
        ? `/Applications/Headway Versions/${variant.id}.app`
        : parent?.installed?.path ||
          (parent?.app.kind === "cli"
            ? `/Users/preview/.local/bin/${parent.app.id}`
            : `/Applications/${parent?.app.name ?? "Tool"}.app`);
      return {
        app_id: variant?.id ?? id,
        to: candidate(request.reference, 0),
        up_to_date: false,
        reinstall: false,
        method:
          request.method === "configured"
            ? parent?.app.source === "github"
              ? parent.app.install.method
              : "source"
            : request.method,
        steps: [
          `Checkout ${request.reference}`,
          `Install managed artifact → ${destination}`,
          ...(scenario() === "long-plan"
            ? Array.from(
                { length: 12 },
                (_, i) =>
                  `Step ${i + 1}: verify source artifact under /Users/preview/Source/checkouts/revisions/${request.reference}/build/output/Preview.app before replacing the managed destination`,
              )
            : []),
        ],
      };
    }
    case "install_revision": {
      if (!view) throw { code: "not_found", message: "Subscription missing" };
      const request = args?.request as RevisionRequest;
      validateReleaseTag(request);
      run(view, request);
      return null;
    }
    case "install_variant": {
      const parentId = typeof args?.parentId === "string" ? args.parentId : "";
      const parent = apps.find((item) => item.app.id === parentId);
      const request = args?.request as VariantRequest;
      validateReleaseTag(request.revision);
      if (!parent || parent.app.source !== "github")
        throw { code: "not_found", message: "Parent subscription missing" };
      if (apps.some((item) => item.app.id === request.id))
        throw {
          code: "app_exists",
          message: "This independent installation ID is already tracked.",
        };
      const path =
        parent.app.kind === "app"
          ? `/Applications/Headway Versions/${request.id}.app`
          : `/Users/preview/.local/bin/${request.id}`;
      const install =
        request.revision.method === "asset"
          ? {
              ...(request.revision.release_asset ?? {
                method: "asset" as const,
              }),
              dest: path,
            }
          : {
              ...(request.revision.source_build ??
                (parent.app.install.method === "source"
                  ? parent.app.install
                  : { method: "source" as const, build: "make" })),
              dest: path,
            };
      const child: AppView = {
        app: {
          ...parent.app,
          id: request.id,
          name: request.name || request.id,
          auto_update: false,
          update_policy: {
            mode: "notify",
            max_steps: 1,
            pause_on_failure: true,
          },
          track: {
            mode: "commit",
            branch:
              request.revision.reference.split("@").at(-1) ??
              request.revision.reference,
          },
          install,
          variant: {
            parent_id: parentId,
            reference:
              request.revision.reference.split("@").at(-1) ??
              request.revision.reference,
            release_tag: request.revision.release_tag ?? null,
            branch:
              parent.app.track.mode === "commit"
                ? parent.app.track.branch
                : null,
          },
        },
        busy: false,
        status: "unchecked",
        pending_count: 0,
      };
      setTimeout(
        () => {
          apps.push(child);
          run(child, request.revision);
          void emit(EVENTS.appsChanged);
        },
        scenario() === "variant-slow-register" ? 1500 : 250,
      );
      return null;
    }
    case "launch_variant":
      if (!view?.installed?.path)
        throw {
          code: "not_installed",
          message: "Install this independent version before launching.",
        };
      return { app_id: id, pid: 4242, path: view.installed.path };
    case "select_repository": {
      if (!view || view.app.source !== "github")
        throw { code: "not_found", message: "Subscription missing" };
      const repo = typeof args?.repo === "string" ? args.repo : "";
      if (scenario() === "repository-error")
        throw {
          code: "busy",
          message: "Repository activation is blocked by an active job.",
        };
      const current = {
        repo: view.app.repo,
        label: "Official",
        git_url: view.app.git_url,
        branch: view.app.track.mode === "commit" ? view.app.track.branch : null,
      };
      const rows = [current, ...(view.app.repositories ?? [])];
      const selected = rows.find((row) => row.repo === repo);
      if (!selected)
        throw {
          code: "invalid_input",
          message: "Repository is not configured.",
        };
      view.app = {
        ...view.app,
        auto_update: false,
        repo,
        git_url: selected.git_url,
        repositories: rows.filter(
          (row, i, all) =>
            all.findIndex((item) => item.repo === row.repo) === i,
        ),
        track:
          view.app.track.mode === "commit"
            ? { mode: "commit", branch: selected.branch }
            : view.app.track,
      };
      Object.assign(view, {
        installation: {
          presence: view.installed ? "present" : "unknown",
          path: view.installed?.path,
          baseline_matches: false,
          recorded: view.installed
            ? { repo: current.repo, installed: view.installed }
            : view.installation?.recorded,
        },
      });
      view.installed = null;
      view.last_check = null;
      view.pending_count = 0;
      view.status = "unknown";
      return view;
    }
    default:
      return undefined;
  }
}
export { seedRevisionMock } from "./mockRevisionFixtures";
