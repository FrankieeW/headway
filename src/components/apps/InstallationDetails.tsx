import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { appsApi, errorMessage, type AppView } from "@/lib/api";
import { shortSha } from "@/lib/format";

export function InstallationDetails({ view }: { readonly view: AppView }) {
  const { t } = useTranslation();
  const result = useQuery({
    queryKey: [
      "installation",
      view.app.id,
      view.app.source === "github" ? view.app.repo : "",
      view.installed?.installed_at,
      view.status,
      view.installation,
    ],
    queryFn: () => appsApi.detectInstallation(view.app.id),
    enabled: view.app.source === "github",
    initialData: view.installation ?? undefined,
    retry: false,
  });
  if (view.app.source !== "github") return null;
  const detection = result.data;
  return (
    <div className="space-y-1 border-t border-border pt-3 text-caption text-fg-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span>{t(`installation.${detection?.presence ?? "unknown"}`)}</span>
        <Button
          variant="ghost"
          disabled={result.isFetching || view.busy}
          onClick={() => void result.refetch()}
        >
          {t("installation.refresh")}
        </Button>
      </div>
      {detection?.path ? (
        <p className="break-all font-mono">{detection.path}</p>
      ) : null}
      {detection?.recorded && !detection.baseline_matches ? (
        <p className="break-words">
          {t("installation.recorded", {
            repo: detection.recorded.repo,
            version:
              detection.recorded.installed.version ??
              shortSha(detection.recorded.installed.id),
          })}
        </p>
      ) : null}
      {view.pull_request ? (
        <p>
          {t("pulls.installed", {
            number: view.pull_request.number,
            sha: shortSha(view.pull_request.head.sha),
          })}
        </p>
      ) : null}
      {result.error ? (
        <p role="alert" className="break-words text-danger-text">
          {errorMessage(result.error)}
        </p>
      ) : null}
    </div>
  );
}
