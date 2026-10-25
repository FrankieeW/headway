import { useCallback, useRef, useState, type SetStateAction } from "react";
import { useTranslation } from "react-i18next";

import {
  appsApi,
  errorMessage,
  type SourceLookup,
  type RepoInspection,
  type TrackedApp,
} from "@/lib/api";
import { slugify } from "@/lib/format";

import {
  emptyForm,
  formFromApp,
  withInstallMethod,
  type AppFormState,
} from "./appFormModel";
export * from "./appFormModel";

function savedEdits(app?: TrackedApp): Partial<AppFormState> {
  if (!app) return {};
  const fields: Partial<AppFormState> = formFromApp(app);
  delete fields.source;
  delete fields.repo;
  delete fields.package;
  return fields;
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
  lookup: SourceLookup | null;
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
  const [form, setFormState] = useState<AppFormState>(() =>
    app ? formFromApp(app) : emptyForm(),
  );
  const currentForm = useRef(form);
  // Dirty-field bookkeeping follows update order even when React batches edits.
  const setForm = useCallback((action: SetStateAction<AppFormState>) => {
    const next =
      typeof action === "function" ? action(currentForm.current) : action;
    currentForm.current = next;
    setFormState(next);
  }, []);
  const [idEdited, setIdEdited] = useState(Boolean(app));
  const [inspection, setInspection] = useState<RepoInspection | null>(null);
  const [lookup, setLookup] = useState<SourceLookup | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [inspectionError, setInspectionError] = useState<string | null>(null);

  /** counts up per inspection, so a late answer can be ignored */
  const requestId = useRef(0);
  const editedIdentity = useRef({ id: Boolean(app), kind: Boolean(app) });
  const edits = useRef<Partial<AppFormState>>(savedEdits(app));

  const update = useCallback(
    <K extends keyof AppFormState>(key: K, value: AppFormState[K]) => {
      if (!["source", "repo", "package"].includes(key)) {
        edits.current = { ...edits.current, [key]: value };
      }
      if (key === "kind") editedIdentity.current.kind = true;
      if (["source", "repo", "package", "cask", "distTag"].includes(key)) {
        requestId.current += 1;
        setInspection(null);
        setLookup(null);
        setInspectionError(null);
        setInspecting(false);
      }
      const dropPath =
        (key === "source" && value !== currentForm.current.source) ||
        (key === "package" && value !== currentForm.current.package) ||
        (key === "cask" && value !== currentForm.current.cask);
      if (dropPath) {
        edits.current = { ...edits.current, installPath: "" };
      }
      setForm((previous) => {
        const current = dropPath ? { ...previous, installPath: "" } : previous;
        if (
          key === "installMethod" &&
          (value === "source" || value === "asset")
        ) {
          const next = withInstallMethod(current, value);
          edits.current = {
            ...edits.current,
            installMethod: next.installMethod,
            sourceBuild: next.sourceBuild,
            releaseAsset: next.releaseAsset,
            sourceVerifyCommand: next.sourceVerifyCommand,
            assetVerifyCommand: next.assetVerifyCommand,
            verifyCommand: next.verifyCommand,
          };
          return next;
        }
        if (
          key === "source" &&
          value !== "github" &&
          value !== "npm" &&
          current.updatePolicy.mode === "next"
        ) {
          const updatePolicy = {
            ...current.updatePolicy,
            mode: "latest" as const,
          };
          edits.current = { ...edits.current, updatePolicy };
          return { ...current, [key]: value, updatePolicy };
        }
        return { ...current, [key]: value };
      });
    },
    [],
  );

  const replace = useCallback((next: AppFormState) => {
    requestId.current += 1;
    edits.current = next;
    setInspecting(false);
    setInspection(null);
    setLookup(null);
    setForm(next);
  }, []);

  const setName = useCallback(
    (value: string) => {
      edits.current = {
        ...edits.current,
        name: value,
        ...(!editedIdentity.current.id ? { id: slugify(value) } : {}),
      };
      setForm((current) => ({
        ...current,
        name: value,
        // the id follows the name until the user typed one
        id: editedIdentity.current.id ? current.id : slugify(value),
      }));
    },
    [idEdited],
  );

  const setId = useCallback((value: string) => {
    edits.current = { ...edits.current, id: value };
    editedIdentity.current.id = true;
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
        const result =
          form.source === "github"
            ? await appsApi.inspect(trimmed)
            : await appsApi.lookup(
                /^(npm:|brew:|cask:|https?:)/.test(trimmed)
                  ? trimmed
                  : (form.source === "brew" && form.cask
                      ? "cask"
                      : form.source) +
                      ":" +
                      trimmed,
              );
        if (request !== requestId.current) return;

        if ("repo" in result) setInspection(result);
        else setLookup(result);
        const suggestion = formFromApp(result.suggestion);
        setForm((current) => {
          const merged = {
            ...suggestion,
            autoUpdate: current.autoUpdate,
            updatePolicy: current.updatePolicy,
            ...edits.current,
            source: current.source,
          };
          return merged;
        });
      } catch (error) {
        if (request !== requestId.current) return;
        setInspection(null);
        setInspectionError(errorMessage(error));
      } finally {
        if (request === requestId.current) setInspecting(false);
      }
    },
    [t, idEdited, form.source, form.cask],
  );

  const reset = useCallback((next?: TrackedApp) => {
    // a pending inspection must not fill the new form
    requestId.current += 1;
    edits.current = savedEdits(next);
    setForm(next ? formFromApp(next) : emptyForm());
    editedIdentity.current = { id: Boolean(next), kind: Boolean(next) };
    setIdEdited(Boolean(next));
    setInspection(null);
    setLookup(null);
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
    lookup,
    inspecting,
    inspectionError,
    inspect,
    reset,
  };
}
