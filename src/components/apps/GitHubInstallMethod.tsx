import { useTranslation } from "react-i18next";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { UpdateMethod } from "@/lib/api";

/** Build and download are the two ways to install one GitHub app. The saved method is the default. */
export function GitHubInstallMethod({
  saved,
  selected,
  disabled,
  onChange,
}: {
  saved: "source" | "asset";
  selected: UpdateMethod;
  disabled: boolean;
  onChange: (method: UpdateMethod) => void;
}) {
  const { t } = useTranslation();
  const shown = selected === "configured" ? saved : selected;
  return (
    <div className="space-y-1 border-t border-border pt-3">
      <label className="block text-caption text-fg-2" htmlFor="update-method">
        {t("appPage.installMethod")}
      </label>
      <Select
        value={shown}
        disabled={disabled}
        onValueChange={(value) => {
          if (value === "source" || value === "asset") {
            onChange(value === saved ? "configured" : value);
          }
        }}
      >
        <SelectTrigger
          id="update-method"
          className="w-full"
          aria-label={t("appPage.installMethod")}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="source">{t("appPage.methodSource")}</SelectItem>
          <SelectItem value="asset">{t("appPage.methodAsset")}</SelectItem>
        </SelectContent>
      </Select>
      <p className="text-caption text-fg-3">
        {t("appPage.installMethodsHint")}
      </p>
    </div>
  );
}
