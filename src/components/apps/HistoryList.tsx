import { CheckCircle2, RotateCcw, XCircle } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import type { HistoryEntry } from "@/lib/api";
import { formatDuration, shortSha, timeAgo } from "@/lib/format";

export interface HistoryListProps {
  entries: HistoryEntry[] | undefined;
  loading: boolean;
  onReinstall: (ref: string) => void;
}

/** What one entry did, in one sentence. */
function describe(
  entry: HistoryEntry,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  if (entry.action === "mark") {
    return t("history.marked", { ref: shortSha(entry.to) });
  }
  if (entry.from) {
    return t("history.updated", {
      from: shortSha(entry.from),
      to: shortSha(entry.to),
    });
  }
  return t("history.installed", { ref: shortSha(entry.to) });
}

/**
 * The history tab: what Headway installed and when.
 */
export function HistoryList({
  entries,
  loading,
  onReinstall,
}: HistoryListProps) {
  const { t, i18n } = useTranslation();

  if (loading && !entries) {
    return (
      <p className="px-2 py-6 text-center text-caption text-fg-3">
        {t("history.loading")}
      </p>
    );
  }

  if (!entries || entries.length === 0) {
    return (
      <div className="px-6 py-10 text-center">
        <p className="text-body font-medium text-fg-1">
          {t("history.emptyTitle")}
        </p>
        <p className="mt-1 text-caption text-fg-2">
          {t("history.emptyDescription")}
        </p>
      </div>
    );
  }

  return (
    <ul className="space-y-0.5">
      {entries.map((entry, index) => (
        <li
          key={`${entry.at}-${index}`}
          className="group flex items-start gap-3 rounded-control px-2 py-2 transition-colors hover:bg-subtle"
        >
          <span className="mt-0.5 shrink-0">
            {entry.ok ? (
              <CheckCircle2
                className="h-4 w-4 text-success-text"
                strokeWidth={1.75}
              />
            ) : (
              <XCircle
                className="h-4 w-4 text-danger-text"
                strokeWidth={1.75}
              />
            )}
          </span>

          <div className="min-w-0 flex-1">
            <p className="truncate text-body text-fg-1">
              {describe(entry, t)}
            </p>
            <p className="truncate text-caption text-fg-3">
              {[
                entry.version,
                timeAgo(entry.at, i18n.language),
                entry.duration_ms != null
                  ? formatDuration(entry.duration_ms)
                  : null,
              ]
                .filter(Boolean)
                .join(" · ")}
            </p>
            {entry.ok ? null : entry.message ? (
              <p className="mt-0.5 break-words text-caption text-danger-text">
                {entry.message}
              </p>
            ) : null}
          </div>

          {entry.ok ? (
            <div className="shrink-0 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
              <Button
                variant="neutral"
                size="compact"
                onClick={() => onReinstall(entry.to)}
              >
                <RotateCcw className="h-3.5 w-3.5" strokeWidth={1.75} />
                {t("history.reinstall")}
              </Button>
            </div>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
