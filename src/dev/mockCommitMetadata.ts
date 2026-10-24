import type { CommitMetadata } from "@/lib/api";

const base = {
  checked_at: "2026-10-24T08:00:00Z",
  notes: [],
  checks_count: 0,
  ci_url: null,
  release_url: null,
  tags: [],
  version: null,
  is_release: null,
  ci_state: "unknown",
} satisfies Omit<CommitMetadata, "sha">;
const fixtures: Record<string, CommitMetadata> = {
  "2222222222222222222222222222222222222222": {
    ...base,
    sha: "2222222222222222222222222222222222222222",
    version: "0.2.1",
    version_source: "manifest",
    is_release: false,
    ci_state: "success",
    checks_count: 3,
  },
  "9f4c1e2a7b3d5f6081a2b3c4d5e6f7a8b9c0d1e2": {
    ...base,
    sha: "9f4c1e2a7b3d5f6081a2b3c4d5e6f7a8b9c0d1e2",
    version: "v0.162.0",
    version_source: "tag",
    tags: ["v0.162.0"],
    is_release: true,
    release_url: "https://github.com/zed-industries/zed/releases/tag/v0.162.0",
    ci_state: "success",
    checks_count: 4,
    ci_url: "https://github.com/zed-industries/zed/actions/runs/1",
  },
  "8e3b0d19f6a2c4e5d7b8a9c0d1e2f3a4b5c6d7e8": {
    ...base,
    sha: "8e3b0d19f6a2c4e5d7b8a9c0d1e2f3a4b5c6d7e8",
    ci_state: "pending",
    checks_count: 2,
    is_release: false,
  },
  "7d2a9c08e5f1b3d4c6a7b8c9d0e1f2a3b4c5d6e7": {
    ...base,
    sha: "7d2a9c08e5f1b3d4c6a7b8c9d0e1f2a3b4c5d6e7",
    version: "v0.162.0-rc.1",
    version_source: "tag",
    tags: ["v0.162.0-rc.1"],
    is_release: false,
    ci_state: "failure",
    checks_count: 3,
  },
};

export function mockCommitMetadata(
  args: Record<string, unknown> | undefined,
): CommitMetadata {
  const sha = typeof args?.sha === "string" ? args.sha : "";
  const scenario = new URLSearchParams(location.search).get("scenario");
  if (scenario === "metadata-error")
    throw {
      code: "github_rate_limited",
      message: "GitHub quota unavailable; refresh later.",
    };
  if (scenario === "metadata-unknown")
    return {
      ...base,
      sha,
      notes: ["Release and CI metadata could not be verified."],
    };
  if (scenario === "metadata-manifest")
    return {
      ...base,
      sha,
      version: "0.3.0",
      version_source: "manifest",
      is_release: false,
      ci_state: "success",
    };
  if (scenario === "metadata-not-run")
    return { ...base, sha, ci_state: "not_run", is_release: false };
  if (scenario === "metadata-text")
    return {
      ...base,
      sha,
      version: "<img src=x onerror=alert(1)>",
      tags: ["<literal tag>"],
      is_release: false,
      ci_state: "not_run",
    };
  return fixtures[sha] ?? { ...base, sha };
}
