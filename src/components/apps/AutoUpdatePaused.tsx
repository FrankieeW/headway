import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { errorMessage, opsApi } from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";

export function AutoUpdatePaused({
  id,
  busy,
}: {
  readonly id: string;
  readonly busy: boolean;
}) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const resume = useMutation({
    mutationFn: () => opsApi.resumeAutoUpdate(id),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: queryKeys.apps });
      await client.invalidateQueries({ queryKey: queryKeys.app(id) });
    },
  });
  return (
    <div className="space-y-2 rounded-control bg-warning-soft p-3 text-caption">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p>{t("roadmap.paused")}</p>
        <Button
          variant="neutral"
          disabled={busy || resume.isPending}
          onClick={() => resume.mutate()}
        >
          {t("roadmap.resume")}
        </Button>
      </div>
      {resume.error ? (
        <p role="alert" className="text-danger-text">
          {errorMessage(resume.error)}
        </p>
      ) : null}
    </div>
  );
}
