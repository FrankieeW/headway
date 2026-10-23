import { Pencil } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import type { TrackedApp } from "@/lib/api";
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
    <div className="grid grid-cols-[140px_1fr] items-start gap-3 border-b border-border px-4 py-2.5 last:border-b-0">
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

  rows.push({ label: t("config.repo"), value: app.repo });
  rows.push({ label: t("config.id"), value: app.id, mono: true });
  rows.push({
    label: t("config.kind"),
    value:
      app.kind === "cli" ? t("form.kindCli") : t("form.kindApp"),
  });
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

  return (
    <div className="overflow-hidden rounded-panel border border-border bg-surface">
      {rows.map((row) => (
        <Row key={row.label} label={row.label} value={row.value} mono={row.mono} />
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
