import { Info } from "lucide-react";
export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2">
        <span className="text-caption font-medium text-fg-2">{label}</span>
        {hint ? (
          <span className="flex items-center gap-1 text-caption text-fg-3">
            <Info className="h-3 w-3 shrink-0" strokeWidth={1.75} />
            {hint}
          </span>
        ) : null}
      </div>
      {children}
    </div>
  );
}
