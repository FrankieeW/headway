import { CommitBadges } from "./CommitBadges";
import { GitHubInstallMethod } from "./GitHubInstallMethod";
import { InstallationDetails } from "./InstallationDetails";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ArrowRight, Download } from "lucide-react";
import { useTranslation } from "react-i18next";

import { RunProgress } from "@/components/apps/RunProgress";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { AppView, UpdateMethod } from "@/lib/api";
import { firstLine, shortSha, timeAgo } from "@/lib/format";
import type { UpdateRun } from "@/lib/updateRuns";
import { supportsRefs } from "@/lib/sources";
import { cn } from "@/lib/utils";

export interface AppSummaryProps {
  app: AppView;
  /** the live run of this app, if one is going on */
  run: UpdateRun | undefined;
  starting: boolean;
  cancelling: boolean;
  onUpdateLatest: () => void;
  onUpdateNext: () => void;
  onCancel: () => void;
  /** How the next update installs; only that run uses it. */
  updateMethod: UpdateMethod;
  onUpdateMethodChange: (method: UpdateMethod) => void;
  /**
   * Persist the auto-update switch. Must return a promise that rejects when
   * the backend rejected the change, so the switch can roll back.
   */
  onToggleAutoUpdate: (value: boolean) => Promise<unknown>;
}

type Tone = "normal" | "success" | "warning" | "danger";

function SummaryColumn({
  label,
  value,
  title,
  time,
  children,
}: {
  label: string;
  value: string;
  title?: string;
  time?: string;
  children?: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <p className="text-caption text-fg-3">{label}</p>
      <p className="mt-0.5 truncate font-mono text-strong text-fg-1">{value}</p>
      {title ? (
        <p className="mt-0.5 truncate text-caption text-fg-2" title={title}>
          {title}
        </p>
      ) : null}
      {time ? <p className="text-caption text-fg-3">{time}</p> : null}
      {children}
    </div>
  );
}

/**
 * The card at the top of the app page: what is installed, what is
 * available and the buttons that start an update.
 */
