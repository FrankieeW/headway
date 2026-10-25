import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { appsApi, errorMessage, type AppView } from "@/lib/api";
import { shortSha } from "@/lib/format";

export function PullRequests({
  view,
  onChoose,
  onBack,
}: {
  readonly view: AppView;
  readonly onChoose: (reference: string) => void;
  readonly onBack: () => void;
}) {
  const { t } = useTranslation();
  const [page, setPage] = useState(1);
  const [number, setNumber] = useState("");
  const result = useQuery({
    queryKey: [
      "pull-requests",
      view.app.id,
      view.app.source === "github" ? view.app.repo : "",
      page,
    ],
    queryFn: () => appsApi.listPullRequests(view.app.id, page),
    staleTime: 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-section">{t("pulls.title")}</h2>
        <Button variant="neutral" onClick={onBack}>
          {t("pulls.back")}
        </Button>
      </div>
      <div className="flex flex-wrap gap-2">
        <Input
          className="min-w-0 flex-1"
          inputMode="numeric"
          value={number}
          onChange={(event) => setNumber(event.target.value)}
          aria-label={t("pulls.number")}
          placeholder={t("pulls.number")}
        />
        <Button
          variant="neutral"
          disabled={view.busy || !/^[1-9]\d*$/.test(number)}
          onClick={() => onChoose(`pr:${number}`)}
        >
          {t("versions.resolveReference")}
        </Button>
      </div>
      <p className="text-caption text-fg-3">{t("pulls.hint")}</p>
      {result.isFetching ? (
        <p role="status" className="text-caption text-fg-3">
          {t("common.loading")}
        </p>
      ) : null}
      {result.error ? (
        <div role="alert" className="space-y-2 text-caption text-danger-text">
          <p>{errorMessage(result.error)}</p>
          <Button variant="neutral" onClick={() => void result.refetch()}>
            {t("roadmap.retry")}
          </Button>
        </div>
      ) : null}
      {result.data?.pull_requests.length === 0 ? (
        <p className="py-6 text-caption text-fg-3">{t("pulls.empty")}</p>
      ) : null}
      <ul className="space-y-1">
        {result.data?.pull_requests.map((pull) => (
          <li
            key={pull.number}
            className="space-y-2 rounded-control px-2 py-3 hover:bg-subtle"
          >
            <p className="break-words text-body text-fg-1">
              #{pull.number} · {pull.title}
            </p>
            <p className="break-all text-caption text-fg-3">
              {pull.head.repo?.full_name ?? t("pulls.deletedFork")} ·{" "}
              {pull.head.ref} · {shortSha(pull.head.sha)}
              {pull.draft ? ` · ${t("pulls.draft")}` : ""}
            </p>
            <Button
              variant="neutral"
              disabled={view.busy}
              onClick={() => onChoose(`pr:${pull.number}@${pull.head.sha}`)}
            >
              {t("versions.installOptions")}
            </Button>
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-3">
        <Button
          variant="neutral"
          disabled={page <= 1 || result.isFetching}
          onClick={() => setPage(page - 1)}
        >
          {t("versions.previous")}
        </Button>
        <span className="text-caption text-fg-3">
          {t("versions.page", { page })}
        </span>
        <Button
          variant="neutral"
          disabled={!result.data?.has_more || result.isFetching}
          onClick={() => setPage(page + 1)}
        >
          {t("versions.next")}
        </Button>
      </div>
    </section>
  );
}
