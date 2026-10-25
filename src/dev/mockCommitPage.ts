import type { CiState, CommitFilter, CommitPage } from "@/lib/api";
import { mockCommitMetadata } from "./mockCommitMetadata";

function readFilter(value: unknown): CommitFilter {
  if (typeof value !== "object" || value === null) return {};
  const ci = "ci" in value ? value.ci : null;
  const states: readonly CiState[] = [
    "success",
    "failure",
    "pending",
    "not_run",
    "unknown",
  ];
  return {
    releases_only: "releases_only" in value && value.releases_only === true,
    tag: "tag" in value && typeof value.tag === "string" ? value.tag : null,
    ci: states.find((state) => state === ci) ?? null,
  };
}

/** Preview the same page-local filtering and metadata as the backend. */
export function filterCommitPage(page: CommitPage, value: unknown): CommitPage {
  const filter = readFilter(value);
  let pattern: RegExp | undefined;
  try {
    pattern = filter.tag ? new RegExp(filter.tag) : undefined;
  } catch (error) {
    if (!(error instanceof SyntaxError)) throw error;
    throw { code: "invalid_input", message: error.message };
  }
  const annotated = page.commits.map((candidate) => {
    const metadata = mockCommitMetadata({ sha: candidate.sha ?? candidate.id });
    return {
      ...candidate,
      tags: metadata.tags,
      release_tag: metadata.is_release ? metadata.tags[0] : null,
      release_url: metadata.release_url,
      can_download: metadata.is_release === true,
      ci_state: filter.ci ? metadata.ci_state : null,
    };
  });
  const commits = annotated.filter(
    (entry) =>
      (!filter.releases_only || entry.release_tag) &&
      (!pattern || entry.tags.some((tag) => pattern.test(tag))) &&
      (!filter.ci || entry.ci_state === filter.ci),
  );
  return {
    ...page,
    commits,
    filter,
    filtered_out: annotated.length - commits.length,
  };
}
