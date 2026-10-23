import { Check, CircleDashed, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { AppStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

export interface StatusBadgeProps {
  status: AppStatus;
  /** how many updates are waiting — shown as a number */
  pendingCount?: number;
  /** `compact` fits the sidebar, `full` the detail header */
  variant?: "compact" | "full";
}

/**
 * The status pill of one app.
 *
 * `update_available` carries the number of pending updates; the other
 * states are a word or an icon.
 */
export function StatusBadge({
  status,
  pendingCount = 0,
  variant = "compact",
}: StatusBadgeProps) {
  const { t } = useTranslation();
  const compact = variant === "compact";

  if (status === "busy") {
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center text-fg-2",
          compact ? "h-5 w-5" : "h-5 gap-1.5 px-1.5",
        )}
        title={t("status.busy")}
        aria-label={t("status.busy")}
      >
        <Loader2 className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
        {!compact ? (
          <span className="text-badge text-fg-2">{t("status.busy")}</span>
        ) : null}
      </span>
    );
  }

  if (status === "up_to_date") {
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center text-success-text",
          compact ? "h-5 w-5" : "h-5 gap-1.5 px-1.5",
        )}
        title={t("status.upToDate")}
        aria-label={t("status.upToDate")}
      >
        <Check className="h-3.5 w-3.5" strokeWidth={2.5} />
        {!compact ? (
          <span className="text-badge text-success-text">
            {t("status.upToDate")}
          </span>
        ) : null}
      </span>
    );
  }

  if (status === "unchecked") {
    return (
      <span
        className={cn(
          "inline-flex h-5 items-center justify-center rounded-full px-1.5 text-badge text-fg-3",
          compact ? "w-5 px-0" : "",
        )}
        title={t("status.unchecked")}
      >
        {t("status.unchecked")}
      </span>
    );
  }

  if (status === "update_available") {
    const count = Math.max(0, pendingCount);
    return (
      <span
        className={cn(
          "inline-flex h-5 items-center justify-center rounded-full bg-action-soft text-badge font-semibold text-action-text",
          compact ? "min-w-5 px-1.5" : "gap-1 px-2",
        )}
        title={t("status.newCount", { count })}
      >
        {compact ? count : t("status.newCount", { count })}
      </span>
    );
  }

  if (status === "not_installed") {
    return (
      <span
        className="inline-flex h-5 items-center justify-center rounded-full px-1.5 text-badge text-fg-3"
        title={t("status.notInstalled")}
        aria-label={compact ? t("status.notInstalled") : undefined}
      >
        {compact ? (
          <CircleDashed className="h-3.5 w-3.5" strokeWidth={2} />
        ) : (
          t("status.notInstalled")
        )}
      </span>
    );
  }

  if (status === "diverged") {
    return (
      <span
        className={cn(
          "inline-flex h-5 items-center justify-center rounded-full bg-warning-soft text-badge font-medium text-warning-text",
          compact ? "max-w-[72px] truncate px-1.5" : "px-2",
        )}
        title={t("status.diverged")}
      >
        {t("status.diverged")}
      </span>
    );
  }

  return (
    <span
      className={cn(
        "inline-flex h-5 items-center justify-center rounded-full bg-danger-soft text-badge font-medium text-danger-text",
        compact ? "max-w-[72px] truncate px-1.5" : "px-2",
      )}
      title={t("status.error")}
    >
      {t("status.error")}
    </span>
  );
}
