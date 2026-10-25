import { useTranslation } from "react-i18next";

import { useTheme } from "@/components/theme-provider";
import {
  SettingsBlock,
  SettingsCard,
  SettingsSwitchRow,
} from "@/components/settings/SettingsLayout";
import { SegmentedControl } from "@/components/ui/segmented-control";
import {
  useAutostart,
  useSaveSettings,
  useSetAutostart,
  useSettings,
} from "@/hooks/useSettings";
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
  const { data: settings } = useSettings();
  const saveSettings = useSaveSettings({ silent: true });

  const changeLanguage = (value: Language): void => {
    setLanguage(value);
    // The tray is rendered by the backend, so persist the choice instead of
    // keeping it in localStorage only.
    if (settings && settings.language !== value) {
      saveSettings.mutate({ ...settings, language: value });
    }
  };

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
          <span className="min-w-0">
            <span className="block text-body font-medium text-fg-1">
              {t("settings.language")}
            </span>
            <span className="block text-caption text-fg-3">
              {t("settings.languageDescription")}
            </span>
          </span>
          <SegmentedControl
            size="sm"
            aria-label={t("settings.language")}
            value={i18n.language.startsWith("zh") ? "zh" : "en"}
            onValueChange={(value) => changeLanguage(value as Language)}
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
