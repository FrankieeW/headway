import { DEFAULT_UPDATE_POLICY } from "@/lib/api";
import { describe, expect, it } from "vitest";

import {
  appFromForm,
  applyPreset,
  emptyForm,
  formFromApp,
  inspectedBinary,
  sourceRecipeNeedsBuild,
  withInstallMethod,
} from "./useAppForm";
import type { Preset, TrackedApp } from "@/lib/api";

const cliApp: TrackedApp = {
  source: "github",
  id: "my-tool",
  name: "My Tool",
  repo: "owner/my-tool",
  kind: "cli",
  auto_update: true,
  update_policy: DEFAULT_UPDATE_POLICY,
  git_url: null,
  track: { mode: "commit", branch: "main" },
  install: {
    method: "source",
    storage: { keep: true, root: null },
    verify_command: null,
    build: "cargo build --release",
    workdir: null,
    artifact: "target/release/{bin}",
    dest: null,
  },
};

const releaseApp: TrackedApp = {
  source: "github",
  id: "some-app",
  name: "Some App",
  repo: "owner/some-app",
  kind: "app",
  auto_update: false,
  update_policy: DEFAULT_UPDATE_POLICY,
  git_url: "git@github.com:owner/some-app.git",
  track: { mode: "release", prerelease: true, tag_filter: "^v" },
  install: {
    method: "asset",
    verify_command: null,
    pattern: "*.dmg",
    binary: "Some App.app",
    dest: "/Applications",
  },
};

describe("formFromApp", () => {
  it("reads a commit app", () => {
    const form = formFromApp(cliApp);

    expect(form.repo).toBe("owner/my-tool");
    expect(form.name).toBe("My Tool");
    expect(form.id).toBe("my-tool");
    expect(form.kind).toBe("cli");
    expect(form.trackMode).toBe("commit");
    expect(form.branch).toBe("main");
    expect(form.installMethod).toBe("source");
    expect(form.build).toBe("cargo build --release");
    expect(form.artifact).toBe("target/release/{bin}");
    expect(form.autoUpdate).toBe(true);
  });

  it("turns empty optional fields into empty strings", () => {
    const form = formFromApp(cliApp);
    expect(form.workdir).toBe("");
    expect(form.dest).toBe("");
  });

  it("reads a release app", () => {
    const form = formFromApp(releaseApp);

    expect(form.trackMode).toBe("release");
    expect(form.prerelease).toBe(true);
    expect(form.tagFilter).toBe("^v");
    expect(form.installMethod).toBe("asset");
    expect(form.pattern).toBe("*.dmg");
    expect(form.binary).toBe("Some App.app");
    expect(form.dest).toBe("/Applications");
    // the source fields are not used by an asset install
    expect(form.build).toBe("");
  });
});

