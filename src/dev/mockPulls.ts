import type {
  AppView,
  InstallationDetection,
  PullRequest,
  PullRequestPage,
  RevisionOptions,
} from "@/lib/api";

export const mockPullSha = "c".repeat(40);
export function mockPull(repo: string, number = 7): PullRequest {
  return {
    number,
    title:
      "修复多仓库安装来源识别 · Preserve installation provenance across forks",
    html_url: `https://github.com/${repo}/pull/${number}`,
    state: "open",
    draft: false,
    head: {
      ref: "fix/install-detection",
      sha: mockPullSha,
      repo: { full_name: "contributor/headway" },
    },
    base: { ref: "main", sha: "a".repeat(40), repo: { full_name: repo } },
  };
}
export function pullOptions(
  view: AppView,
  reference: string,
): RevisionOptions | null {
  if (!reference.startsWith("pr:") || view.app.source !== "github") return null;
  if (new URLSearchParams(location.search).get("scenario") === "pr-moved")
    throw {
      code: "invalid_input",
      message: "PR head changed since review; inspect the PR again.",
    };
  const pull = mockPull(
    view.app.repo,
    Number(reference.slice(3).split("@")[0]),
  );
  return {
    reference: `pr:${pull.number}@${pull.head.sha}`,
    commit: pull.head.sha,
    candidate: {
      id: pull.head.sha,
      sha: pull.head.sha,
      title: pull.title,
      prerelease: false,
    },
    release_tag: null,
    release_url: null,
    can_download: false,
    release_asset: null,
    source_build:
      view.app.install.method === "source"
        ? view.app.install
        : (view.app.source_build ?? null),
    pull_request: pull,
  };
}
export function handleInstallationOrigins(
  cmd: string,
  args: Record<string, unknown> | undefined,
  apps: AppView[],
): unknown {
  const view = apps.find((item) => item.app.id === args?.id);
  if (!view || view.app.source !== "github") return undefined;
  const scenario = new URLSearchParams(location.search).get("scenario");
  switch (cmd) {
    case "detect_installation":
      return (
        view.installation ??
        ({
          presence:
            scenario === "installation-invalid"
              ? "invalid"
              : view.installed
                ? "present"
                : "unknown",
          path: view.installed?.path ?? view.app.install.dest,
          baseline_matches: Boolean(view.installed),
          recorded: view.installed
            ? { repo: view.app.repo, installed: view.installed }
            : null,
        } satisfies InstallationDetection)
      );
    case "list_pull_requests": {
      if (scenario === "prs-error")
        throw {
          code: "rate_limited",
          message: "GitHub rate limit reached; retry later.",
        };
      const page = typeof args?.page === "number" ? args.page : 1;
      return {
        repo: view.app.repo,
        page,
        per_page: 20,
        has_more: false,
        pull_requests:
          scenario === "prs-empty" ? [] : [mockPull(view.app.repo)],
      } satisfies PullRequestPage;
    }
    case "refresh_repositories": {
      view.app = {
        ...view.app,
        repositories: [
          ...(view.app.repositories ?? []),
          {
            repo: "upstream/headway",
            label: "upstream/headway",
            branch: "main",
          },
        ].filter(
          (row, index, all) =>
            all.findIndex((other) => other.repo === row.repo) === index,
        ),
      };
      return structuredClone(view);
    }
    default:
      return undefined;
  }
}
