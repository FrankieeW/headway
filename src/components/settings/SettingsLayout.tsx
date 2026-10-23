import type { ReactNode } from "react";

import { HelpTip } from "@/components/ui/help-tip";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

/**
 * The layout of the settings page (v7): a section title, then a card with
 * one row per setting — name on the left, control on the right.
 */
export function SettingsBlock({
  id,
  title,
  help,
  actions,
  children,
  className,
}: {
  id?: string;
  title?: ReactNode;
  /** the "?" next to the title: rules and terms, not consequences */
  help?: { title: string; body: ReactNode };
  /** actions on the right of the section title */
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section id={id} className={cn("space-y-3", className)}>
      {(title || actions) && (
        <div className="flex min-h-7 items-center gap-2">
          {title ? <h2 className="m-0 text-section text-fg-1">{title}</h2> : null}
          {help ? <HelpTip title={help.title}>{help.body}</HelpTip> : null}
          {actions ? (
            <div className="ms-auto flex items-center gap-2">{actions}</div>
          ) : null}
        </div>
      )}
      {children}
    </section>
  );
}

/** A group of setting rows: white card, a thin line between the rows. */
export function SettingsCard({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "divide-y divide-border overflow-hidden rounded-panel border border-border bg-surface",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function SettingsRow({
  label,
  help,
  description,
  control,
  htmlFor,
  children,
  className,
}: {
  label: ReactNode;
  help?: { title: string; body: ReactNode };
  /** only real data (paths, addresses) belongs under the name */
  description?: ReactNode;
  control?: ReactNode;
  htmlFor?: string;
  /** content below the control, spanning the whole row */
  children?: ReactNode;
  className?: string;
}) {
  const Label = htmlFor ? "label" : "div";

  return (
    <div className={cn("px-4 py-3", className)}>
      <div className="flex min-h-[28px] items-center gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1">
            <Label
              htmlFor={htmlFor}
              className="text-body font-medium text-fg-1"
            >
              {label}
            </Label>
            {help ? <HelpTip title={help.title}>{help.body}</HelpTip> : null}
          </div>
          {description ? (
            <div className="mt-0.5 break-all text-caption text-fg-2">
              {description}
            </div>
          ) : null}
        </div>
        {control ? (
          <div className="flex shrink-0 items-center gap-2">{control}</div>
        ) : null}
      </div>
      {children ? <div className="mt-2.5">{children}</div> : null}
    </div>
  );
}

export function SettingsSwitchRow({
  label,
  help,
  description,
  checked,
  onCheckedChange,
  disabled,
}: {
  label: string;
  help?: { title: string; body: ReactNode };
  description?: ReactNode;
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <SettingsRow
      label={label}
      help={help}
      description={description}
      control={
        <Switch
          checked={checked}
          onCheckedChange={onCheckedChange}
          disabled={disabled}
          aria-label={label}
        />
      }
    />
  );
}

/** The body of the settings page: 760 wide, sections 28 apart. */
export function SettingsBody({ children }: { children: ReactNode }) {
  return (
    <div className="mx-auto w-full max-w-[760px] space-y-7 px-6 pb-10 pt-6">
      {children}
    </div>
  );
}
