import { useTranslation } from "react-i18next";
import { Field } from "./FormField";
import { SourceStorageFields } from "./SourceStorageFields";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { SourceInstall, AssetInstall } from "@/lib/api";

export function RevisionSourceFields({
  value,
  onChange,
}: {
  readonly value: SourceInstall;
  readonly onChange: (value: SourceInstall) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      <Field label={t("form.buildCommand")}>
        <Textarea
          aria-label={t("form.buildCommand")}
          value={value.build}
          onChange={(event) =>
            onChange({ ...value, build: event.target.value })
          }
          className="font-mono"
        />
      </Field>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label={t("form.workdir")}>
          <Input
            aria-label={t("form.workdir")}
            value={value.workdir ?? ""}
            onChange={(event) =>
              onChange({ ...value, workdir: event.target.value || null })
            }
            className="font-mono"
          />
        </Field>
        <Field label={t("form.artifact")}>
          <Input
            aria-label={t("form.artifact")}
            value={value.artifact ?? ""}
            onChange={(event) =>
              onChange({ ...value, artifact: event.target.value || null })
            }
            className="font-mono"
          />
        </Field>
      </div>
      <Field label={t("roadmap.verify")}>
        <Textarea
          aria-label={t("roadmap.verify")}
          value={value.verify_command ?? ""}
          onChange={(event) =>
            onChange({ ...value, verify_command: event.target.value || null })
          }
          className="font-mono"
        />
      </Field>
      <SourceStorageFields
        value={value.storage ?? { keep: true, root: null }}
        onChange={(storage) => onChange({ ...value, storage })}
      />
    </div>
  );
}
export function RevisionAssetFields({
  value,
  onChange,
}: {
  readonly value: AssetInstall;
  readonly onChange: (value: AssetInstall) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      <Field label={t("form.assetPattern")}>
        <Input
          aria-label={t("form.assetPattern")}
          value={value.pattern ?? ""}
          onChange={(event) =>
            onChange({ ...value, pattern: event.target.value || null })
          }
          placeholder={t("form.assetPatternPlaceholder")}
          className="font-mono"
        />
      </Field>
      <Field label={t("form.binary")}>
        <Input
          aria-label={t("form.binary")}
          value={value.binary ?? ""}
          onChange={(event) =>
            onChange({ ...value, binary: event.target.value || null })
          }
          className="font-mono"
        />
      </Field>
      <Field label={t("roadmap.verify")}>
        <Textarea
          aria-label={t("roadmap.verify")}
          value={value.verify_command ?? ""}
          onChange={(event) =>
            onChange({ ...value, verify_command: event.target.value || null })
          }
          className="font-mono"
        />
      </Field>
    </div>
  );
}
