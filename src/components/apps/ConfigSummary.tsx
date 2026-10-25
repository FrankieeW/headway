import { Pencil } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { DEFAULT_UPDATE_POLICY, type TrackedApp } from "@/lib/api";
import { installLabel, trackLabel } from "@/lib/format";

export interface ConfigSummaryProps {
  app: TrackedApp;
  onEdit: () => void;
}

function Row({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-[140px_1fr] items-start gap-3 border-b border-border px-4 py-2.5 last:border-b-0">
      <span className="text-caption text-fg-3">{label}</span>
      <span
        className={
          mono
            ? "break-all font-mono text-caption text-fg-1"
            : "break-words text-body text-fg-1"
        }
      >
        {value}
      </span>
    </div>
  );
}

/**
 * The config tab: what Headway will do for this app, read only.
 */
export function ConfigSummary({ app, onEdit }: ConfigSummaryProps) {
  const { t } = useTranslation();

  const rows: { label: string; value: string; mono?: boolean }[] = [];

  rows.push({ label: t("form.source"), value: t(`sources.${app.source}`) });
  rows.push({ label: t("config.id"), value: app.id, mono: true });
  rows.push({
    label: t("config.kind"),
    value: app.kind === "cli" ? t("form.kindCli") : t("form.kindApp"),
  });
  const policy = app.update_policy ?? DEFAULT_UPDATE_POLICY;
  rows.push({ label: t("roadmap.policy"), value: t(`roadmap.${policy.mode}`) });
  rows.push({ label: t("roadmap.maxSteps"), value: String(policy.max_steps) });
  rows.push({
    label: t("roadmap.pauseOnFailure"),
    value: t(policy.pause_on_failure ? "common.yes" : "common.no"),
  });
  if (app.source === "github") {
    rows.push({
      label: t("roadmap.verify"),
      value: app.install.verify_command || "—",
      mono: true,
    });
    rows.push({ label: t("config.repo"), value: app.repo });
    rows.push({ label: t("config.follow"), value: trackLabel(app.track, t) });

    if (app.track.mode === "commit") {
      rows.push({
        label: t("config.branch"),
        value: app.track.branch || t("config.defaultBranch"),
        mono: true,
      });
    } else {
      rows.push({
        label: t("config.prerelease"),
        value: app.track.prerelease ? t("common.yes") : t("common.no"),
      });
      rows.push({
        label: t("config.tagFilter"),
        value: app.track.tag_filter || "—",
        mono: true,
      });
    }

    rows.push({
      label: t("config.installMethod"),
      value: installLabel(app.install, t),
    });

    if (app.install.method === "source") {
      rows.push({
        label: t("config.buildCommand"),
        value: app.install.build || "—",
        mono: true,
      });
      rows.push({
        label: t("config.workdir"),
        value: app.install.workdir || "—",
        mono: true,
      });
      rows.push({
        label: t("config.artifact"),
        value: app.install.artifact || t("config.artifactEmpty"),
        mono: true,
      });
    } else {
      rows.push({
        label: t("config.assetPattern"),
        value: app.install.pattern || t("config.assetAuto"),
        mono: true,
      });
      rows.push({
        label: t("config.binary"),
        value: app.install.binary || "—",
        mono: true,
      });
    }

    rows.push({
      label: t("config.dest"),
      value: app.install.dest || t("config.destDefault"),
      mono: true,
    });
    rows.push({
      label: t("config.gitUrl"),
      value: app.git_url || `https://github.com/${app.repo}.git`,
      mono: true,
    });
  } else if (app.source === "npm" || app.source === "brew") {
    if (app.source === "brew" && app.install_path) {
      rows.push({
        label: t("form.installPath"),
        value: app.install_path,
        mono: true,
      });
    }
    rows.push({ label: t("form.package"), value: app.package });
    rows.push(
      app.source === "npm"
        ? { label: t("form.distTag"), value: app.dist_tag }
        : {
            label: t("form.cask"),
            value: t(app.cask ? "common.yes" : "common.no"),
          },
    );
  } else {
    rows.push({
      label: t("form.updateCommand"),
      value: app.update_command,
      mono: true,
    });
    rows.push({
      label: t("form.versionCommand"),
      value: app.version_command || "—",
      mono: true,
    });
    rows.push({
      label: t("form.latestCommand"),
      value: app.latest_command || "—",
      mono: true,
    });
  }
  return (
    <div className="overflow-hidden rounded-panel border border-border bg-surface">
      {rows.map((row) => (
        <Row
          key={row.label}
          label={row.label}
          value={row.value}
          mono={row.mono}
        />
      ))}
      <div className="flex items-center justify-end border-t border-border px-4 py-2.5">
        <Button variant="neutral" size="compact" onClick={onEdit}>
          <Pencil className="h-3.5 w-3.5" strokeWidth={1.75} />
          {t("config.edit")}
        </Button>
      </div>
    </div>
  );
}
