import type { UpdateFinished } from "@/lib/api";
import type { TranslateFn } from "@/lib/format";

/** How a completed update is announced. Warnings never look like plain success. */
export type UpdateNotificationTone = "success" | "warning";

export interface UpdateNotification {
  tone: UpdateNotificationTone;
  title: string;
  /** Warning evidence plus a hint to the full log; rendered as plain text. */
  description?: string;
  /** Raw backend warning strings, preserved verbatim (never HTML). */
  warnings: string[];
}

/** How many warning lines the toast previews before summarising the rest. */
export const WARNING_PREVIEW_LIMIT = 3;

/** Warnings carried by a completed run (integrity evidence, not authenticity). */
export function outcomeWarnings(payload: UpdateFinished): string[] {
  return payload.outcome?.warnings ?? [];
}

/**
 * Description for a warning toast: the first few backend warnings verbatim,
 * a count for the rest, and where to read everything. The backend text is
 * data, so it is joined into one plain-text string (sonner renders it as text).
 */
export function warningDescription(
  warnings: string[],
  t: TranslateFn,
): string | undefined {
  if (warnings.length === 0) return undefined;
  const shown = warnings.slice(0, WARNING_PREVIEW_LIMIT);
  const hidden = warnings.length - shown.length;
  const lines = [...shown];
  if (hidden > 0) {
    lines.push(t("toast.warningsMore", { count: hidden }));
  }
  lines.push(t("toast.warningsLogHint"));
  return lines.join("\n");
}

/**
 * Decide the toast for a finished update. A run that succeeded with warnings
 * is announced as a warning, never as a verified success; the warning text
 * itself is shown unchanged.
 */
export function updateNotification(
  payload: UpdateFinished,
  name: string,
  t: TranslateFn,
): UpdateNotification {
  const version = payload.outcome?.to.version || payload.outcome?.to.id;
  const warnings = outcomeWarnings(payload);
  if (warnings.length > 0) {
    return {
      tone: "warning",
      title: version
        ? t("toast.updatedToWithWarnings", { name, version })
        : t("toast.updatedWithWarnings", { name }),
      description: warningDescription(warnings, t),
      warnings,
    };
  }
  return {
    tone: "success",
    title: version
      ? t("toast.updatedTo", { name, version })
      : t("toast.updated", { name }),
    warnings: [],
  };
}

/**
 * A failed cycle may still have completed steps before failing. Name the last
 * successful outcome so the error toast does not hide installed progress.
 */
export function completedOutcomeNote(
  payload: UpdateFinished,
  t: TranslateFn,
): string | undefined {
  if (payload.ok || !payload.outcome) return undefined;
  const version = payload.outcome.to.version || payload.outcome.to.id;
  return version
    ? t("toast.completedBeforeFailure", { version })
    : undefined;
}
