import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { errorMessage, type SourceMatches } from "@/lib/api";
import type { MatchSelection } from "./matchSelection";

export function SourceMatchList({
  matches,
  onSelect,
}: {
  readonly matches: SourceMatches;
  readonly onSelect: (source: MatchSelection) => void;
}) {
  const { t } = useTranslation();
  const rows =
    matches.from === "brew"
      ? matches.repositories.map((match) => ({
          ...match,
          key: match.repo,
          label: match.repo,
          installed: null,
          selection: {
            source: "github",
            repo: match.repo,
          } satisfies MatchSelection,
        }))
      : matches.packages.map((match) => ({
          ...match,
          key: `${match.cask}:${match.package}`,
          label: `${t(match.cask ? "matches.cask" : "matches.formula")} · ${match.package}`,
          selection: {
            source: "brew",
            package: match.package,
            cask: match.cask,
          } satisfies MatchSelection,
        }));
  return (
    <div className="space-y-2 text-caption">
      {rows.length === 0 ? (
        <p className="text-fg-3">{t("matches.none")}</p>
      ) : (
        <ul className="space-y-2">
          {rows.map((row) => (
            <li
              key={row.key}
              aria-label={row.label}
              className="min-w-0 space-y-2 rounded-control bg-subtle p-3"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="min-w-0 break-all font-medium">
                  {row.label}
                </span>
                <Badge variant="secondary">
                  {t(`matches.confidence.${row.confidence}`)}
                </Badge>
              </div>
              {row.description ? (
                <p className="break-words text-fg-2">{row.description}</p>
              ) : null}
              <ul className="space-y-1 text-fg-3">
                {row.evidence.map((evidence) => (
                  <li
                    className="break-all"
                    key={`${evidence.kind}:${evidence.value}`}
                  >
                    {t(`matches.evidence.${evidence.kind}`)}: {evidence.value}
                  </li>
                ))}
              </ul>
              {row.installed ? (
                <p className="text-fg-2">
                  {t("matches.installed", { version: row.installed })}
                </p>
              ) : null}
              {row.tracked_by.length > 0 ? (
                <p className="break-words text-fg-3">
                  {t("matches.trackedBy", { ids: row.tracked_by.join(", ") })}
                </p>
              ) : null}
              <Button
                type="button"
                variant="neutral"
                className="min-h-11"
                disabled={row.tracked_by.length > 0}
                onClick={() => onSelect(row.selection)}
              >
                {t(
                  matches.from === "brew"
                    ? "matches.trackGithub"
                    : "matches.useHomebrew",
                )}
              </Button>
            </li>
          ))}
        </ul>
      )}
      {matches.notes.map((note) => (
        <p key={note} className="break-words text-fg-3">
          {note}
        </p>
      ))}
      {matches.errors.map(({ source, error }, index) => (
        <p
          key={`${source}:${index}`}
          role="alert"
          className="break-words text-danger-text"
        >
          {source}: {errorMessage(error)}
        </p>
      ))}
    </div>
  );
}
