import { describe, expect, it } from "vitest";
import { appFromForm, formFromApp } from "./appFormModel";
import { requestFromDraft, revisionDraft } from "./revisionDraft";
import {
  withPolicyDefaults,
  type RevisionOptions,
  type TrackedApp,
} from "@/lib/api";

const sha = "a".repeat(40);
const app: TrackedApp = {
  source: "github",
  id: "tool",
  name: "Tool",
  kind: "app",
  auto_update: false,
  repo: "official/tool",
  git_url: "ssh://official/tool",
  track: { mode: "commit", branch: "main" },
  install: {
    method: "source",
    build: "make",
    artifact: "out/Tool.app",
    storage: { keep: false, root: "~/Sources" },
  },
  repositories: [
    {
      repo: "fork/tool",
      label: "My fork",
      git_url: "ssh://fork/tool",
      branch: "develop",
    },
  ],
  release_asset: { method: "asset", pattern: "*.dmg", binary: "Tool.app" },
  variant: { parent_id: "parent", reference: sha },
};
const options: RevisionOptions = {
  reference: "v1",
  commit: sha,
  candidate: { id: sha, sha, title: "Commit", prerelease: false },
  release_tag: "v1",
  release_url: null,
  can_download: true,
  source_build: { method: "source", build: "make", artifact: "out/Tool.app" },
  release_asset: { method: "asset", pattern: "*.zip" },
};

describe("revision configuration preservation", () => {
  it("preserves flat repository, recipe, variant and storage choices during an unrelated edit", () => {
    // Given a saved app with distinct recipes, fork metadata and custom source storage.
    const form = formFromApp(app);
    // When editing its display name only.
    const saved = appFromForm({ ...form, name: "Renamed" });
    // Then all source options survive as flat JSON fields.
    expect(saved).toMatchObject({ ...app, name: "Renamed" });
    expect(saved).not.toHaveProperty("options");
  });
  it("materializes keep=true defaults for legacy source recipes at the API boundary", () => {
    // Given a legacy source installation without storage settings.
    const legacy: TrackedApp = {
      ...app,
      source: "github",
      install: { method: "source", build: "make" },
      source_build: { method: "source", build: "make other" },
    };
    // When serializing it through the shared API helper.
    const normalized = withPolicyDefaults(legacy);
    // Then both source recipes retain sources by default.
    expect(normalized).toMatchObject({
      install: { storage: { keep: true, root: null } },
      source_build: { storage: { keep: true, root: null } },
    });
  });
  it("pins source installation to the resolved SHA while retaining edited recipe and storage", () => {
    // Given options resolved from a moving tag and a custom source recipe.
    const draft = {
      ...revisionDraft(app, options),
      method: "source" as const,
      source: {
        method: "source" as const,
        build: " make custom ",
        artifact: " out/custom.app ",
        storage: { keep: false, root: " ~/Worktrees " },
      },
    };
    // When building the revision request.
    const request = requestFromDraft(
      draft,
      options.commit,
      options.release_tag,
    );
    // Then it sends an exact revision, explicit method and editable recipe, with no application-data flags.
    expect(request).toEqual({
      reference: sha,
      release_tag: "v1",
      method: "source",
      source_build: {
        method: "source",
        build: "make custom",
        artifact: "out/custom.app",
        workdir: null,
        storage: { keep: false, root: "~/Worktrees" },
      },
    });
    expect(Object.keys(request).sort()).toEqual([
      "method",
      "reference",
      "release_tag",
      "source_build",
    ]);
  });
  it("starts with an editable source recipe when an asset install has no release at the selected commit", () => {
    // Given an asset-managed app and an unreleased commit without a build recipe.
    const assetApp: TrackedApp = {
      ...app,
      source: "github",
      install: { method: "asset" },
      source_build: null,
    };
    // When resolving a draft for that commit.
    const draft = revisionDraft(assetApp, {
      ...options,
      source_build: null,
      can_download: false,
      release_tag: null,
    });
    // Then the missing command stays empty for user entry, with source retention enabled.
    expect(draft).toMatchObject({
      method: "source",
      source: { build: "", storage: { keep: true, root: null } },
    });
  });
});

it.each(["asset", "configured", "source"] as const)(
  "retains a verified old tag for the %s method without moving the target SHA",
  (method) => {
    // Given a known actual release association outside badge metadata's bounded window.
    const draft = { ...revisionDraft(app, options), method };
    // When serializing any installation method.
    const request = requestFromDraft(draft, "e".repeat(40), "v0.001.0");
    // Then both source and asset flows preserve the tag baseline while pinning the exact commit.
    expect(request.reference).toBe("e".repeat(40));
    expect(request.release_tag).toBe("v0.001.0");
  },
);

it.each(["asset", "configured", "source"] as const)(
  "omits release_tag for an unverified/unreleased %s request",
  (method) => {
    // Given no actual published release for the selected commit.
    const draft = { ...revisionDraft(app, options), method };
    // When serializing a null release association.
    const request = requestFromDraft(draft, "e".repeat(40), null);
    // Then neither a manifest version nor the selected method implies a release.
    expect(request).not.toHaveProperty("release_tag");
  },
);

it("normalizes an omitted Rust storage root without changing false or a custom path", () => {
  // Given serde output whose active source root was omitted and a custom alternate recipe.
  const saved: TrackedApp = {
    ...app,
    source: "github",
    install: { method: "source", build: "make", storage: { keep: false } },
    source_build: {
      method: "source",
      build: "make alternate",
      storage: { keep: false, root: "/Volumes/Custom Source" },
    },
  };
  // When serializing the app through the common save boundary.
  const normalized = withPolicyDefaults(saved);
  // Then missing root becomes null while explicit false/custom values are preserved.
  expect(normalized).toMatchObject({
    install: { storage: { keep: false, root: null } },
    source_build: { storage: { keep: false, root: "/Volumes/Custom Source" } },
  });
});

it("keeps the generated child ID valid for a maximum-length parent ID", () => {
  // Given a valid 64-character parent ID.
  const parent: TrackedApp = { ...app, id: "p".repeat(64) };
  // When preparing its default independent installation ID.
  const draft = revisionDraft(parent, options);
  // Then the useful base plus exact SHA suffix fits the core's 64-character limit.
  expect(draft.id).toBe(`${"p".repeat(56)}-${options.commit.slice(0, 7)}`);
  expect(draft.id).toHaveLength(64);
});
