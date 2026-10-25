import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { appsApi, errorMessage, type MatchSide } from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";
import { SourceMatchList } from "./SourceMatchList";
import type { MatchSelection } from "./matchSelection";

export function SourceMatchesSection({
  input,
  from,
  onSelect,
}: {
  readonly input: string;
  readonly from: MatchSide;
  readonly onSelect: (source: MatchSelection) => void;
}) {
  const { t } = useTranslation();
  const matches = useQuery({
    queryKey: queryKeys.sourceMatches(input),
    queryFn: () => appsApi.matchSource(input),
    retry: false,
    staleTime: 5 * 60 * 1000,
    refetchOnWindowFocus: false,
  });
  return (
    <section
      className="space-y-2 pt-2"
      aria-label={t(
        from === "github" ? "matches.alsoHomebrew" : "matches.githubRepository",
      )}
    >
      <h3 className="text-caption font-medium text-fg-2">
        {t(
          from === "github"
            ? "matches.alsoHomebrew"
            : "matches.githubRepository",
        )}
      </h3>
      {matches.isPending ? (
        <p role="status" className="text-caption text-fg-3">
          {t("matches.searching")}
        </p>
      ) : null}
      {matches.error ? (
        <p role="alert" className="break-words text-caption text-danger-text">
          {errorMessage(matches.error)}
        </p>
      ) : null}
      {matches.data ? (
        <SourceMatchList matches={matches.data} onSelect={onSelect} />
      ) : null}
    </section>
  );
}
