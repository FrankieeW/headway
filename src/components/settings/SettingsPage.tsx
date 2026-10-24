import { SelfUpdateStatus } from "./SelfUpdateStatus";
import { useTranslation } from "react-i18next";

import { PageHeader } from "@/components/shell/PageHeader";
import { CliSection } from "@/components/settings/CliSection";
import { DiagnosticsSection } from "@/components/settings/DiagnosticsSection";
import { GeneralSection } from "@/components/settings/GeneralSection";
import { GitHubSection } from "@/components/settings/GitHubSection";
import { LocationsSection } from "@/components/settings/LocationsSection";
import { SettingsBody } from "@/components/settings/SettingsLayout";
import { UpdatesSection } from "@/components/settings/UpdatesSection";

/**
 * The settings page: one card per topic.
 */
export function SettingsPage() {
  const { t } = useTranslation();

  return (
    <>
      <PageHeader title={t("settings.title")} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <SettingsBody>
          <GitHubSection />
          <UpdatesSection />
          <LocationsSection />
          <CliSection />
          <SelfUpdateStatus />
          <GeneralSection />
          <DiagnosticsSection />
        </SettingsBody>
      </div>
    </>
  );
}
