import { useTranslation } from "react-i18next";

import { useTheme } from "@/components/theme-provider";
import {
  SettingsBlock,
  SettingsCard,
  SettingsSwitchRow,
} from "@/components/settings/SettingsLayout";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { useAutostart, useSetAutostart } from "@/hooks/useSettings";
import { setLanguage, type Language } from "@/i18n";
import type { Theme } from "@/lib/themeBootstrap";

const THEMES: Theme[] = ["light", "dark", "system"];
const LANGUAGES: Language[] = ["zh", "en"];

/**
 * The general section: launch at login, theme and language.
 */
export function GeneralSection() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { data: autostart } = useAutostart();
  const setAutostart = useSetAutostart();

  return (
    <SettingsBlock title={t("settings.general")}>
      <SettingsCard>
        <SettingsSwitchRow
          label={t("settings.launchAtLogin")}
          description={t("settings.launchAtLoginDescription")}
          checked={autostart ?? false}
          onCheckedChange={(checked) => setAutostart.mutate(checked)}
          disabled={setAutostart.isPending}
        />

        <div className="flex min-h-[28px] items-center justify-between gap-4 px-4 py-3">
          <span className="text-body font-medium text-fg-1">
            {t("settings.theme.label")}
          </span>
          <SegmentedControl
            size="sm"
            aria-label={t("settings.theme.label")}
            value={theme}
            onValueChange={(value) => setTheme(value as Theme)}
            items={THEMES.map((value) => ({
              value,
              label: t(`settings.theme.${value}`),
            }))}
          />
        </div>

        <div className="flex min-h-[28px] items-center justify-between gap-4 px-4 py-3">
          <span className="text-body font-medium text-fg-1">
            {t("settings.language")}
          </span>
          <SegmentedControl
            size="sm"
            aria-label={t("settings.language")}
            value={i18n.language.startsWith("zh") ? "zh" : "en"}
            onValueChange={(value) => setLanguage(value as Language)}
            items={LANGUAGES.map((value) => ({
              value,
              label: value === "zh" ? "中文" : "English",
            }))}
          />
        </div>
      </SettingsCard>
    </SettingsBlock>
  );
}
