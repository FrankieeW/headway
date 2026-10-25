import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { opsApi } from "@/lib/api";
import { revealPath } from "@/lib/systemActions";
import { queryKeys } from "@/lib/query/keys";
import { useUpdateRun } from "@/lib/updateRuns";
import { cn } from "@/lib/utils";

export interface LogPanelProps {
  appId: string;
}

/** How far from the bottom the view has to be to count as "scrolled up". */
const STICK_THRESHOLD = 24;

/**
 * The log tab: the live output of a running update, otherwise the log
 * file of the last one.
 */
export function LogPanel({ appId }: LogPanelProps) {
  const { t } = useTranslation();
  const run = useUpdateRun(appId);
  const containerRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  const { data: log, isLoading } = useQuery({
    queryKey: queryKeys.log(appId),
    queryFn: () => opsApi.readLog(appId),
  });

  const live = Boolean(run && !run.finished);
  const lines = run?.lines ?? [];

  // a new run starts at the bottom again
  useEffect(() => {
    stickToBottom.current = true;
  }, [run?.startedAt]);

  // follow the output, unless the user scrolled up to read something
  useEffect(() => {
    if (!stickToBottom.current) return;
    const element = containerRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [lines.length, live]);

  const handleScroll = () => {
    const element = containerRef.current;
    if (!element) return;
    stickToBottom.current =
      element.scrollHeight - element.scrollTop - element.clientHeight <
      STICK_THRESHOLD;
  };

  if (!live) {
    return (
      <div className="space-y-2">
        {isLoading ? (
          <p className="px-2 py-6 text-center text-caption text-fg-3">
            {t("log.loading")}
          </p>
        ) : null}

        {log?.content ? (
          <pre className="max-h-[420px] select-text overflow-auto whitespace-pre-wrap rounded-panel bg-subtle p-3 font-mono text-caption text-fg-2">
            {log.content}
          </pre>
        ) : !isLoading ? (
          <p className="px-2 py-6 text-center text-caption text-fg-3">
            {t("log.empty")}
          </p>
        ) : null}

        {log?.path ? (
          <div className="flex items-center justify-between gap-3">
            <p className="truncate font-mono text-caption text-fg-3">
              {log.path}
            </p>
            <Button
              variant="neutral"
              size="compact"
              onClick={() => revealPath(log.path as string)}
            >
              <FolderOpen className="h-3.5 w-3.5" strokeWidth={1.75} />
              {t("log.revealInFinder")}
            </Button>
          </div>
        ) : null}
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <p className="text-caption text-fg-3">{t("log.live")}</p>
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="max-h-[420px] select-text overflow-auto rounded-panel bg-subtle p-3 font-mono text-caption"
      >
        {lines.length === 0 ? (
          <p className="text-fg-3">{t("log.waitingForOutput")}</p>
        ) : (
          lines.map((line, index) => (
            <p
              key={index}
              className={cn(
                "whitespace-pre-wrap",
                line.stream === "stderr" && "text-fg-2",
                line.stream === "info" && "text-action-text",
                line.stream === "stdout" && "text-fg-1",
              )}
            >
              {line.line}
            </p>
          ))
        )}
      </div>
    </div>
  );
}
