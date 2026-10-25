import { useMemo, useState } from "react";
import { PullRequests } from "./PullRequests";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { CommitBadges } from "./CommitBadges";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  appsApi,
  errorMessage,
  isCommandError,
  type AppView,
  type CiState,
  type CommitFilter,
} from "@/lib/api";
import { firstLine, shortSha, timeAgo } from "@/lib/format";

const ciStates = [
  "success",
  "failure",
  "pending",
  "not_run",
  "unknown",
] as const satisfies readonly CiState[];

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
  const [releasesOnly, setReleasesOnly] = useState(false);
  const [tagInput, setTagInput] = useState("");
  const [tag, setTag] = useState("");
  const [ci, setCi] = useState<CiState | "any">("any");
  const repo = view.app.source === "github" ? view.app.repo : "";
  const branch =
    view.app.source === "github" && view.app.track.mode === "commit"
      ? (view.app.track.branch ?? "")
      : "";
  const filter = useMemo<CommitFilter>(
    () => ({
      releases_only: releasesOnly,
      tag: tag || null,
      ci: ci === "any" ? null : ci,
    }),
    [releasesOnly, tag, ci],
  );
  const commits = useQuery({
    queryKey: [
      "commits",
      view.app.id,
      repo,
      branch,
      generation,
      cursor.anchor ?? "tip",
      cursor.page,
      filter.releases_only ? "releases" : "all",
      filter.tag ?? "",
      filter.ci ?? "any",
    ],
    queryFn: () =>
      appsApi.listCommits(view.app.id, cursor.page, 20, cursor.anchor, filter),
    enabled: view.app.source === "github" && !pullsOpen,
    staleTime: 5 * 60_000,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
  const resetToFirstPage = () =>
    setCursor((current) => ({
      page: 1,
      anchor: commits.data?.anchor ?? current.anchor,
    }));
  const invalidTag =
    Boolean(tag) &&
    isCommandError(commits.error) &&
    commits.error.code === "invalid_input";
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
      <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-end">
        <label className="flex items-center gap-2 text-caption text-fg-2">
          <Switch
            aria-label={t("versions.releasesOnly")}
            checked={releasesOnly}
            onCheckedChange={(value) => {
              setReleasesOnly(value);
              resetToFirstPage();
            }}
          />
          {t("versions.releasesOnly")}
        </label>
        <form
          className="flex min-w-0 flex-wrap items-end gap-2 sm:min-w-64 sm:flex-1"
          onSubmit={(event) => {
            event.preventDefault();
            setTag(tagInput.trim());
            resetToFirstPage();
          }}
        >
          <label className="min-w-0 flex-1 space-y-1 text-caption text-fg-2">
            <span>{t("versions.tagFilter")}</span>
            <Input
              className="font-mono"
              aria-label={t("versions.tagFilter")}
              placeholder={t("versions.tagFilterPlaceholder")}
              value={tagInput}
              onChange={(event) => setTagInput(event.target.value)}
            />
          </label>
          <Button type="submit" variant="neutral">
            {t("versions.applyFilter")}
          </Button>
        </form>
        <Select
          value={ci}
          onValueChange={(value) => {
            setCi(ciStates.find((state) => state === value) ?? "any");
            resetToFirstPage();
          }}
        >
          <SelectTrigger
            className="w-full sm:w-40"
            aria-label={t("versions.ciFilter")}
          >
            <SelectValue placeholder={t("versions.ciAny")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="any">{t("versions.ciAny")}</SelectItem>
            {ciStates.map((state) => (
              <SelectItem key={state} value={state}>
                {t(`commitMetadata.ci.${state}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
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
          {invalidTag ? <p>{t("versions.correctTag")}</p> : null}
          <p>{errorMessage(commits.error)}</p>
          {invalidTag ? (
            <Button
              variant="neutral"
              onClick={() => {
                setTagInput("");
                setTag("");
                resetToFirstPage();
              }}
            >
              {t("versions.clearTag")}
            </Button>
          ) : (
            <Button variant="neutral" onClick={() => void commits.refetch()}>
              {t("roadmap.retry")}
            </Button>
          )}
        </div>
      ) : null}
      {result?.commits.length === 0 ? (
        <p className="py-6 text-caption text-fg-3">
          {t(
            releasesOnly || tag || ci !== "any"
              ? "versions.noMatches"
              : "versions.noCommits",
          )}
        </p>
      ) : null}
      {result?.notes?.length ? (
        <div role="status" className="space-y-1 text-caption text-fg-2">
          {result.notes.map((note) => (
            <p key={note}>{note}</p>
          ))}
        </div>
      ) : null}
      {result?.filtered_out ? (
        <p className="text-caption text-fg-3">
          {t("versions.filteredOut", { count: result.filtered_out })}
        </p>
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
              {candidate.release_tag ? (
                <code
                  className="max-w-28 break-all rounded-control bg-action-soft px-1.5 text-badge text-action-text"
                  title={
                    candidate.can_download
                      ? t("versions.releaseAssetReady", {
                          tag: candidate.release_tag,
                        })
                      : t("versions.releaseTag", { tag: candidate.release_tag })
                  }
                >
                  {candidate.release_tag}
                  {candidate.can_download ? " ↓" : ""}
                </code>
              ) : null}
              <div className="order-last w-full min-w-0 sm:order-none sm:w-auto sm:flex-1">
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
                className="ml-auto"
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
