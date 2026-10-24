import type {
  AssetInstall,
  RevisionOptions,
  RevisionRequest,
  SourceInstall,
  TrackedApp,
} from "@/lib/api";

export interface RevisionDraft {
  readonly method: RevisionRequest["method"];
  readonly source: SourceInstall;
  readonly asset: AssetInstall;
  readonly separate: boolean;
  readonly id: string;
  readonly name: string;
}
export function revisionDraft(
  app: TrackedApp,
  options: RevisionOptions,
): RevisionDraft {
  const source = options.source_build ?? { method: "source", build: "" };
  const configured =
    app.source === "github" &&
    (app.install.method === "source" || options.can_download);
  return {
    method: configured ? "configured" : "source",
    source: {
      ...source,
      storage: source.storage ?? { keep: true, root: null },
    },
    asset: options.release_asset ?? { method: "asset" },
    separate: false,
    id: `${app.id.slice(0, 56)}-${options.commit.slice(0, 7)}`,
    name: `${app.name} ${options.commit.slice(0, 7)}`,
  };
}
export function requestFromDraft(
  draft: RevisionDraft,
  commit: string,
  releaseTag?: string | null,
): RevisionRequest {
  const target = {
    reference: commit,
    ...(releaseTag ? { release_tag: releaseTag } : {}),
  };
  switch (draft.method) {
    case "configured":
      return { ...target, method: "configured" };
    case "asset":
      return { ...target, method: "asset", release_asset: draft.asset };
    case "source":
      return {
        ...target,
        method: "source",
        source_build: {
          ...draft.source,
          build: draft.source.build.trim(),
          workdir: draft.source.workdir?.trim() || null,
          artifact: draft.source.artifact?.trim() || null,
          storage: {
            keep: draft.source.storage?.keep ?? true,
            root: draft.source.storage?.root?.trim() || null,
          },
        },
      };
  }
}