export function AppSummary({
  app,
  run,
  starting,
  cancelling,
  onUpdateLatest,
  onUpdateNext,
  onCancel,
  updateMethod,
  onUpdateMethodChange,
  onToggleAutoUpdate,
}: AppSummaryProps) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;

  const installed = app.installed ?? null;
  const latest = app.last_check?.latest ?? null;
  const installedUnconfirmed =
    app.app.source === "github" && app.status === "unknown" && !installed;

  // a run that is not finished yet; a finished one is only history
  const liveRun = run && !run.finished ? run : undefined;
  const busy = app.busy || Boolean(liveRun);

  // the backend may report an update that the CLI started — then there is
  // no run of ours, but the progress block still has to show up
  const fallbackRun = useMemo<UpdateRun>(
    () => ({
      lines: [],
      startedAt: run?.startedAt ?? Date.now(),
      stage: run?.stage,
      stageMessage: run?.stageMessage,
    }),
    [busy, run],
  );
  const activeRun = liveRun ?? (busy ? fallbackRun : undefined);

  // The switch reacts immediately, the server confirms a moment later. A
  // failed save must not leave the switch showing the opposite of the
  // persisted value, so a rejection restores the previous state.
  const [autoUpdate, setAutoUpdate] = useState(app.app.auto_update);
  const [autoUpdatePending, setAutoUpdatePending] = useState(false);
  useEffect(() => {
    setAutoUpdate(app.app.auto_update);
  }, [app.app.auto_update]);

  const toggleAutoUpdate = (value: boolean): void => {
    const previous = autoUpdate;
    setAutoUpdate(value);
    setAutoUpdatePending(true);
    void Promise.resolve(onToggleAutoUpdate(value))
      .catch(() => {
        setAutoUpdate(previous);
      })
      .finally(() => {
        setAutoUpdatePending(false);
      });
  };

  const { text, tone } = describeStatus(app, t);

  const installedValue = installed
    ? installed.version || shortSha(installed.id)
    : "—";
  const latestValue = latest ? shortSha(latest.id) : "—";

  const nextLabel =
    app.app.source === "github" && app.app.track.mode === "commit"
      ? t("appPage.nextCommit")
      : app.app.source === "github" && app.app.track.mode === "release"
        ? t("appPage.nextRelease")
        : t(
            app.app.source === "npm"
              ? "appPage.nextVersion"
              : "appPage.nextTag",
          );

  return (
    <div className="flex flex-col gap-4 rounded-panel border border-border bg-surface p-4 md:flex-row">
      <div className="min-w-0 flex-1 space-y-3">
        <div className="grid grid-cols-2 gap-6">
          <SummaryColumn
            label={t("appPage.installed")}
            value={installedValue}
            time={
              installed
                ? t(
                    app.app.source === "github"
                      ? "appPage.installedAt"
                      : "appPage.detectedAt",
                    {
                      when: timeAgo(installed.installed_at, lang),
                    },
                  )
                : t(
                    installedUnconfirmed
                      ? "appPage.installationFound"
                      : "appPage.notInstalledYet",
                  )
            }
          >
            {installed &&
            app.app.source === "github" &&
            app.app.track.mode === "commit" ? (
              <CommitBadges
                appId={app.app.id}
                repo={app.app.repo}
                sha={installed.sha ?? installed.id}
                autoLoad
              />
            ) : null}
          </SummaryColumn>
          <SummaryColumn
            label={t("appPage.latest")}
            value={latestValue}
            title={
              latest && firstLine(latest.title) !== latest.id
                ? firstLine(latest.title)
                : undefined
            }
            time={
              latest
                ? latest.date
                  ? t("appPage.publishedAt", {
                      when: timeAgo(latest.date, lang),
                    })
                  : undefined
                : t(
                    app.status === "unknown"
                      ? "status.unknown"
                      : "appPage.notCheckedYet",
                  )
            }
          />
        </div>

        <p
          className={cn(
            "text-body",
            tone === "success" && "text-success-text",
            tone === "warning" && "text-warning-text",
            tone === "danger" && "text-danger-text",
            tone === "normal" && "text-fg-2",
          )}
        >
          {text}
        </p>

        <InstallationDetails view={app} />
        {app.last_error ? (
          <Alert className="border-danger/40 bg-danger-soft text-danger-text">
            <AlertDescription className="text-caption text-danger-text">
              {app.last_error.message}
            </AlertDescription>
          </Alert>
        ) : null}
      </div>

      <div className="w-full shrink-0 space-y-3 md:w-[236px]">
        {busy && activeRun ? (
          <RunProgress
            run={activeRun}
            onCancel={onCancel}
            cancelling={cancelling}
          />
        ) : (
          <div className="space-y-2">
            <Button
              variant="solid"
              size="regular"
              className="w-full justify-center"
              onClick={onUpdateLatest}
              disabled={starting || app.status === "up_to_date"}
            >
              <Download className="h-4 w-4" strokeWidth={1.75} />
              {t("appPage.updateToLatest")}
            </Button>

            {supportsRefs(app.app) && app.pending_count > 1 ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="neutral"
                    size="regular"
                    className="w-full justify-center"
                    onClick={onUpdateNext}
                    disabled={starting}
                  >
                    <ArrowRight className="h-4 w-4" strokeWidth={1.75} />
                    {nextLabel}
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("appPage.nextTooltip")}</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
        )}

        {app.app.source === "github" ? (
          <GitHubInstallMethod
            saved={app.app.install.method}
            selected={updateMethod}
            disabled={starting || busy}
            onChange={onUpdateMethodChange}
          />
        ) : null}

        <label className="flex items-center justify-between gap-3 border-t border-border pt-3">
          <span className="text-caption text-fg-2">
            {t("appPage.autoUpdate")}
          </span>
          <Switch
            size="sm"
            checked={autoUpdate}
            disabled={autoUpdatePending}
            onCheckedChange={toggleAutoUpdate}
            aria-label={t("appPage.autoUpdate")}
          />
        </label>
      </div>
    </div>
  );
}

/** The one line that says what Headway found. */
function describeStatus(
  app: AppView,
  t: (key: string, options?: Record<string, unknown>) => string,
): { text: string; tone: Tone } {
  const branch =
    app.last_check?.branch ??
    (app.app.source === "github" && app.app.track.mode === "commit"
      ? app.app.track.branch
      : null);

  switch (app.status) {
    case "update_available": {
      if (app.app.source === "github" && app.app.track.mode === "commit") {
        return {
          text: branch
            ? t("appPage.status.newCommits", {
                count: app.pending_count,
                branch,
              })
            : t("appPage.status.newCommitsNoBranch", {
                count: app.pending_count,
              }),
          tone: "normal",
        };
      }
      const version = app.last_check?.latest
        ? shortSha(app.last_check.latest.id)
        : "";
      return {
        text: t("appPage.status.newRelease", { version }),
        tone: "normal",
      };
    }
    case "unknown":
      return {
        text: t(
          app.app.source === "github" && !app.installed
            ? "appPage.status.installedUnconfirmed"
            : "appPage.status.unknown",
        ),
        tone: "normal",
      };
    case "up_to_date":
      return { text: t("appPage.status.upToDate"), tone: "success" };
    case "not_installed":
      return { text: t("appPage.status.notInstalled"), tone: "normal" };
    case "diverged":
      return {
        text: branch
          ? t("appPage.status.diverged", { branch })
          : t("appPage.status.divergedDefault"),
        tone: "warning",
      };
    case "error":
      return {
        text:
          app.last_error?.during === "update"
            ? t("appPage.status.updateFailed")
            : t("appPage.status.failed"),
        tone: "danger",
      };
    case "busy":
      return { text: t("appPage.status.busy"), tone: "normal" };
    default:
      return { text: t("appPage.status.unchecked"), tone: "normal" };
  }
}
