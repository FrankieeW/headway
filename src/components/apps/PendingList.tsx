import { useState } from "react";
import { Check, ChevronDown, ExternalLink, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { systemApi, type AppView, type Candidate } from "@/lib/api";
import { firstLine, shortSha, timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

export interface PendingListProps {
  app: AppView;
  installing: boolean;
  onInstall: (ref: string) => void;
  onMarkInstalled: (ref: string) => void;
  onCheck: () => void;
}

/** The pill on the left of a row: the short SHA or the tag. */
function RefPill({ candidate }: { candidate: Candidate }) {
  const { t } = useTranslation();
  const isSha = /^[0-9a-f]{7,40}$/i.test(candidate.id);

  return (
    <span className="flex shrink-0 items-center gap-1">
      <span className="rounded-full bg-subtle px-1.5 font-mono text-badge text-fg-2">
        {isSha ? shortSha(candidate.id) : candidate.id}
      </span>
      {candidate.prerelease ? (
        <span className="rounded-full bg-warning-soft px-1.5 text-badge font-medium text-warning-text">
          {t("pending.preRelease")}
        </span>
      ) : null}
    </span>
  );
}

/** Release notes — untrusted text, rendered as plain text. */
function ReleaseNotes({ body }: { body: string }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <div className="mt-1.5">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="inline-flex items-center gap-1 text-caption text-action-text transition-colors hover:underline"
      >
        {t("pending.notes")}
        <ChevronDown
          className={cn(
            "h-3 w-3 transition-transform",
            open && "rotate-180",
          )}
          strokeWidth={2}
        />
      </button>
      {open ? (
        <div className="mt-1.5 max-h-48 overflow-y-auto whitespace-pre-wrap rounded-control bg-subtle p-2 text-caption text-fg-2">
          {body}
        </div>
      ) : null}
    </div>
  );
}

function CandidateRow({
  candidate,
  installing,
  showNotes,
  onInstall,
  onMarkInstalled,
}: {
  candidate: Candidate;
  installing: boolean;
  /** release notes only make sense for releases and tags */
  showNotes: boolean;
  onInstall: (ref: string) => void;
  onMarkInstalled: (ref: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const title = firstLine(candidate.title);
  const meta = [
    candidate.author,
    candidate.date ? timeAgo(candidate.date, i18n.language) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <li className="group flex items-start gap-3 rounded-control px-2 py-2 transition-colors hover:bg-subtle">
      <RefPill candidate={candidate} />

      <div className="min-w-0 flex-1">
        {title && title !== candidate.id ? (
          <p className="truncate text-body text-fg-1" title={title}>
            {title}
          </p>
        ) : null}
        {meta ? (
          <p className="truncate text-caption text-fg-3">{meta}</p>
        ) : null}
        {showNotes && candidate.body && candidate.body.trim() ? (
          <ReleaseNotes body={candidate.body} />
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
        <Button
          variant="neutral"
          size="compact"
          onClick={() => onInstall(candidate.id)}
          disabled={installing}
        >
          {installing ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Check className="h-3.5 w-3.5" strokeWidth={2} />
          )}
          {t("pending.install")}
        </Button>
        <Button
          variant="ghost"
          size="compact"
          onClick={() => onMarkInstalled(candidate.id)}
          disabled={installing}
        >
          {t("pending.markInstalled")}
        </Button>
        {candidate.url ? (
          <button
            type="button"
            onClick={() => void systemApi.openUrl(candidate.url as string)}
            title={t("pending.openOnGitHub")}
            aria-label={t("pending.openOnGitHub")}
            className="inline-flex h-7 w-7 items-center justify-center rounded-control text-fg-2 transition-colors hover:bg-surface hover:text-fg-1"
          >
            <ExternalLink className="h-3.5 w-3.5" strokeWidth={1.75} />
          </button>
        ) : null}
      </div>
    </li>
  );
}

/**
 * The changes tab: what upstream found and what can be installed.
 */
export function PendingList({
  app,
  installing,
  onInstall,
  onMarkInstalled,
  onCheck,
}: PendingListProps) {
  const { t } = useTranslation();
  const check = app.last_check;
  const pending = check?.pending ?? [];
  const showNotes = app.app.track.mode !== "commit";

  if (pending.length === 0) {
    if (app.status === "up_to_date") {
      return (
        <EmptyState
          title={t("pending.emptyUpToDate")}
          description={t("pending.emptyUpToDateDescription")}
        />
      );
    }
    if (app.status === "unchecked" || !check) {
      return (
        <EmptyState
          title={t("pending.notChecked")}
          description={t("pending.notCheckedDescription")}
          action={
            <Button variant="neutral" size="compact" onClick={onCheck}>
              <Check className="h-3.5 w-3.5" strokeWidth={2} />
              {t("pending.checkNow")}
            </Button>
          }
        />
      );
    }
    if (app.status === "not_installed") {
      return (
        <EmptyState
          title={t("pending.emptyNotInstalled")}
          description={t("pending.emptyNotInstalledDescription")}
          action={
            <Button variant="neutral" size="compact" onClick={onCheck}>
              <Check className="h-3.5 w-3.5" strokeWidth={2} />
              {t("pending.checkNow")}
            </Button>
          }
        />
      );
    }
    return (
      <EmptyState
        title={t("pending.emptyNothing")}
        description={t("pending.emptyNothingDescription")}
      />
    );
  }

  const remaining = (check?.pending_total ?? pending.length) - pending.length;

  return (
    <div>
      <ul className="space-y-0.5">
        {pending.map((candidate) => (
          <CandidateRow
            key={candidate.id}
            candidate={candidate}
            installing={installing}
            showNotes={showNotes}
            onInstall={onInstall}
            onMarkInstalled={onMarkInstalled}
          />
        ))}
      </ul>

      {remaining > 0 ? (
        <p className="mt-2 px-2 text-caption text-fg-3">
          {t("pending.moreOnGitHub", { count: remaining })}
        </p>
      ) : null}
    </div>
  );
}

function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-2 px-6 py-10 text-center">
      <p className="text-body font-medium text-fg-1">{title}</p>
      <p className="max-w-sm text-caption text-fg-2">{description}</p>
      {action ? <div className="mt-1">{action}</div> : null}
    </div>
  );
}
