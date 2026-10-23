import { Loader2, Search } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { RepoInspection } from "@/lib/api";

export interface AppFormRepoSectionProps {
  repo: string;
  mode: "add" | "edit";
  inspection: RepoInspection | null;
  inspecting: boolean;
  inspectionError: string | null;
  onRepoChange: (value: string) => void;
  onInspect: () => void;
}

/**
 * The repository field of the add / edit dialog: an input, a button that
 * asks the backend about the repository and what it found.
 */
export function AppFormRepoSection({
  repo,
  mode,
  inspection,
  inspecting,
  inspectionError,
  onRepoChange,
  onInspect,
}: AppFormRepoSectionProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-1.5">
      <label htmlFor="app-form-repo" className="text-caption font-medium text-fg-2">
        {t("form.repository")}
      </label>

      <div className="flex items-center gap-2">
        <Input
          id="app-form-repo"
          value={repo}
          readOnly={mode === "edit"}
          placeholder={t("form.repositoryPlaceholder")}
          onChange={(event) => onRepoChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              onInspect();
            }
          }}
        />
        <Button
          variant="neutral"
          size="regular"
          onClick={onInspect}
          disabled={inspecting || mode === "edit"}
          className="shrink-0"
        >
          {inspecting ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Search className="h-4 w-4" strokeWidth={1.75} />
          )}
          {t("form.inspect")}
        </Button>
      </div>

      {inspectionError ? (
        <p className="text-caption text-danger-text">{inspectionError}</p>
      ) : null}

      {inspection ? (
        <div className="rounded-control bg-subtle p-2.5">
          {inspection.description ? (
            <p className="text-caption text-fg-2">{inspection.description}</p>
          ) : null}
          <p className="text-caption text-fg-3">
            {t("form.inspectionInfo", {
              branch: inspection.default_branch,
              release: inspection.latest_release?.tag ?? t("form.noRelease"),
            })}
          </p>
          {inspection.notes.length > 0 ? (
            <ul className="mt-1.5 list-disc space-y-0.5 ps-4 text-caption text-fg-3">
              {inspection.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
