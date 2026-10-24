import { useTranslation } from "react-i18next";
import { Field } from "./FormField";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { SourceStorage } from "@/lib/api";

export function SourceStorageFields({
  value,
  onChange,
}: {
  readonly value: SourceStorage;
  readonly onChange: (value: SourceStorage) => void;
}) {
  const { t } = useTranslation();
  return (
    <section className="space-y-3 rounded-control bg-subtle p-3">
      <label className="flex items-center justify-between gap-3">
        <span className="text-caption font-medium">
          {t("versions.keepSource")}
        </span>
        <Switch
          aria-label={t("versions.keepSource")}
          checked={value.keep}
          onCheckedChange={(keep) => onChange({ ...value, keep })}
        />
      </label>
      <p className="text-caption text-fg-3">{t("versions.keepSourceHint")}</p>
      <Field label={t("versions.sourceRoot")}>
        <Input
          aria-label={t("versions.sourceRoot")}
          value={value.root ?? ""}
          onChange={(event) =>
            onChange({ ...value, root: event.target.value || null })
          }
          placeholder={t("versions.defaultRoot")}
          className="font-mono"
        />
      </Field>
    </section>
  );
}