describe("appFromForm", () => {
  it("is the inverse of formFromApp", () => {
    expect(appFromForm(formFromApp(cliApp))).toEqual(cliApp);
    expect(appFromForm(formFromApp(releaseApp))).toEqual(releaseApp);
  });

  it("sends an empty branch as null", () => {
    const form = { ...emptyForm(), id: "x", name: "X", repo: "o/x" };
    const app = appFromForm(form);
    if (app.source !== "github") throw new Error("Expected GitHub payload");
    expect(app.track).toEqual({ mode: "commit", branch: null });
  });

  it("sends an empty tag filter as null", () => {
    const form = {
      ...emptyForm(),
      id: "x",
      name: "X",
      repo: "o/x",
      trackMode: "tag" as const,
    };
    const app = appFromForm(form);
    if (app.source !== "github") throw new Error("Expected GitHub payload");
    expect(app.track).toEqual({
      mode: "tag",
      prerelease: false,
      tag_filter: null,
    });
  });

  it("keeps a download recipe beside the source build", () => {
    const form = {
      ...formFromApp(cliApp),
      pattern: "*.tar.gz",
      binary: "my-tool",
    };
    const app = appFromForm(form);
    if (app.source !== "github") throw new Error("Expected GitHub payload");
    expect(app.install.method).toBe("source");
    expect(app.release_asset).toMatchObject({
      method: "asset",
      pattern: "*.tar.gz",
      binary: "my-tool",
    });
  });

  it("edits and clears the source recipe without changing the download default", () => {
    const form = {
      ...formFromApp(releaseApp),
      build: "make",
      artifact: "build/tool",
    };
    const added = appFromForm(form);
    if (added.source !== "github") throw new Error("Expected GitHub payload");
    expect(added.install).toEqual(releaseApp.install);
    expect(added.source_build).toMatchObject({
      build: "make",
      artifact: "build/tool",
    });
    const edited = appFromForm({
      ...formFromApp(added),
      build: "make release",
    });
    if (edited.source !== "github") throw new Error("Expected GitHub payload");
    expect(edited.install).toEqual(releaseApp.install);
    expect(edited.source_build?.build).toBe("make release");
    const cleared = appFromForm({ ...formFromApp(edited), build: " \n " });
    if (cleared.source !== "github") throw new Error("Expected GitHub payload");
    expect(cleared.install).toEqual(releaseApp.install);
    expect(cleared.source_build).toBeUndefined();
  });

  it("preserves both recipes and their verifiers when switching defaults", () => {
    let form = {
      ...formFromApp(cliApp),
      build: "make release",
      pattern: "*.zip",
      binary: "my-tool",
      verifyCommand: "check-source",
      assetVerifyCommand: "check-download",
      dest: "/tmp/my-tool",
    };
    const download = appFromForm(withInstallMethod(form, "asset"));
    if (download.source !== "github")
      throw new Error("Expected GitHub payload");
    expect(download.install).toMatchObject({
      method: "asset",
      pattern: "*.zip",
      binary: "my-tool",
      verify_command: "check-download",
      dest: "/tmp/my-tool",
    });
    expect(download.source_build).toMatchObject({
      build: "make release",
      verify_command: "check-source",
    });
    expect(download.release_asset).toBeUndefined();
    form = formFromApp(download);
    const source = appFromForm(withInstallMethod(form, "source"));
    if (source.source !== "github") throw new Error("Expected GitHub payload");
    expect(source.install).toMatchObject({
      method: "source",
      build: "make release",
      verify_command: "check-source",
      dest: "/tmp/my-tool",
    });
    expect(source.release_asset).toMatchObject({
      pattern: "*.zip",
      verify_command: "check-download",
    });
    expect(source.source_build).toBeUndefined();
  });

  it.each(["source", "asset"] as const)(
    "drops the stale duplicate of the active %s recipe",
    (method) => {
      const app = {
        ...(method === "source" ? cliApp : releaseApp),
        source_build: { method: "source" as const, build: "stale-build" },
        release_asset: { method: "asset" as const, pattern: "stale.zip" },
      };
      const saved = appFromForm(formFromApp(app));
      if (saved.source !== "github") throw new Error("Expected GitHub payload");
      expect(saved.install).toEqual(app.install);
      if (method === "source") {
        expect(saved.source_build).toBeUndefined();
        expect(saved.release_asset?.pattern).toBe("stale.zip");
      } else {
        expect(saved.release_asset).toBeUndefined();
        expect(saved.source_build?.build).toBe("stale-build");
      }
    },
  );

  it("sends empty source fields as null", () => {
    const form = {
      ...emptyForm(),
      id: "x",
      name: "X",
      repo: "o/x",
      build: "make",
    };
    const app = appFromForm(form);
    if (app.source !== "github") throw new Error("Expected GitHub payload");
    expect(app.install).toEqual({
      method: "source",
      storage: { keep: true, root: null },
      verify_command: null,
      build: "make",
      workdir: null,
      artifact: null,
      dest: null,
    });
  });

  it("trims the values", () => {
    const form = {
      ...emptyForm(),
      id: "  x  ",
      name: "  X  ",
      repo: "  o/x  ",
      branch: "  main  ",
    };
    const app = appFromForm(form);
    if (app.source !== "github") throw new Error("Expected GitHub payload");
    expect(app.id).toBe("x");
    expect(app.name).toBe("X");
    expect(app.repo).toBe("o/x");
    expect(app.track).toEqual({ mode: "commit", branch: "main" });
  });
});

