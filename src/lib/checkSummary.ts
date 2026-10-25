import type {
  CheckAllSummary,
  CheckSummaryFailure,
  CheckSummaryRecord,
  CommandError,
} from "@/lib/api";

/** One failed app, independent of which backend shape produced it. */
export interface NormalizedCheckFailure extends CheckSummaryFailure {
  error?: CommandError | null;
}

export interface NormalizedCheckSummary {
  checked: number;
  with_updates: number;
  results: CheckSummaryRecord[];
  failures: NormalizedCheckFailure[];
}

/**
 * Normalises the shared `check_all` summary.
 *
 * Newer backends return the CLI's shared `results` document plus the
 * desktop-compatible `failed` aggregate; older ones only returned counts and
 * `failed`. When both exist, `failed` is the compatibility view and `results`
 * carries the per-app status/details.
 */
export function normalizeCheckSummary(
  summary: CheckAllSummary,
): NormalizedCheckSummary {
  const results = summary.results ?? [];
  const failures: NormalizedCheckFailure[] =
    summary.failed && summary.failed.length > 0
      ? summary.failed.map((failure) => ({
          id: failure.id,
          message: failure.message,
          error: failure.error ?? null,
        }))
      : results
          .filter((record) => !record.ok)
          .map((record) => ({
            id: record.id,
            message: record.error?.message ?? "",
            error: record.error ?? null,
          }));

  return {
    checked: summary.checked,
    with_updates: summary.with_updates,
    results,
    failures,
  };
}
