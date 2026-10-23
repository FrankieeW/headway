import { useEffect, useRef, useState } from "react";
import { FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  SettingsBlock,
  SettingsCard,
  SettingsRow,
} from "@/components/settings/SettingsLayout";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useSaveSettings, useSettings } from "@/hooks/useSettings";
import { systemApi } from "@/lib/api";

/**
 * The locations section: where binaries and `.app` bundles are installed.
 * Paths are saved when the field loses the focus.
 */
export function LocationsSection() {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const saveSettings = useSaveSettings();

  const [binDir, setBinDir] = useState("");
  const [appsDir, setAppsDir] = useState("");

  // Fill the fields once the settings arrive — but only when the value on
  // the server really changed. A background refetch must not overwrite
  // what the user is typing right now.
  const synced = useRef<{ bin?: string; apps?: string }>({});

  useEffect(() => {
    if (!settings) return;
    if (synced.current.bin !== settings.bin_dir) {
      synced.current.bin = settings.bin_dir;
      setBinDir(settings.bin_dir);
    }
    if (synced.current.apps !== settings.apps_dir) {
      synced.current.apps = settings.apps_dir;
      setAppsDir(settings.apps_dir);
    }
  }, [settings]);

  const save = (patch: { bin_dir?: string; apps_dir?: string }) => {
    if (!settings) return;
    saveSettings.mutate({ ...settings, ...patch });
  };

  return (
    <SettingsBlock title={t("settings.locations")}>
      <SettingsCard>
        <SettingsRow
          label={t("settings.binDir")}
          description={t("settings.binDirDescription")}
        >
          <div className="flex items-center gap-2">
            <Input
              value={binDir}
              onChange={(event) => setBinDir(event.target.value)}
              onBlur={() => {
                const value = binDir.trim();
                if (!value || value === settings?.bin_dir) return;
                save({ bin_dir: value });
              }}
              className="font-mono"
              spellCheck={false}
            />
            <Button
              variant="neutral"
              size="regular"
              onClick={() => void systemApi.revealPath(binDir)}
              disabled={!binDir}
              className="shrink-0"
            >
              <FolderOpen className="h-4 w-4" strokeWidth={1.75} />
              {t("settings.reveal")}
            </Button>
          </div>
        </SettingsRow>

        <SettingsRow
          label={t("settings.appsDir")}
          description={t("settings.appsDirDescription")}
        >
          <div className="flex items-center gap-2">
            <Input
              value={appsDir}
              onChange={(event) => setAppsDir(event.target.value)}
              onBlur={() => {
                const value = appsDir.trim();
                if (!value || value === settings?.apps_dir) return;
                save({ apps_dir: value });
              }}
              className="font-mono"
              spellCheck={false}
            />
            <Button
              variant="neutral"
              size="regular"
              onClick={() => void systemApi.revealPath(appsDir)}
              disabled={!appsDir}
              className="shrink-0"
            >
              <FolderOpen className="h-4 w-4" strokeWidth={1.75} />
              {t("settings.reveal")}
            </Button>
          </div>
        </SettingsRow>
      </SettingsCard>
    </SettingsBlock>
  );
}
