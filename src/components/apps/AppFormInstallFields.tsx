import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { SegmentedControl } from "@/components/ui/segmented-control";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { BuildCandidates } from "./BuildCandidates";
import type { AppFormFieldsProps } from "./AppFormFields";
import { Field } from "./FormField";
import type { AppFormState } from "./appFormModel";
const NO_PRESET = "__none__";
export function AppFormInstallFields({
  form,
  presets,
  inspection,
  destPlaceholder,
  onUpdate,
  onApplyPreset,
}: Pick<
  AppFormFieldsProps,
  | "form"
  | "presets"
  | "inspection"
  | "destPlaceholder"
  | "onUpdate"
  | "onApplyPreset"
>) {
  const { t } = useTranslation();
  const buildId = useId();
  const workdirId = useId();
  const artifactId = useId();
  const destId = useId();
  const patternId = useId();
  const binaryId = useId();
  const canUseAsset = form.trackMode === "release";
  const installMethod = canUseAsset ? form.installMethod : "source";
  return (
    <>
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
          <p className="text-caption text-fg-3">
            {t("form.assetNeedsRelease")}
          </p>
        ) : null}
      </Field>

      {installMethod === "source" ? (
        <div className="space-y-3">
          <BuildCandidates
            key={inspection?.repo ?? ""}
            candidates={inspection?.build_candidates ?? []}
            onApply={(candidate) => {
              onUpdate("build", candidate.build);
              onUpdate("workdir", candidate.workdir ?? "");
              onUpdate("artifact", candidate.artifact ?? "");
              onUpdate("kind", candidate.kind);
              onUpdate("presetId", "");
            }}
          />
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
                <SelectItem value={NO_PRESET}>
                  {t("form.presetNone")}
                </SelectItem>
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
              aria-label={t("form.buildCommand")}
              id={buildId}
              value={form.build}
              onChange={(event) => onUpdate("build", event.target.value)}
              placeholder="cargo build --release"
              className="min-h-[72px] font-mono"
              spellCheck={false}
            />
          </Field>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <Field label={t("form.workdir")}>
              <Input
                aria-label={t("form.workdir")}
                id={workdirId}
                value={form.workdir}
                onChange={(event) => onUpdate("workdir", event.target.value)}
                placeholder={t("form.workdirPlaceholder")}
                className="font-mono"
              />
            </Field>

            <Field label={t("form.artifact")} hint={t("form.artifactHelp")}>
              <Input
                aria-label={t("form.artifact")}
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

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
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
      <Field label={t("roadmap.verify")}>
        <Textarea
          aria-label={t("roadmap.verify")}
          value={form.verifyCommand}
          onChange={(event) => onUpdate("verifyCommand", event.target.value)}
          className="font-mono"
        />
        <p className="text-caption text-fg-3">{t("roadmap.verifyHint")}</p>
      </Field>
    </>
  );
}
