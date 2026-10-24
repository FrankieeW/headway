import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { appsApi, errorMessage, type AppView } from "@/lib/api";

export function VariantSection({ view }: { readonly view: AppView }) {
  const { t } = useTranslation();
  const launch = useMutation({
    mutationFn: () => appsApi.launchVariant(view.app.id),
  });
  if (view.app.source !== "github" || !view.app.variant) return null;
  return (
    <section className="space-y-2 rounded-panel border border-border bg-surface p-4">
      <h2 className="text-section">{t("versions.variant")}</h2>
      <p className="break-all text-caption text-fg-3">
        {view.app.variant.parent_id} · {view.app.variant.reference}
      </p>
      <p className="text-caption text-fg-2">{t("versions.sharedData")}</p>
      {view.app.kind === "cli" ? (
        <p className="text-caption text-fg-2">
          {t("versions.cliCommand")}:{" "}
          <code className="break-all font-mono">{view.app.id}</code>
        </p>
      ) : null}
      {view.app.kind === "app" ? (
        <Button
          variant="neutral"
          disabled={!view.installed || view.busy || launch.isPending}
          onClick={() => launch.mutate()}
        >
          {t("versions.launch")}
        </Button>
      ) : null}
      {launch.error ? (
        <p role="alert" className="text-caption text-danger-text">
          {errorMessage(launch.error)}
        </p>
      ) : null}
      {launch.data ? (
        <p role="status" className="break-all text-caption">
          {t("versions.launched", {
            pid: launch.data.pid,
            path: launch.data.path,
          })}
        </p>
      ) : null}
    </section>
  );
}
