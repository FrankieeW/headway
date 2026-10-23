import {
  AppWindow,
  Check,
  Copy,
  Github,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Terminal,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { StatusBadge } from "@/components/apps/StatusBadge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { AppView } from "@/lib/api";
import { systemApi } from "@/lib/api";
import { DRAG_REGION_ATTR, DRAG_REGION_STYLE } from "@/lib/platform";
import { cn } from "@/lib/utils";

const NO_DRAG = { WebkitAppRegion: "no-drag" } as React.CSSProperties;

export interface AppHeaderProps {
  app: AppView;
  checking: boolean;
  onCheck: () => void;
  onEdit: () => void;
  onMarkLatestInstalled: () => void;
  onRemove: () => void;
}

/**
 * The header of the app detail page: name, status, repository and actions.
 */
export function AppHeader({
  app,
  checking,
  onCheck,
  onEdit,
  onMarkLatestInstalled,
  onRemove,
}: AppHeaderProps) {
  const { t } = useTranslation();
  const Glyph = app.app.kind === "cli" ? Terminal : AppWindow;
  const repoUrl = `https://github.com/${app.app.repo}`;

  const copyCliCommand = async () => {
    const command = `headway update ${app.app.id}`;
    try {
      await navigator.clipboard.writeText(command);
      toast.success(t("toast.copied", { value: command }));
    } catch {
      toast.error(t("toast.copyFailed"));
    }
  };

  return (
    <header
      {...DRAG_REGION_ATTR}
      style={DRAG_REGION_STYLE as React.CSSProperties}
      className="flex h-[52px] shrink-0 items-center gap-3 border-b border-border bg-app pe-4 ps-6"
    >
      <div
        {...DRAG_REGION_ATTR}
        className="flex min-w-0 shrink items-center gap-2.5"
      >
        <span
          aria-hidden="true"
          className="pointer-events-none flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-[6px] bg-surface text-fg-1 ring-1 ring-border"
        >
          <Glyph className="h-3.5 w-3.5" strokeWidth={1.75} />
        </span>

        <h1
          {...DRAG_REGION_ATTR}
          className="m-0 truncate whitespace-nowrap text-title text-fg-1"
        >
          {app.app.name}
        </h1>

        <span style={NO_DRAG}>
          <StatusBadge
            status={app.status}
            pendingCount={app.pending_count}
            variant="full"
          />
        </span>

        <button
          type="button"
          onClick={() => void systemApi.openUrl(repoUrl)}
          title={repoUrl}
          style={NO_DRAG}
          className="inline-flex h-6 max-w-[220px] items-center gap-1.5 rounded-full bg-subtle px-2 text-caption text-fg-2 transition-colors hover:bg-selected hover:text-fg-1"
        >
          <Github className="h-3 w-3 shrink-0" strokeWidth={1.75} />
          <span className="truncate">{app.app.repo}</span>
        </button>
      </div>

      <div {...DRAG_REGION_ATTR} className="flex-1" />

      <div
        className="flex min-w-0 shrink-0 items-center justify-end gap-2"
        style={NO_DRAG}
      >
        <Button
          variant="neutral"
          size="compact"
          onClick={onCheck}
          disabled={checking}
        >
          <RefreshCw
            className={cn("h-3.5 w-3.5", checking && "animate-spin")}
            strokeWidth={2}
          />
          {checking ? t("appPage.checking") : t("appPage.check")}
        </Button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="quiet"
              size="icon-compact"
              aria-label={t("appPage.moreActions")}
            >
              <MoreHorizontal className="h-4 w-4" strokeWidth={1.75} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="min-w-[200px]">
            <DropdownMenuItem onSelect={onEdit}>
              <Pencil className="h-3.5 w-3.5" strokeWidth={1.75} />
              {t("appPage.edit")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={onMarkLatestInstalled}>
              <Check className="h-3.5 w-3.5" strokeWidth={1.75} />
              {t("appPage.markLatestInstalled")}
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => void systemApi.openUrl(repoUrl)}
            >
              <Github className="h-3.5 w-3.5" strokeWidth={1.75} />
              {t("appPage.openOnGitHub")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void copyCliCommand()}>
              <Copy className="h-3.5 w-3.5" strokeWidth={1.75} />
              {t("appPage.copyCliCommand")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onSelect={onRemove}
              className="text-danger-text focus:bg-danger-soft focus:text-danger-text"
            >
              {t("appPage.remove")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </header>
  );
}
