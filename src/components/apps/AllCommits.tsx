import { useState } from "react";
import { PullRequests } from "./PullRequests";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { CommitBadges } from "./CommitBadges";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { appsApi, errorMessage, type AppView } from "@/lib/api";
import { firstLine, shortSha, timeAgo } from "@/lib/format";

export function AllCommits({
  view,
  onChoose,
}: {
  readonly view: AppView;
  readonly onChoose: (reference: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [cursor, setCursor] = useState<{ page: number; anchor?: string }>({
    page: 1,
  });
  const [generation, setGeneration] = useState(0);
  const [reference, setReference] = useState("");
  const [pullsOpen, setPullsOpen] = useState(false);
  const repo = view.app.source === "github" ? view.app.repo : "";
  const branch =
    view.app.source === "github" && view.app.track.mode === "commit"
      ? (view.app.track.branch ?? "")
      : "";
  const commits = useQuery({
    queryKey: [
      "commits",
      view.app.id,
      repo,
      branch,
      generation,
      cursor.anchor ?? "tip",
      cursor.page,
    ],
    queryFn: () =>
      appsApi.listCommits(view.app.id, cursor.page, 20, cursor.anchor),
    enabled: view.app.source === "github" && !pullsOpen,
    staleTime: 5 * 60_000,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
  if (view.app.source !== "github") return null;
  if (pullsOpen)
    return (
      <PullRequests
        view={view}
        onChoose={onChoose}
        onBack={() => setPullsOpen(false)}
      />
    );
  const result = commits.data;
  const budget = view.installed && view.app.track.mode === "commit" ? 4 : 5;
  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-section">{t("versions.allCommits")}</h2>
        <Button variant="neutral" onClick={() => setPullsOpen(true)}>
          {t("pulls.title")}
        </Button>
        <Button
          variant="neutral"
          disabled={commits.isFetching}
          onClick={() => {
            setCursor({ page: 1 });
            setGeneration((value) => value + 1);
          }}
        >
          {t("versions.refreshHistory")}
        </Button>
      </div>
      <p className="break-all text-caption text-fg-3">
        {repo} · {result?.branch || branch || t("config.defaultBranch")} ·{" "}
        {t("versions.anchor")}: {result?.anchor ?? cursor.anchor ?? "—"}
      </p>
      <div className="flex flex-wrap gap-2">
        <Input
          className="min-w-0 flex-1 font-mono"
          aria-label={t("versions.reference")}
          value={reference}
          onChange={(event) => setReference(event.target.value)}
          placeholder={t("versions.referencePlaceholder")}
        />
        <Button
          variant="neutral"
          disabled={!reference.trim() || view.busy}
          onClick={() => onChoose(reference.trim())}
        >
          {t("versions.resolveReference")}
        </Button>
      </div>
      {commits.isFetching ? (
        <p role="status" className="text-caption text-fg-3">
          {t("common.loading")}
        </p>
      ) : null}
      {commits.error ? (
        <div role="alert" className="space-y-2 text-caption text-danger-text">
          <p>{errorMessage(commits.error)}</p>
          <Button variant="neutral" onClick={() => void commits.refetch()}>
            {t("roadmap.retry")}
          </Button>
        </div>
      ) : null}
      {result?.commits.length === 0 ? (
        <p className="py-6 text-caption text-fg-3">{t("versions.noCommits")}</p>
      ) : null}
      <ul className="space-y-1">
        {result?.commits.map((candidate, index) => (
          <li
            key={candidate.sha ?? candidate.id}
            className="space-y-2 rounded-control px-2 py-3 hover:bg-subtle"
          >
            <div className="flex flex-wrap items-start gap-2">
              <code className="rounded-full bg-subtle px-1.5 text-badge text-fg-2">
                {shortSha(candidate.sha ?? candidate.id)}
              </code>
              <div className="min-w-0 flex-1">
                <p className="break-words text-body text-fg-1">
                  {firstLine(candidate.title)}
                </p>
                <p className="text-caption text-fg-3">
                  {[
                    candidate.author,
                    candidate.date
                      ? timeAgo(candidate.date, i18n.language)
                      : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
                <CommitBadges
                  appId={view.app.id}
                  repo={repo}
                  sha={candidate.sha ?? candidate.id}
                  autoLoad={index < budget}
                />
              </div>
              <Button
                variant="neutral"
                disabled={view.busy}
                onClick={() => onChoose(candidate.sha ?? candidate.id)}
              >
                {t("versions.installOptions")}
              </Button>
            </div>
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-3">
        <Button
          variant="neutral"
          disabled={cursor.page <= 1 || commits.isFetching}
          onClick={() => {
            const anchor = result?.anchor ?? cursor.anchor;
            if (anchor) setCursor({ page: cursor.page - 1, anchor });
          }}
        >
          {t("versions.previous")}
        </Button>
        <span className="text-caption text-fg-3">
          {t("versions.page", { page: cursor.page })}
        </span>
        <Button
          variant="neutral"
          disabled={
            !result?.has_more || result.next_page === null || commits.isFetching
          }
          onClick={() => {
            if (result?.next_page)
              setCursor({ page: result.next_page, anchor: result.anchor });
          }}
        >
          {t("versions.next")}
        </Button>
      </div>
    </section>
  );
}
