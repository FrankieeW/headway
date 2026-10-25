import type { RepositoryMatch, SourceMatches } from "@/lib/api";

export function mockRepository(repo: string, cask: boolean): RepositoryMatch {
  const html_url = `https://github.com/${repo}`;
  return {
    repo,
    html_url,
    confidence: "high",
    evidence: [
      {
        kind: cask ? "download_url" : "source_url",
        value: cask
          ? `${html_url}/releases/download/v2.0/app.dmg`
          : `${html_url}/archive/refs/tags/v2.0.tar.gz`,
      },
    ],
    description: null,
    stars: null,
    tracked_by: [],
  };
}

export function handleMatches(
  cmd: string,
  args: Record<string, unknown> | undefined,
): SourceMatches | undefined {
  if (cmd !== "match_source") return undefined;
  const input = typeof args?.input === "string" ? args.input.trim() : "";
  const brew = /^(brew:|cask:|https:\/\/formulae\.brew\.sh\/)/.test(input);
  const cask =
    input.startsWith("cask:") || input.includes("formulae.brew.sh/cask/");
  const value = input
    .replace(
      /^(brew:|cask:|https:\/\/formulae\.brew\.sh\/(formula|cask)\/|https:\/\/github\.com\/)/,
      "",
    )
    .replace(/\/$/, "");
  const name = value.split("/").at(-1) ?? "tool";
  const repo = brew
    ? name === "zed"
      ? "zed-industries/zed"
      : `acme/${name}`
    : value;
  const repository = mockRepository(repo, brew ? cask : true);
  const scenario = new URLSearchParams(location.search).get("scenario");
  if (scenario === "match-error")
    throw {
      code: "tool_missing",
      message: "Homebrew is unavailable in this preview scenario",
    };
  return {
    query: brew ? `brew ${cask ? "cask " : ""}${value}` : repo,
    from: brew ? "brew" : "github",
    repositories: brew && scenario !== "empty" ? [repository] : [],
    packages:
      !brew && scenario !== "empty"
        ? [
            {
              package: name,
              cask: true,
              name,
              description: null,
              homepage: null,
              installed: "2.0.0",
              repo,
              confidence: "high",
              evidence: repository.evidence,
              tracked_by: [],
            },
          ]
        : [],
    notes: [],
    errors:
      scenario === "error"
        ? [
            {
              source: "github",
              error: {
                code: "rate_limited",
                message:
                  "GitHub search is rate limited in this preview scenario",
              },
            },
          ]
        : [],
  };
}
