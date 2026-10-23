import { AppWindow, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";

import { StatusBadge } from "@/components/apps/StatusBadge";
import type { AppView } from "@/lib/api";
import { trackLabel } from "@/lib/format";
import { cn } from "@/lib/utils";

export interface AppNavItemProps {
  app: AppView;
  selected: boolean;
  onSelect: (id: string) => void;
}

/**
 * One app in the sidebar: glyph, name, the tracked source and the status.
 */
export function AppNavItem({ app, selected, onSelect }: AppNavItemProps) {
  const { t } = useTranslation();
  const Glyph = app.app.kind === "cli" ? Terminal : AppWindow;

  return (
    <button
      type="button"
      onClick={() => onSelect(app.app.id)}
      aria-current={selected ? "true" : undefined}
      className={cn(
        "flex h-11 w-full items-center gap-2.5 rounded-control px-2 text-start transition-colors",
        selected
          ? "bg-selected"
          : "hover:bg-subtle focus-visible:bg-subtle",
      )}
    >
      <span
        aria-hidden="true"
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-[6px] bg-surface text-fg-2 ring-1 ring-border"
      >
        <Glyph className="h-3.5 w-3.5" strokeWidth={1.75} />
      </span>

      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-body font-medium text-fg-1">
          {app.app.name}
        </span>
        <span className="truncate text-caption text-fg-3">
          {app.app.repo} · {trackLabel(app.app.track, t)}
        </span>
      </span>

      <StatusBadge
        status={app.status}
        pendingCount={app.pending_count}
        variant="compact"
      />
    </button>
  );
}
