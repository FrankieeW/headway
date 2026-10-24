import { describe, expect, it } from "vitest";

import {
  appFromForm,
  applyPreset,
  emptyForm,
  formFromApp,
  inspectedBinary,
} from "./useAppForm";
import type { Preset, TrackedApp } from "@/lib/api";

const cliApp: TrackedApp = {
  id: "my-tool",
  name: "My Tool",
  repo: "owner/my-tool",
  kind: "cli",
  auto_update: true,
  git_url: null,
  track: { mode: "commit", branch: "main" },
  install: {
    method: "source",
    build: "cargo build --release",
    workdir: null,
    artifact: "target/release/{bin}",
    dest: null,
  },
};

const releaseApp: TrackedApp = {
  id: "some-app",
  name: "Some App",
  repo: "owner/some-app",
  kind: "app",
  auto_update: false,
  git_url: "git@github.com:owner/some-app.git",
  track: { mode: "release", prerelease: true, tag_filter: "^v" },
  install: {
    method: "asset",
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
    expect(app.track).toEqual({ mode: "tag", prerelease: false, tag_filter: null });
  });

  it("sends empty source fields as null", () => {
    const form = {
      ...emptyForm(),
      id: "x",
      name: "X",
      repo: "o/x",
      build: "make",
    };
    const app = appFromForm(form);
    expect(app.install).toEqual({
      method: "source",
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
