import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  FolderOpen,
  Loader2,
  XCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  SettingsBlock,
  SettingsCard,
  SettingsRow,
} from "@/components/settings/SettingsLayout";
import { Button } from "@/components/ui/button";
import { useDoctor, usePaths } from "@/hooks/useSettings";
import { systemApi, type DoctorCheck } from "@/lib/api";

/** The icon of one check result. */
function CheckIcon({ status }: { status: DoctorCheck["status"] }) {
  if (status === "ok") {
    return (
      <CheckCircle2 className="h-4 w-4 text-success-text" strokeWidth={1.75} />
    );
  }
  if (status === "warn") {
    return (
      <AlertTriangle className="h-4 w-4 text-warning-text" strokeWidth={1.75} />
    );
  }
  return <XCircle className="h-4 w-4 text-danger-text" strokeWidth={1.75} />;
}

/**
 * The diagnostics section: environment checks, paths and the app version.
 */
export function DiagnosticsSection() {
  const { t } = useTranslation();
  const doctor = useDoctor();
  const { data: paths } = usePaths();

  const { data: version } = useQuery({
    queryKey: ["appVersion"],
    queryFn: () => systemApi.appVersion(),
    staleTime: Infinity,
  });

  return (
    <SettingsBlock
      title={t("settings.diagnostics")}
      actions={
        <Button
          variant="neutral"
          size="compact"
          onClick={() => void doctor.refetch()}
          disabled={doctor.isFetching}
        >
          {doctor.isFetching ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : null}
          {t("settings.runChecks")}
        </Button>
      }
    >
      <SettingsCard>
        {doctor.data ? (
          doctor.data.checks.map((check) => (
            <SettingsRow
              key={check.name}
              label={
                <span className="flex items-center gap-2">
                  <CheckIcon status={check.status} />
                  {check.name}
                </span>
              }
              description={check.hint ? `${check.message} — ${check.hint}` : check.message}
            />
          ))
        ) : (
          <SettingsRow
            label={t("settings.checks")}
            description={
              doctor.isFetching
                ? t("common.loading")
                : t("settings.checksDescription")
            }
          />
        )}

        <SettingsRow
          label={t("settings.paths")}
          description={t("settings.pathsDescription")}
        >
          <div className="space-y-1.5">
            {paths ? (
              (
                [
                  ["config_file", paths.config_file],
                  ["state_file", paths.state_file],
                  ["cache_dir", paths.cache_dir],
                  ["log_dir", paths.log_dir],
                ] as const
              ).map(([label, path]) => (
                <div
                  key={label}
                  className="flex items-center gap-2 rounded-control bg-subtle px-2 py-1.5"
                >
                  <span className="shrink-0 text-caption text-fg-3">{label}</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-caption text-fg-1">
                    {path}
                  </span>
                  <Button
                    variant="ghost"
                    size="icon-compact"
                    onClick={() => void systemApi.revealPath(path)}
                    aria-label={t("settings.reveal")}
                    title={t("settings.reveal")}
                  >
                    <FolderOpen className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </div>
              ))
            ) : (
              <p className="text-caption text-fg-3">{t("common.loading")}</p>
            )}
          </div>
        </SettingsRow>

        <SettingsRow
          label={t("settings.version")}
          description={version ?? t("common.loading")}
        />
      </SettingsCard>
    </SettingsBlock>
  );
}
