import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { formatDuration } from "@/lib/format";
import type { UpdateRun } from "@/lib/updateRuns";
import { cn } from "@/lib/utils";

export interface RunProgressProps {
  run: UpdateRun;
  onCancel: () => void;
  cancelling?: boolean;
}

/** A ticking clock so the elapsed time grows while the run works. */
function useElapsed(startedAt: number): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  return Math.max(0, now - startedAt);
}

/**
 * The progress block that replaces the action buttons while an update runs.
 */
export function RunProgress({
  run,
  onCancel,
  cancelling = false,
}: RunProgressProps) {
  const { t } = useTranslation();
  const elapsed = useElapsed(run.startedAt);

  const total = run.progress?.total;
  const done = run.progress?.done ?? 0;
  const ratio =
    total !== undefined && total > 0
      ? Math.min(1, Math.max(0, done / total))
      : null;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <span className="text-body font-medium text-fg-1">
          {run.stage ? t(`stage.${run.stage}`) : t("appPage.working")}
        </span>
        <span className="text-caption text-fg-3">
          {formatDuration(elapsed)}
        </span>
      </div>

      {run.stageMessage ? (
        <p className="break-words text-caption text-fg-2">{run.stageMessage}</p>
      ) : null}

      {ratio !== null ? (
        <div className="space-y-1">
          <div
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(ratio * 100)}
            className="h-1.5 w-full overflow-hidden rounded-full bg-subtle"
          >
            <div
              className="h-full rounded-full bg-action transition-[width] duration-200"
              style={{ width: `${Math.round(ratio * 100)}%` }}
            />
          </div>
          <p className="text-caption text-fg-3">
            {t("appPage.progressBytes", {
              done: done.toLocaleString(),
              total: (total ?? 0).toLocaleString(),
            })}
          </p>
        </div>
      ) : null}

      <Button
        variant="neutral"
        size="compact"
        onClick={onCancel}
        disabled={cancelling}
      >
        <X className={cn("h-3.5 w-3.5", cancelling && "animate-pulse")} />
        {t("appPage.cancel")}
      </Button>
    </div>
  );
}
