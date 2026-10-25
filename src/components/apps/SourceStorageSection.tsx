import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { appsApi, errorMessage, type TrackedApp } from "@/lib/api";
import { revealPath } from "@/lib/systemActions";
import { Button } from "@/components/ui/button";

export function SourceStorageSection({ app }: { readonly app: TrackedApp }) {
  const { t } = useTranslation();
  const enabled = app.source === "github" && app.install.method === "source";
  const status = useQuery({
    queryKey: ["source-storage", app.id],
    queryFn: () => appsApi.sourceStorageStatus(app.id),
    enabled,
  });
  if (!enabled) return null;
  return (
    <section className="mt-4 space-y-2 rounded-panel border border-border bg-surface p-4">
      <h2 className="text-section">{t("versions.sourceStorage")}</h2>
      {status.isPending ? (
        <p role="status" className="text-caption">
          {t("common.loading")}
        </p>
      ) : null}
      {status.error ? (
        <p role="alert" className="text-caption text-danger-text">
          {errorMessage(status.error)}{" "}
          <Button variant="neutral" onClick={() => void status.refetch()}>
            {t("roadmap.retry")}
          </Button>
        </p>
      ) : null}
      {status.data ? (
        <>
          <p className="break-all font-mono text-caption">{status.data.path}</p>
          <p className="text-caption text-fg-3">
            {t(
              status.data.exists
                ? "versions.storageExists"
                : "versions.storageMissing",
            )}{" "}
            ·{" "}
            {t(
              status.data.keep
                ? "versions.sourceKept"
                : "versions.sourceTemporary",
            )}{" "}
            ·{" "}
            {t(
              status.data.custom_root
                ? "versions.customRoot"
                : "versions.defaultRoot",
            )}
          </p>
          <Button
            variant="neutral"
            disabled={!status.data.exists}
            onClick={() => {
              if (status.data) revealPath(status.data.path);
            }}
          >
            {t("versions.revealSource")}
          </Button>
        </>
      ) : null}
    </section>
  );
}
