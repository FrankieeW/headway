import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import { SegmentedControl } from "@/components/ui/segmented-control";
import type { AppFormFieldsProps } from "./AppFormFields";

export function AppFormSourceSection({
  form,
  onUpdate,
  onInspect,
  inspecting,
  lookup,
  error,
}: {
  readonly form: AppFormFieldsProps["form"];
  readonly onUpdate: AppFormFieldsProps["onUpdate"];
  readonly onInspect: () => void;
  readonly inspecting: boolean;
  readonly lookup: AppFormFieldsProps["lookup"];
  readonly error: string | null;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      <div className="space-y-1.5">
        <span className="block text-caption font-medium text-fg-2">
          {t("form.source")}
        </span>
        <SegmentedControl
          size="sm"
          aria-label={t("form.source")}
          value={form.source}
          onValueChange={(value) => {
            switch (value) {
              case "github":
              case "npm":
              case "brew":
              case "script":
                onUpdate("source", value);
                break;
            }
          }}
          items={(["github", "npm", "brew", "script"] as const).map(
            (value) => ({ value, label: t(`sources.${value}`) }),
          )}
        />
      </div>
      {form.source === "npm" || form.source === "brew" ? (
        <>
          <label className="block space-y-1.5">
            <span className="text-caption font-medium text-fg-2">
              {t("form.package")}
            </span>
            <Input
              value={form.package}
              onChange={(e) => onUpdate("package", e.target.value)}
              placeholder={form.source === "npm" ? "@scope/package" : "wget"}
            />
          </label>
          {form.source === "npm" ? (
            <label className="block space-y-1.5">
              <span className="text-caption font-medium text-fg-2">
                {t("form.distTag")}
              </span>
              <Input
                value={form.distTag}
                onChange={(e) => onUpdate("distTag", e.target.value)}
                placeholder="latest"
              />
            </label>
          ) : (
            <label className="flex items-center justify-between text-caption text-fg-2">
              {t("form.cask")}
              <Switch
                checked={form.cask}
                onCheckedChange={(v) => onUpdate("cask", v)}
                aria-label={t("form.cask")}
              />
            </label>
          )}
          <Button
            variant="neutral"
            onClick={onInspect}
            disabled={inspecting || !form.package.trim()}
          >
            {t(inspecting ? "form.inspecting" : "form.inspect")}
          </Button>
          {form.source === "brew" && form.installPath ? (
            <p className="break-all text-caption text-fg-3">
              {t("form.installPath")}: {form.installPath}
            </p>
          ) : null}
          {error ? (
            <p role="alert" className="text-caption text-danger-text">
              {error}
            </p>
          ) : null}
          {lookup ? (
            <div className="space-y-1 rounded-control bg-subtle p-3 text-caption text-fg-2">
              {lookup.description ? <p>{lookup.description}</p> : null}
              <p>
                {t("appPage.installed")}: {lookup.installed ?? "—"} ·{" "}
                {t("appPage.latest")}: {lookup.latest ?? "—"}
              </p>
              {lookup.notes.map((note) => (
                <p key={note}>{note}</p>
              ))}
            </div>
          ) : null}
        </>
      ) : null}
      {form.source === "script" ? (
        <>
          <p className="text-caption text-fg-3">{t("form.scriptHelp")}</p>
          {(["updateCommand", "versionCommand", "latestCommand"] as const).map(
            (key) => (
              <label key={key} className="block space-y-1.5">
                <span className="text-caption font-medium text-fg-2">
                  {t(`form.${key}`)}
                </span>
                <Textarea
                  value={form[key]}
                  required={key === "updateCommand"}
                  onChange={(e) => onUpdate(key, e.target.value)}
                  className="font-mono"
                  spellCheck={false}
                />
              </label>
            ),
          )}
        </>
      ) : null}
    </div>
  );
}
