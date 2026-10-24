import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { AppFormState } from "./appFormModel";

export function UpdatePolicyFields({
  form,
  onUpdate,
}: {
  readonly form: AppFormState;
  readonly onUpdate: <K extends keyof AppFormState>(
    key: K,
    value: AppFormState[K],
  ) => void;
}) {
  const { t } = useTranslation();
  const policy = form.updatePolicy;
  const supportsNext = form.source === "github" || form.source === "npm";
  return (
    <section className="space-y-3 border-t border-border pt-4">
      <label className="block space-y-1.5">
        <span className="text-caption font-medium">{t("roadmap.policy")}</span>
        <Select
          value={policy.mode}
          onValueChange={(mode) => {
            if (mode !== "notify" && mode !== "latest" && mode !== "next")
              return;
            onUpdate("updatePolicy", { ...policy, mode });
            if (mode === "notify") onUpdate("autoUpdate", false);
          }}
        >
          <SelectTrigger aria-label={t("roadmap.policy")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="notify">{t("roadmap.notify")}</SelectItem>
            <SelectItem value="latest">{t("roadmap.latest")}</SelectItem>
            <SelectItem value="next" disabled={!supportsNext}>
              {t("roadmap.next")}
            </SelectItem>
          </SelectContent>
        </Select>
      </label>
      <p className="text-caption text-fg-3">{t("roadmap.optIn")}</p>
      {policy.mode === "next" ? (
        <label className="block space-y-1.5">
          <span className="text-caption">{t("roadmap.maxSteps")}</span>
          <Input
            aria-label={t("roadmap.maxSteps")}
            type="number"
            min={1}
            step={1}
            value={policy.max_steps}
            onChange={(event) => {
              const max_steps = Number(event.target.value);
              if (Number.isSafeInteger(max_steps) && max_steps > 0)
                onUpdate("updatePolicy", { ...policy, max_steps });
            }}
          />
        </label>
      ) : null}
      <label className="flex items-center justify-between gap-3">
        <span className="text-body">{t("roadmap.pauseOnFailure")}</span>
        <Switch
          aria-label={t("roadmap.pauseOnFailure")}
          checked={policy.pause_on_failure}
          onCheckedChange={(pause_on_failure) =>
            onUpdate("updatePolicy", { ...policy, pause_on_failure })
          }
        />
      </label>
      <label className="flex items-center justify-between gap-3">
        <span className="text-body">{t("form.autoUpdate")}</span>
        <Switch
          aria-label={t("form.autoUpdate")}
          checked={form.autoUpdate}
          disabled={policy.mode === "notify"}
          onCheckedChange={(value) => onUpdate("autoUpdate", value)}
        />
      </label>
    </section>
  );
}
