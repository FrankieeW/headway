import type { ReactNode } from "react";

import { DRAG_REGION_ATTR, DRAG_REGION_STYLE } from "@/lib/platform";
import { cn } from "@/lib/utils";

const NO_DRAG = { WebkitAppRegion: "no-drag" } as React.CSSProperties;

export interface PageHeaderProps {
  title: ReactNode;
  /** element in front of the title (an app glyph) */
  icon?: ReactNode;
  /** grey text behind the title (a repository chip) */
  subtitle?: ReactNode;
  /** small element behind the title (a status badge) */
  titleExtra?: ReactNode;
  /** right hand side: the primary action goes last */
  actions?: ReactNode;
  className?: string;
}

/**
 * Content area page header: 52 pixels high, the whole bar drags the window.
 *
 * Only the drag region itself is draggable — buttons keep their own
 * behaviour through `WebkitAppRegion: no-drag`.
 */
export function PageHeader({
  title,
  icon,
  subtitle,
  titleExtra,
  actions,
  className,
}: PageHeaderProps) {
  return (
    <header
      {...DRAG_REGION_ATTR}
      style={DRAG_REGION_STYLE as React.CSSProperties}
      className={cn(
        "flex h-[52px] shrink-0 items-center gap-3 border-b border-border bg-app pe-4 ps-6",
        className,
      )}
    >
      <div
        {...DRAG_REGION_ATTR}
        className="flex min-w-0 shrink-0 items-center gap-2.5"
      >
        {icon ? (
          <span
            aria-hidden="true"
            className="pointer-events-none flex h-[22px] w-[22px] shrink-0 items-center justify-center text-fg-1"
          >
            {icon}
          </span>
        ) : null}
        <h1
          {...DRAG_REGION_ATTR}
          className="m-0 truncate whitespace-nowrap text-title text-fg-1"
        >
          {title}
        </h1>
        {(subtitle || titleExtra) && (
          <div
            className="flex min-w-0 items-center gap-2"
            style={NO_DRAG}
          >
            {subtitle}
            {titleExtra}
          </div>
        )}
      </div>
      <div {...DRAG_REGION_ATTR} className="flex-1" />
      {actions ? (
        <div
          className="flex min-w-0 shrink-0 items-center justify-end gap-2"
          style={NO_DRAG}
        >
          {actions}
        </div>
      ) : null}
    </header>
  );
}
