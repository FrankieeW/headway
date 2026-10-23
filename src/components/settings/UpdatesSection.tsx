import { useTranslation } from "react-i18next";

import {
  SettingsBlock,
  SettingsCard,
  SettingsRow,
  SettingsSwitchRow,
} from "@/components/settings/SettingsLayout";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useSaveSettings, useSettings } from "@/hooks/useSettings";

/** The offered check intervals, in minutes. `0` turns the checks off. */
const INTERVALS = [0, 15, 30, 60, 180, 360, 720, 1440];

/**
 * The updates section: how often Headway checks and whether it notifies.
 */
export function UpdatesSection() {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const saveSettings = useSaveSettings();

  const save = (patch: Partial<NonNullable<typeof settings>>) => {
    if (!settings) return;
    saveSettings.mutate({ ...settings, ...patch });
  };

  const intervalLabel = (minutes: number): string => {
    if (minutes === 0) return t("settings.intervalOff");
    if (minutes < 60) return t("settings.intervalMinutes", { count: minutes });
    const hours = minutes / 60;
    return t("settings.intervalHours", { count: hours });
  };

  return (
    <SettingsBlock title={t("settings.updates")}>
      <SettingsCard>
        <SettingsRow
          label={t("settings.checkInterval")}
          description={t("settings.checkIntervalDescription")}
          control={
            <Select
              value={String(settings?.check_interval_minutes ?? 60)}
              onValueChange={(value) =>
                save({ check_interval_minutes: Number(value) })
              }
              disabled={!settings}
            >
              <SelectTrigger className="w-[150px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {INTERVALS.map((minutes) => (
                  <SelectItem key={minutes} value={String(minutes)}>
                    {intervalLabel(minutes)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />

        <SettingsSwitchRow
          label={t("settings.notifications")}
          description={t("settings.notificationsDescription")}
          checked={settings?.notify ?? false}
          onCheckedChange={(checked) => save({ notify: checked })}
          disabled={!settings}
        />
      </SettingsCard>
    </SettingsBlock>
  );
}
