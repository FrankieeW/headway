import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { SettingsBlock, SettingsCard, SettingsRow } from "./SettingsLayout";
import { Button } from "@/components/ui/button";
import { errorMessage, systemApi, type BuildInfo } from "@/lib/api";
import { useApps } from "@/hooks/useApps";

export function SelfUpdateStatus() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const { data: apps } = useApps();
  const status = useQuery({
    queryKey: ["self-update"],
    queryFn: systemApi.selfUpdateStatus,
    refetchInterval: 5000,
  });
  const restart = useMutation({
    mutationFn: systemApi.requestSelfRestart,
    onSuccess: (value) => client.setQueryData(["self-update"], value),
  });
  const value = status.data;
  const buildLabel = (build: BuildInfo) =>
    `${build.version} · ${build.commit ?? t("roadmap.unknownCommit")}${build.dirty ? ` · ${t("roadmap.dirtyBuild")}` : ""}`;
  const busy = apps?.some((app) => app.busy) ?? false;
  return (
    <SettingsBlock title={t("roadmap.selfUpdate")}>
      <SettingsCard>
        {status.isPending ? <SettingsRow label={t("common.loading")} /> : null}
        {status.error ? (
          <SettingsRow label={t("roadmap.statusFailed")}>
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(status.error)}
            </p>
            <Button variant="neutral" onClick={() => void status.refetch()}>
              {t("roadmap.retry")}
            </Button>
          </SettingsRow>
        ) : null}
        {value ? (
          <>
            <SettingsRow label={t("roadmap.running")}>
              <p className="break-all font-mono text-caption">
                {buildLabel(value.running)}
              </p>
            </SettingsRow>
            <SettingsRow label={t("roadmap.installed")}>
              <p className="break-all font-mono text-caption">
                {value.installed
                  ? buildLabel(value.installed)
                  : t("roadmap.notDetected")}
              </p>
              <p className="break-all text-caption text-fg-3">
                {value.installed_path || value.app_path}
              </p>
            </SettingsRow>
            <SettingsRow
              label={
                value.restart_error
                  ? t("roadmap.restartFailed")
                  : value.pending_restart
                    ? t("roadmap.pendingRestart")
                    : value.confirmed
                      ? t("roadmap.confirmed")
                      : t("roadmap.notConfirmed")
              }
              description={
                value.restart_requested && !value.restart_error
                  ? t("roadmap.restartQueued")
                  : busy && value.pending_restart
                    ? t("roadmap.restartBusy")
                    : undefined
              }
              control={
                value.pending_restart || value.restart_error ? (
                  <Button
                    variant="neutral"
                    disabled={restart.isPending || value.restart_requested}
                    onClick={() => restart.mutate()}
                  >
                    {t(
                      value.restart_error
                        ? "roadmap.retry"
                        : busy
                          ? "roadmap.queueRestart"
                          : "roadmap.restart",
                    )}
                  </Button>
                ) : undefined
              }
            >
              {value.restart_error ? (
                <p
                  role="alert"
                  className="break-words text-caption text-danger-text"
                >
                  {value.restart_error}
                </p>
              ) : null}
              {restart.error ? (
                <p role="alert" className="text-caption text-danger-text">
                  {errorMessage(restart.error)}
                </p>
              ) : null}
            </SettingsRow>
          </>
        ) : null}
      </SettingsCard>
    </SettingsBlock>
  );
}
