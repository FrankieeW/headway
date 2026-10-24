import { DEFAULT_UPDATE_POLICY, type UpdatePolicy } from "@/lib/api";
import type {
  AppKind,
  Install,
  Preset,
  RepoInspection,
  Track,
  TrackMode,
  TrackedApp,
} from "@/lib/api";
/** Everything the form dialog edits. */
export interface AppFormState {
  source: TrackedApp["source"];
  package: string;
  distTag: string;
  cask: boolean;
  updateCommand: string;
  versionCommand: string;
  latestCommand: string;
  repo: string;
  name: string;
  id: string;
  kind: AppKind;
  trackMode: TrackMode;
  /** branch to follow — empty means the default branch */
  branch: string;
  prerelease: boolean;
  tagFilter: string;
  installMethod: "source" | "asset";
  presetId: string;
  build: string;
  workdir: string;
  artifact: string;
  dest: string;
  pattern: string;
  binary: string;
  /** clone URL override — kept so editing does not drop it */
  gitUrl: string;
  autoUpdate: boolean;
  updatePolicy: UpdatePolicy;
  verifyCommand: string;
}

/** A fresh form for a new repository. */
export function emptyForm(): AppFormState {
  return {
    source: "github",
    package: "",
    distTag: "latest",
    cask: false,
    updateCommand: "",
    versionCommand: "",
    latestCommand: "",
    repo: "",
    name: "",
    id: "",
    kind: "cli",
    trackMode: "commit",
    branch: "",
    prerelease: false,
    tagFilter: "",
    installMethod: "source",
    presetId: "",
    build: "",
    workdir: "",
    artifact: "",
    dest: "",
    pattern: "",
    binary: "",
    gitUrl: "",
    autoUpdate: false,
    updatePolicy: DEFAULT_UPDATE_POLICY,
    verifyCommand: "",
  };
}

/** Read an existing app into the form. */
export function formFromApp(app: TrackedApp): AppFormState {
  const common = {
    ...emptyForm(),
    source: app.source,
    id: app.id,
    name: app.name,
    kind: app.kind,
    autoUpdate: app.auto_update,
    updatePolicy: app.update_policy ?? DEFAULT_UPDATE_POLICY,
  };
  switch (app.source) {
    case "npm":
      return { ...common, package: app.package, distTag: app.dist_tag };
    case "brew":
      return { ...common, package: app.package, cask: app.cask };
    case "script":
      return {
        ...common,
        updateCommand: app.update_command,
        versionCommand: app.version_command ?? "",
        latestCommand: app.latest_command ?? "",
      };
    case "github":
      break;
  }
  const track = app.track;
  const install = app.install;

  return {
    ...common,
    repo: app.repo,
    name: app.name,
    id: app.id,
    kind: app.kind,
    trackMode: track.mode,
    branch: track.mode === "commit" ? (track.branch ?? "") : "",
    prerelease: track.mode === "commit" ? false : track.prerelease,
    tagFilter: track.mode === "commit" ? "" : (track.tag_filter ?? "") || "",
    installMethod: install.method,
    presetId: "",
    build: install.method === "source" ? install.build : "",
    workdir: install.method === "source" ? (install.workdir ?? "") || "" : "",
    artifact: install.method === "source" ? (install.artifact ?? "") || "" : "",
    verifyCommand: install.verify_command ?? "",
    dest: install.dest ?? "",
    pattern: install.method === "asset" ? (install.pattern ?? "") || "" : "",
    binary: install.method === "asset" ? (install.binary ?? "") || "" : "",
    gitUrl: app.git_url ?? "",
    autoUpdate: app.auto_update,
    updatePolicy: app.update_policy ?? DEFAULT_UPDATE_POLICY,
  };
}

/**
 * Read the form back into the payload the backend expects.
 *
 * Empty optional fields become `null` — that is how the backend tells
 * "not set" from "set to the empty string".
 */
export function appFromForm(form: AppFormState): TrackedApp {
  const common = {
    id: form.id.trim(),
    name: form.name.trim(),
    kind: form.kind,
    auto_update: form.updatePolicy.mode === "notify" ? false : form.autoUpdate,
    update_policy: form.updatePolicy,
  };
  switch (form.source) {
    case "npm":
      return {
        ...common,
        source: "npm",
        package: form.package.trim(),
        dist_tag: form.distTag.trim() || "latest",
      };
    case "brew":
      return {
        ...common,
        source: "brew",
        package: form.package.trim(),
        cask: form.cask,
      };
    case "script":
      return {
        ...common,
        source: "script",
        update_command: form.updateCommand.trim(),
        version_command: form.versionCommand.trim() || null,
        latest_command: form.latestCommand.trim() || null,
      };
    case "github":
      break;
  }
  const track: Track =
    form.trackMode === "commit"
      ? { mode: "commit", branch: form.branch.trim() || null }
      : form.trackMode === "release"
        ? {
            mode: "release",
            prerelease: form.prerelease,
            tag_filter: form.tagFilter.trim() || null,
          }
        : {
            mode: "tag",
            prerelease: form.prerelease,
            tag_filter: form.tagFilter.trim() || null,
          };

  const install: Install =
    form.installMethod === "asset"
      ? {
          method: "asset",
          verify_command: form.verifyCommand.trim() || null,
          pattern: form.pattern.trim() || null,
          binary: form.binary.trim() || null,
          dest: form.dest.trim() || null,
        }
      : {
          method: "source",
          verify_command: form.verifyCommand.trim() || null,
          build: form.build.trim(),
          workdir: form.workdir.trim() || null,
          artifact: form.artifact.trim() || null,
          dest: form.dest.trim() || null,
        };

  return {
    source: "github",
    id: form.id.trim(),
    name: form.name.trim(),
    repo: form.repo.trim(),
    kind: form.kind,
    auto_update: form.updatePolicy.mode === "notify" ? false : form.autoUpdate,
    update_policy: form.updatePolicy,
    git_url: form.gitUrl.trim() || null,
    track,
    install,
  };
}

/**
 * Quote a value for a POSIX shell unless it only has safe characters
 * (same rule as `shell_quote` in headway-core).
 */
export function shellQuote(value: string): string {
  if (value !== "" && /^[A-Za-z0-9\-_./:=@+,%^]+$/.test(value)) return value;
  return `'${value.replace(/'/g, "'\\''")}'`;
}

/**
 * Fill the build command and the artifact from a preset.
 *
 * `{bin}` becomes the binary the repository builds (or the repository
 * name), `{name}` becomes the display name of the app. In the build command
 * the values are shell-quoted: they come from repository file names.
 */
export function applyPreset(
  form: AppFormState,
  preset: Preset,
  context: { bin: string; name: string },
): AppFormState {
  const fill = (
    value: string | null | undefined,
    quote: (part: string) => string,
  ): string =>
    (value ?? "")
      .replace(/\{bin\}/g, quote(context.bin))
      .replace(/\{name\}/g, quote(context.name));

  return {
    ...form,
    presetId: preset.id,
    kind: preset.kind,
    build: fill(preset.build, shellQuote),
    artifact: fill(preset.artifact, (part) => part),
  };
}

/** The binary name the repository builds, falling back to the repo name. */
export function inspectedBinary(inspection: RepoInspection | null): string {
  if (!inspection) return "";
  const first = inspection.binaries?.[0];
  if (first) return first;
  const parts = inspection.repo.split("/");
  return parts[parts.length - 1] ?? "";
}