describe("applyPreset", () => {
  const preset: Preset = {
    id: "cargo",
    label: "Cargo",
    kind: "cli",
    build: "cargo build --release",
    artifact: "target/release/{bin}",
    description: "Rust",
  };

  it("fills the placeholders", () => {
    const form = applyPreset(emptyForm(), preset, {
      bin: "my-tool",
      name: "My Tool",
    });

    expect(form.build).toBe("cargo build --release");
    expect(form.artifact).toBe("target/release/my-tool");
    expect(form.presetId).toBe("cargo");
    expect(form.kind).toBe("cli");
  });

  it("replaces the name placeholder", () => {
    const named: Preset = { ...preset, build: "make install {name}" };
    const form = applyPreset(emptyForm(), named, {
      bin: "my-tool",
      name: "My Tool",
    });

    expect(form.build).toBe("make install 'My Tool'");
  });

  it("quotes repository-controlled names in the build command only", () => {
    const xcode: Preset = {
      ...preset,
      build: "xcodebuild -scheme {name} build",
      artifact: "build/{name}.app",
    };
    const form = applyPreset(emptyForm(), xcode, {
      bin: "b",
      name: "$(touch pwned)",
    });

    expect(form.build).toBe("xcodebuild -scheme '$(touch pwned)' build");
    expect(form.artifact).toBe("build/$(touch pwned).app");
  });

  it("keeps the other fields", () => {
    const form = applyPreset(
      { ...emptyForm(), repo: "owner/repo", tagFilter: "^v" },
      preset,
      { bin: "b", name: "n" },
    );

    expect(form.repo).toBe("owner/repo");
    expect(form.tagFilter).toBe("^v");
  });

  it("handles a preset without an artifact", () => {
    const withoutArtifact: Preset = { ...preset, artifact: null };
    const form = applyPreset(emptyForm(), withoutArtifact, {
      bin: "b",
      name: "n",
    });

    expect(form.artifact).toBe("");
  });

  it("keeps the kind when the preset fills only the alternate build", () => {
    const form = applyPreset(formFromApp(releaseApp), preset, {
      bin: "b",
      name: "n",
    });

    expect(form.kind).toBe("app");
    expect(form.installMethod).toBe("asset");
    expect(form.build).toBe("cargo build --release");
  });
});

describe("sourceRecipeNeedsBuild", () => {
  it("flags source paths typed without a build beside a download default", () => {
    const form = formFromApp(releaseApp);

    expect(sourceRecipeNeedsBuild({ ...form, artifact: "out/tool" })).toBe(
      true,
    );
    expect(sourceRecipeNeedsBuild({ ...form, storageKeep: false })).toBe(true);
    expect(sourceRecipeNeedsBuild(form)).toBe(false);
  });

  it("treats clearing an existing alternate build as removal", () => {
    const form = formFromApp({
      ...releaseApp,
      source_build: {
        method: "source",
        build: "make",
        artifact: "out/tool",
      },
    });

    expect(sourceRecipeNeedsBuild({ ...form, build: "" })).toBe(false);
  });

  it("leaves an empty active build to backend validation", () => {
    expect(
      sourceRecipeNeedsBuild({
        ...formFromApp(cliApp),
        build: "",
      }),
    ).toBe(false);
  });
});

describe("inspectedBinary", () => {
  it("takes the first binary of the inspection", () => {
    expect(
      inspectedBinary({
        repo: "owner/repo",
        default_branch: "main",
        private: false,
        archived: false,
        html_url: "https://github.com/owner/repo",
        binaries: ["first", "second"],
        suggestion: cliApp,
        notes: [],
      }),
    ).toBe("first");
  });

  it("falls back to the repository name", () => {
    expect(
      inspectedBinary({
        repo: "owner/repo",
        default_branch: "main",
        private: false,
        archived: false,
        html_url: "https://github.com/owner/repo",
        binaries: [],
        suggestion: cliApp,
        notes: [],
      }),
    ).toBe("repo");
  });

  it("handles no inspection", () => {
    expect(inspectedBinary(null)).toBe("");
  });
});
