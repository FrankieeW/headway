import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  appsApi,
  errorMessage,
  type AppKind,
  type Install,
  type Preset,
  type RepoInspection,
  type Track,
  type TrackMode,
  type TrackedApp,
} from "@/lib/api";
import { slugify } from "@/lib/format";

/** Everything the form dialog edits. */
export interface AppFormState {
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
}

/** A fresh form for a new repository. */
export function emptyForm(): AppFormState {
  return {
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
  };
}

/** Read an existing app into the form. */
export function formFromApp(app: TrackedApp): AppFormState {
  const track = app.track;
  const install = app.install;

  return {
    repo: app.repo,
    name: app.name,
    id: app.id,
    kind: app.kind,
    trackMode: track.mode,
    branch: track.mode === "commit" ? (track.branch ?? "") : "",
    prerelease: track.mode === "commit" ? false : track.prerelease,
    tagFilter:
      track.mode === "commit" ? "" : (track.tag_filter ?? "") || "",
    installMethod: install.method,
    presetId: "",
    build: install.method === "source" ? install.build : "",
    workdir: install.method === "source" ? (install.workdir ?? "") || "" : "",
    artifact: install.method === "source" ? (install.artifact ?? "") || "" : "",
    dest: install.dest ?? "",
    pattern: install.method === "asset" ? (install.pattern ?? "") || "" : "",
    binary: install.method === "asset" ? (install.binary ?? "") || "" : "",
    gitUrl: app.git_url ?? "",
    autoUpdate: app.auto_update,
  };
}

/**
 * Read the form back into the payload the backend expects.
 *
 * Empty optional fields become `null` — that is how the backend tells
 * "not set" from "set to the empty string".
 */
export function appFromForm(form: AppFormState): TrackedApp {
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
          pattern: form.pattern.trim() || null,
          binary: form.binary.trim() || null,
          dest: form.dest.trim() || null,
        }
      : {
          method: "source",
          build: form.build.trim(),
          workdir: form.workdir.trim() || null,
          artifact: form.artifact.trim() || null,
          dest: form.dest.trim() || null,
        };

  return {
    id: form.id.trim(),
    name: form.name.trim(),
    repo: form.repo.trim(),
    kind: form.kind,
    auto_update: form.autoUpdate,
    git_url: form.gitUrl.trim() || null,
    track,
    install,
  };
}

/**
 * Fill the build command and the artifact from a preset.
 *
 * `{bin}` becomes the binary the repository builds (or the repository
 * name), `{name}` becomes the display name of the app.
 */
export function applyPreset(
  form: AppFormState,
  preset: Preset,
  context: { bin: string; name: string },
): AppFormState {
  const fill = (value: string | null | undefined): string =>
    (value ?? "")
      .replace(/\{bin\}/g, context.bin)
      .replace(/\{name\}/g, context.name);

  return {
    ...form,
    presetId: preset.id,
    kind: preset.kind,
    build: fill(preset.build),
    artifact: fill(preset.artifact),
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

export interface UseAppFormResult {
  form: AppFormState;
  /** change one field */
  update: <K extends keyof AppFormState>(
    key: K,
    value: AppFormState[K],
  ) => void;
  /** replace the whole form */
  replace: (next: AppFormState) => void;
  /** change the name; keeps the id in sync until it was edited by hand */
  setName: (value: string) => void;
  /** the user typed an id — stop deriving it from the name */
  setId: (value: string) => void;
  /** did the user edit the id? */
  idEdited: boolean;
  inspection: RepoInspection | null;
  inspecting: boolean;
  inspectionError: string | null;
  /** ask the backend about a repository */
  inspect: (repo: string) => Promise<void>;
  /** start over (new app) or load an existing one (edit) */
  reset: (app?: TrackedApp) => void;
}

/**
 * The state of the add / edit dialog.
 */
export function useAppForm(app?: TrackedApp): UseAppFormResult {
  const { t } = useTranslation();
  const [form, setForm] = useState<AppFormState>(() =>
    app ? formFromApp(app) : emptyForm(),
  );
  const [idEdited, setIdEdited] = useState(Boolean(app));
  const [inspection, setInspection] = useState<RepoInspection | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [inspectionError, setInspectionError] = useState<string | null>(null);

  /** counts up per inspection, so a late answer can be ignored */
  const requestId = useRef(0);

  const update = useCallback(
    <K extends keyof AppFormState>(key: K, value: AppFormState[K]) => {
      setForm((current) => ({ ...current, [key]: value }));
    },
    [],
  );

  const replace = useCallback((next: AppFormState) => {
    setForm(next);
  }, []);

  const setName = useCallback(
    (value: string) => {
      setForm((current) => ({
        ...current,
        name: value,
        // the id follows the name until the user typed one
        id: idEdited ? current.id : slugify(value),
      }));
    },
    [idEdited],
  );

  const setId = useCallback((value: string) => {
    setIdEdited(true);
    setForm((current) => ({ ...current, id: value }));
  }, []);

  const inspect = useCallback(
    async (repo: string) => {
      const trimmed = repo.trim();
      if (!trimmed) {
        setInspectionError(t("form.repoRequired"));
        return;
      }

      // only the newest request may change the form
      const request = ++requestId.current;
      setInspecting(true);
      setInspectionError(null);
      try {
        const result = await appsApi.inspect(trimmed);
        if (request !== requestId.current) return;

        setInspection(result);
        const suggestion = formFromApp(result.suggestion);
        setForm((current) => ({
          ...suggestion,
          // an id the user typed wins over the suggested one
          id: idEdited ? current.id : suggestion.id,
        }));
      } catch (error) {
        if (request !== requestId.current) return;
        setInspection(null);
        setInspectionError(errorMessage(error));
      } finally {
        if (request === requestId.current) setInspecting(false);
      }
    },
    [t, idEdited],
  );

  const reset = useCallback((next?: TrackedApp) => {
    // a pending inspection must not fill the new form
    requestId.current += 1;
    setForm(next ? formFromApp(next) : emptyForm());
    setIdEdited(Boolean(next));
    setInspection(null);
    setInspectionError(null);
    setInspecting(false);
  }, []);

  return {
    form,
    update,
    replace,
    setName,
    setId,
    idEdited,
    inspection,
    inspecting,
    inspectionError,
    inspect,
    reset,
  };
}
