import { RepositoryFields } from "./RepositoryFields";
import { AppFormInstallFields } from "./AppFormInstallFields";
import { Switch } from "@/components/ui/switch";
import { UpdatePolicyFields } from "./UpdatePolicyFields";
import { useId } from "react";
import { Field } from "./FormField";
import { useTranslation } from "react-i18next";

import { AppFormSourceSection } from "./AppFormSourceSection";
import { AppFormRepoSection } from "@/components/apps/AppFormRepoSection";
import { Input } from "@/components/ui/input";
import { SegmentedControl } from "@/components/ui/segmented-control";

import type { Preset, RepoInspection, SourceLookup } from "@/lib/api";
import type { AppFormState } from "@/components/apps/useAppForm";

/** Radix Select does not allow an empty item value. */

export interface AppFormFieldsProps {
  form: AppFormState;
  mode: "add" | "edit";
  presets: Preset[] | undefined;
  inspection: RepoInspection | null;
  lookup: SourceLookup | null;
  inspecting: boolean;
  inspectionError: string | null;
  /** where a CLI binary goes when the destination is left empty */
  destPlaceholder: string;
  onInspect: () => void;
  onUpdate: <K extends keyof AppFormState>(
    key: K,
    value: AppFormState[K],
  ) => void;
  onSetName: (value: string) => void;
  onSetId: (value: string) => void;
  onApplyPreset: (preset: Preset) => void;
}

/**
 * The fields of the add / edit dialog.
 */
export function AppFormFields({
  form,
  mode,
  presets,
  inspection,
  lookup,
  inspecting,
  inspectionError,
  destPlaceholder,
  onInspect,
  onUpdate,
  onSetName,
  onSetId,
  onApplyPreset,
}: AppFormFieldsProps) {
  const { t } = useTranslation();
  const nameId = useId();
  const idId = useId();
  const branchId = useId();
  const tagFilterId = useId();

  return (
    <div className="space-y-5">
      <AppFormSourceSection
        form={form}
        onUpdate={onUpdate}
        onInspect={onInspect}
        inspecting={inspecting}
        lookup={lookup}
        error={inspectionError}
      />
      {form.source === "github" ? (
        <>
          <AppFormRepoSection
            repo={form.repo}
            mode={mode}
            inspection={inspection}
            inspecting={inspecting}
            inspectionError={inspectionError}
            onRepoChange={(value) => onUpdate("repo", value)}
            onInspect={onInspect}
          />
        </>
      ) : null}
      {/* 2 — identity */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <Field label={t("form.name")}>
          <Input
            aria-label={t("form.name")}
            id={nameId}
            value={form.name}
            onChange={(event) => onSetName(event.target.value)}
            placeholder={t("form.namePlaceholder")}
          />
        </Field>

        <Field label={t("form.id")}>
          <Input
            aria-label={t("form.id")}
            id={idId}
            maxLength={64}
            value={form.id}
            readOnly={mode === "edit"}
            onChange={(event) => onSetId(event.target.value)}
            placeholder="my-tool"
            className="font-mono"
          />
        </Field>
      </div>

      <Field label={t("form.kind")}>
        <SegmentedControl
          size="sm"
          aria-label={t("form.kind")}
          value={form.kind}
          onValueChange={(value) =>
            onUpdate("kind", value as AppFormState["kind"])
          }
          items={[
            { value: "cli", label: t("form.kindCli") },
            { value: "app", label: t("form.kindApp") },
          ]}
        />
      </Field>

      {form.source === "github" ? (
        <>
          {/* 3 — what to follow */}
          <Field label={t("form.follow")}>
            <SegmentedControl
              size="sm"
              aria-label={t("form.follow")}
              value={form.trackMode}
              onValueChange={(value) => onUpdate("trackMode", value)}
              items={[
                { value: "commit", label: t("form.followCommit") },
                { value: "release", label: t("form.followRelease") },
                { value: "tag", label: t("form.followTag") },
              ]}
            />
          </Field>

          {form.trackMode === "commit" ? (
            <Field label={t("form.branch")}>
              <Input
                id={branchId}
                value={form.branch}
                onChange={(event) => onUpdate("branch", event.target.value)}
                placeholder={
                  inspection?.default_branch ?? t("form.branchPlaceholder")
                }
              />
            </Field>
          ) : (
            <div className="space-y-3">
              <label className="flex items-center justify-between gap-3">
                <span className="text-caption text-fg-2">
                  {t("form.includePrerelease")}
                </span>
                <Switch
                  size="sm"
                  checked={form.prerelease}
                  onCheckedChange={(value) => onUpdate("prerelease", value)}
                  aria-label={t("form.includePrerelease")}
                />
              </label>

              <Field label={t("form.tagFilter")}>
                <Input
                  id={tagFilterId}
                  value={form.tagFilter}
                  onChange={(event) =>
                    onUpdate("tagFilter", event.target.value)
                  }
                  placeholder="^cli-v"
                  className="font-mono"
                />
              </Field>
            </div>
          )}

          <AppFormInstallFields
            form={form}
            presets={presets}
            inspection={inspection}
            destPlaceholder={destPlaceholder}
            onUpdate={onUpdate}
            onApplyPreset={onApplyPreset}
          />
        </>
      ) : null}
      {/* 5 — automation */}
      {form.source === "github" ? (
        <RepositoryFields
          rows={form.repositories}
          onChange={(rows) => onUpdate("repositories", rows)}
        />
      ) : null}
      <UpdatePolicyFields form={form} onUpdate={onUpdate} />
    </div>
  );
}
