import { Plus, RefreshCw, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";

import { AppNavItem } from "@/components/shell/AppNavItem";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useApps, useCheckAll } from "@/hooks/useApps";
import { useTokenStatus } from "@/hooks/useSettings";
import { DRAG_REGION_ATTR, DRAG_REGION_STYLE, isMac } from "@/lib/platform";
import { cn } from "@/lib/utils";

const NO_DRAG = { WebkitAppRegion: "no-drag" } as React.CSSProperties;

export interface SidebarProps {
  /** id of the app that is currently shown */
  selectedId: string | null;
  onSelectApp: (id: string) => void;
  onOpenSettings: () => void;
  onAddApp: () => void;
}

/**
 * The left rail: the tracked apps, plus settings and the token state.
 */
export function Sidebar({
  selectedId,
  onSelectApp,
  onOpenSettings,
  onAddApp,
}: SidebarProps) {
  const { t } = useTranslation();
  const { data: apps } = useApps();
  const checkAll = useCheckAll();
  const { data: token } = useTokenStatus();

  const hasToken = Boolean(token && token.source !== "none");

  return (
    <aside className="flex h-52 w-full shrink-0 flex-col sm:h-auto sm:w-[248px] border-r border-border bg-sidebar">
      <header
        {...DRAG_REGION_ATTR}
        style={DRAG_REGION_STYLE as React.CSSProperties}
        className={cn(
          "flex h-[52px] shrink-0 items-center gap-2 border-b border-border pe-2",
          isMac() ? "ps-[78px]" : "ps-3",
        )}
      >
        <span className="min-w-0 flex-1 truncate text-body font-semibold text-fg-1">
          {t("app.name")}
        </span>

        <div className="flex items-center gap-0.5" style={NO_DRAG}>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon-compact"
                onClick={() => checkAll.mutate()}
                disabled={checkAll.isPending}
                aria-label={t("sidebar.checkAll")}
              >
                <RefreshCw
                  className={cn(
                    "h-4 w-4",
                    checkAll.isPending && "animate-spin",
                  )}
                  strokeWidth={1.75}
                />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("sidebar.checkAll")}</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon-compact"
                onClick={onAddApp}
                aria-label={t("sidebar.addRepository")}
              >
                <Plus className="h-4 w-4" strokeWidth={1.75} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("sidebar.addRepository")}</TooltipContent>
          </Tooltip>
        </div>
      </header>

      <nav
        aria-label={t("sidebar.appsLabel")}
        className="flex-1 space-y-0.5 overflow-y-auto p-2"
      >
        {apps?.map((app) => (
          <AppNavItem
            key={app.app.id}
            app={app}
            selected={app.app.id === selectedId}
            onSelect={onSelectApp}
          />
        ))}
      </nav>

      <div className="flex shrink-0 border-t border-border px-2 sm:block sm:p-2">
        <button
          type="button"
          onClick={onOpenSettings}
          className={cn(
            "flex h-9 w-full items-center gap-2.5 rounded-control px-2 text-start text-body transition-colors",
            "text-fg-2 hover:bg-subtle hover:text-fg-1",
          )}
        >
          <Settings className="h-4 w-4 shrink-0" strokeWidth={1.75} />
          <span className="truncate">{t("sidebar.settings")}</span>
        </button>

        <button
          type="button"
          onClick={onOpenSettings}
          className="mt-0.5 flex w-full items-center gap-2 rounded-control px-2 py-1.5 text-start text-caption text-fg-3 transition-colors hover:bg-subtle hover:text-fg-2"
        >
          <span
            aria-hidden="true"
            className={cn(
              "h-1.5 w-1.5 shrink-0 rounded-full",
              hasToken ? "bg-success" : "bg-warning",
            )}
          />
          <span className="min-w-0 truncate">
            {!hasToken
              ? t("sidebar.tokenMissing")
              : token?.login
                ? t("sidebar.tokenReady", { login: token.login })
                : t("sidebar.tokenSaved")}
          </span>
        </button>
      </div>
    </aside>
  );
}
