import { useId } from "react";
import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";

import { AppFormRepoSection } from "@/components/apps/AppFormRepoSection";
import { Input } from "@/components/ui/input";
import { SegmentedControl } from "@/components/ui/segmented-control";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import type { Preset, RepoInspection } from "@/lib/api";
import type { AppFormState } from "@/components/apps/useAppForm";

/** Radix Select does not allow an empty item value. */
const NO_PRESET = "__none__";

export interface AppFormFieldsProps {
  form: AppFormState;
  mode: "add" | "edit";
  presets: Preset[] | undefined;
  inspection: RepoInspection | null;
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

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2">
        <span className="text-caption font-medium text-fg-2">{label}</span>
        {hint ? (
          <span className="flex items-center gap-1 text-caption text-fg-3">
            <Info className="h-3 w-3 shrink-0" strokeWidth={1.75} />
            {hint}
          </span>
        ) : null}
      </div>
      {children}
    </div>
  );
}

/**
 * The fields of the add / edit dialog.
 */
export function AppFormFields({
  form,
  mode,
  presets,
  inspection,
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
  const buildId = useId();
  const workdirId = useId();
  const artifactId = useId();
  const destId = useId();
  const patternId = useId();
  const binaryId = useId();

  const isRelease = form.trackMode === "release";
  const canUseAsset = isRelease;
  const installMethod = canUseAsset ? form.installMethod : "source";

  return (
    <div className="space-y-5">
      {/* 1 — repository */}
      <AppFormRepoSection
        repo={form.repo}
        mode={mode}
        inspection={inspection}
        inspecting={inspecting}
        inspectionError={inspectionError}
        onRepoChange={(value) => onUpdate("repo", value)}
        onInspect={onInspect}
      />

      {/* 2 — identity */}
      <div className="grid grid-cols-2 gap-3">
        <Field label={t("form.name")}>
          <Input
            id={nameId}
            value={form.name}
            onChange={(event) => onSetName(event.target.value)}
            placeholder={t("form.namePlaceholder")}
          />
        </Field>

        <Field label={t("form.id")}>
          <Input
            id={idId}
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
          onValueChange={(value) => onUpdate("kind", value as AppFormState["kind"])}
          items={[
            { value: "cli", label: t("form.kindCli") },
            { value: "app", label: t("form.kindApp") },
          ]}
        />
      </Field>

      {/* 3 — what to follow */}
      <Field label={t("form.follow")}>
        <SegmentedControl
          size="sm"
          aria-label={t("form.follow")}
          value={form.trackMode}
          onValueChange={(value) => {
            const mode = value as AppFormState["trackMode"];
            onUpdate("trackMode", mode);
            // release assets only exist for releases
            if (mode !== "release" && form.installMethod === "asset") {
              onUpdate("installMethod", "source");
            }
            // following releases that ship a macOS asset: download it by default
            const assetKind = inspection?.latest_release?.suggested_kind;
            if (mode === "release" && assetKind) {
              onUpdate("installMethod", "asset");
              onUpdate("kind", assetKind);
            }
          }}
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
              onChange={(event) => onUpdate("tagFilter", event.target.value)}
              placeholder="^cli-v"
              className="font-mono"
            />
          </Field>
        </div>
      )}

      {/* 4 — how to install */}
      <Field label={t("form.install")}>
        <SegmentedControl
          size="sm"
          aria-label={t("form.install")}
          value={installMethod}
          onValueChange={(value) =>
            onUpdate("installMethod", value as AppFormState["installMethod"])
          }
          items={[
            { value: "source", label: t("form.installSource") },
            {
              value: "asset",
              label: t("form.installAsset"),
              disabled: !canUseAsset,
            },
          ]}
        />
        {!canUseAsset ? (
          <p className="text-caption text-fg-3">{t("form.assetNeedsRelease")}</p>
        ) : null}
      </Field>

      {installMethod === "source" ? (
        <div className="space-y-3">
          <Field label={t("form.preset")}>
            <Select
              value={form.presetId || NO_PRESET}
              onValueChange={(value) => {
                if (value === NO_PRESET) {
                  onUpdate("presetId", "");
                  return;
                }
                const preset = presets?.find((entry) => entry.id === value);
                if (preset) onApplyPreset(preset);
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder={t("form.presetNone")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_PRESET}>{t("form.presetNone")}</SelectItem>
                {presets?.map((preset) => (
                  <SelectItem key={preset.id} value={preset.id}>
                    {preset.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>

          <Field label={t("form.buildCommand")}>
            <Textarea
              id={buildId}
              value={form.build}
              onChange={(event) => onUpdate("build", event.target.value)}
              placeholder="cargo build --release"
              className="min-h-[72px] font-mono"
              spellCheck={false}
            />
          </Field>

          <div className="grid grid-cols-2 gap-3">
            <Field label={t("form.workdir")}>
              <Input
                id={workdirId}
                value={form.workdir}
                onChange={(event) => onUpdate("workdir", event.target.value)}
                placeholder={t("form.workdirPlaceholder")}
                className="font-mono"
              />
            </Field>

            <Field label={t("form.artifact")} hint={t("form.artifactHelp")}>
              <Input
                id={artifactId}
                value={form.artifact}
                onChange={(event) => onUpdate("artifact", event.target.value)}
                placeholder="target/release/{bin}"
                className="font-mono"
              />
            </Field>
          </div>

          <Field label={t("form.dest")}>
            <Input
              id={destId}
              value={form.dest}
              onChange={(event) => onUpdate("dest", event.target.value)}
              placeholder={destPlaceholder}
              className="font-mono"
            />
          </Field>
        </div>
      ) : (
        <div className="space-y-3">
          <Field label={t("form.assetPattern")}>
            <Input
              id={patternId}
              value={form.pattern}
              onChange={(event) => onUpdate("pattern", event.target.value)}
              placeholder={t("form.assetPatternPlaceholder")}
              className="font-mono"
            />
          </Field>

          {inspection?.latest_release?.suggested_asset ? (
            <p className="text-caption text-fg-3">
              {t("form.suggestedAsset", {
                asset: inspection.latest_release.suggested_asset,
              })}
            </p>
          ) : null}

          <div className="grid grid-cols-2 gap-3">
            <Field label={t("form.binary")}>
              <Input
                id={binaryId}
                value={form.binary}
                onChange={(event) => onUpdate("binary", event.target.value)}
                placeholder={t("form.binaryPlaceholder")}
                className="font-mono"
              />
            </Field>

            <Field label={t("form.dest")}>
              <Input
                id={destId}
                value={form.dest}
                onChange={(event) => onUpdate("dest", event.target.value)}
                placeholder={destPlaceholder}
                className="font-mono"
              />
            </Field>
          </div>
        </div>
      )}

      {/* 5 — automation */}
      <label className="flex items-center justify-between gap-3 border-t border-border pt-4">
        <span className="text-body text-fg-1">{t("form.autoUpdate")}</span>
        <Switch
          checked={form.autoUpdate}
          onCheckedChange={(value) => onUpdate("autoUpdate", value)}
          aria-label={t("form.autoUpdate")}
        />
      </label>
    </div>
  );
}
