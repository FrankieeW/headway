import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  appsApi,
  errorMessage,
  type SourceLookup,
  type RepoInspection,
  type TrackedApp,
} from "@/lib/api";
import { slugify } from "@/lib/format";

import { emptyForm, formFromApp, type AppFormState } from "./appFormModel";
export * from "./appFormModel";

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
  const [form, setForm] = useState<AppFormState>(() =>
    app ? formFromApp(app) : emptyForm(),
  );
  const [idEdited, setIdEdited] = useState(Boolean(app));
  const [inspection, setInspection] = useState<RepoInspection | null>(null);
  const [lookup, setLookup] = useState<SourceLookup | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [inspectionError, setInspectionError] = useState<string | null>(null);

  /** counts up per inspection, so a late answer can be ignored */
  const requestId = useRef(0);
  const editedIdentity = useRef({ id: Boolean(app), kind: Boolean(app) });

  const update = useCallback(
    <K extends keyof AppFormState>(key: K, value: AppFormState[K]) => {
      if (key === "kind") editedIdentity.current.kind = true;
      if (["source", "repo", "package", "cask", "distTag"].includes(key)) {
        requestId.current += 1;
        setInspection(null);
        setLookup(null);
        setInspectionError(null);
        setInspecting(false);
      }
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
        setForm((current) => ({
          ...suggestion,
          name: current.name.trim() ? current.name : suggestion.name,
          autoUpdate: current.autoUpdate,
          kind: editedIdentity.current.kind ? current.kind : suggestion.kind,
          distTag:
            current.source === "npm" ? current.distTag : suggestion.distTag,
          // an id the user typed wins over the suggested one
          id:
            editedIdentity.current.id || current.name.trim()
              ? current.id
              : suggestion.id,
        }));
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
