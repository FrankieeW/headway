import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { appsApi, errorMessage, type CiState } from "@/lib/api";
import { openUrl } from "@/lib/systemActions";
import { cn } from "@/lib/utils";
import { shortSha, timeAgo } from "@/lib/format";

const ciColors: Record<CiState, string> = {
  success: "bg-success-soft text-success-text",
  failure: "bg-danger-soft text-danger-text",
  pending: "bg-warning-soft text-warning-text",
  not_run: "bg-subtle text-fg-2",
  unknown: "bg-subtle text-fg-3",
};
const pill =
  "inline-flex max-w-full items-center rounded-full px-1.5 py-0.5 text-badge";

export function CommitBadges({
  appId,
  repo,
  sha,
  autoLoad,
}: {
  readonly appId: string;
  readonly repo: string;
  readonly sha: string;
  readonly autoLoad: boolean;
}) {
  const { t, i18n } = useTranslation();
  const container = useRef<HTMLDivElement>(null);
  const forceRefresh = useRef(false);
  const [visible, setVisible] = useState(false);
  const [requested, setRequested] = useState(false);
  const validSha = /^[0-9a-f]{7,40}$/i.test(sha);
  const key = ["commit-metadata", appId, repo, sha] as const;

  useEffect(() => {
    const element = container.current;
    if (!element) return;
    if (typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      setVisible(entries.some((entry) => entry.isIntersecting));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const metadata = useQuery({
    queryKey: key,
    queryFn: () => {
      const refresh = forceRefresh.current;
      forceRefresh.current = false;
      return appsApi.commitMetadata(appId, sha, refresh);
    },
    enabled: validSha && visible && (autoLoad || requested),
    staleTime: 5 * 60_000,
    gcTime: 30 * 60_000,
    retry: false,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    refetchInterval: (query) =>
      visible && query.state.data?.ci_state === "pending" ? 30_000 : false,
    refetchIntervalInBackground: false,
  });
  const data = metadata.isError ? undefined : metadata.data;
  const ci = data?.ci_state ?? "unknown";
  const release =
    data?.is_release === true
      ? "release"
      : data?.is_release === false
        ? "notRelease"
        : "releaseUnknown";
  const version = t("commitMetadata.version", {
    version: data?.version || "—",
  });
  const versionDetail =
    [
      data?.version_source
        ? t(`commitMetadata.versionSources.${data.version_source}`)
        : null,
      ...(data?.tags ?? []),
    ]
      .filter(Boolean)
      .join(" · ") || version;
  const ciLabel = t(`commitMetadata.ci.${ci}`);
  const releaseLabel = t(`commitMetadata.${release}`);
  const open = (url: string) => openUrl(url);

  return (
    <div
      ref={container}
      className="mt-1.5 space-y-1"
      aria-label={t("commitMetadata.label")}
      title={
        data
          ? t("commitMetadata.checkedAt", {
              time: timeAgo(data.checked_at, i18n.language),
            })
          : undefined
      }
    >
      <div className="flex flex-wrap items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <span
              tabIndex={0}
              className={cn(
                pill,
                "break-all bg-subtle text-fg-2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
              )}
              aria-label={`${version} · ${versionDetail}`}
            >
              {version}
            </span>
          </TooltipTrigger>
          <TooltipContent>{versionDetail}</TooltipContent>
        </Tooltip>
        {data?.ci_url ? (
          <button
            type="button"
            className={cn(
              pill,
              ciColors[ci],
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            )}
            onClick={() => {
              if (data.ci_url) open(data.ci_url);
            }}
            title={t("commitMetadata.checks", { count: data.checks_count })}
          >
            {ciLabel}
          </button>
        ) : (
          <span
            className={cn(pill, ciColors[ci])}
            title={
              data
                ? t("commitMetadata.checks", { count: data.checks_count })
                : undefined
            }
          >
            {ciLabel}
          </span>
        )}
        {data?.is_release === true && data.release_url ? (
          <button
            type="button"
            className={cn(
              pill,
              "bg-action-soft text-action-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            )}
            onClick={() => {
              if (data.release_url) open(data.release_url);
            }}
          >
            {releaseLabel}
          </button>
        ) : (
          <span className={cn(pill, "bg-subtle text-fg-3")}>
            {releaseLabel}
          </span>
        )}
        <button
          type="button"
          className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-fg-3 hover:bg-subtle hover:text-fg-1 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50"
          aria-label={t("commitMetadata.refresh", { sha: shortSha(sha) })}
          title={t("commitMetadata.refresh", { sha: shortSha(sha) })}
          disabled={!validSha || metadata.isFetching}
          onClick={() => {
            forceRefresh.current = true;
            setRequested(true);
            void metadata.refetch();
          }}
        >
          <RefreshCw
            className={cn("h-3 w-3", metadata.isFetching && "animate-spin")}
          />
        </button>
      </div>
      {metadata.error ? (
        <p role="alert" className="break-words text-caption text-danger-text">
          {errorMessage(metadata.error)}
        </p>
      ) : null}
      {data?.notes.length ? (
        <p className="break-words text-caption text-fg-3">
          {data.notes.join(" · ")}
        </p>
      ) : null}
    </div>
  );
}
