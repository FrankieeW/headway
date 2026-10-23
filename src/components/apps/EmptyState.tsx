import { Github } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";

export interface EmptyStateProps {
  onAddApp: () => void;
}

/**
 * Shown when nothing is tracked yet.
 */
export function EmptyState({ onAddApp }: EmptyStateProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-panel bg-subtle text-fg-2">
        <Github className="h-6 w-6" strokeWidth={1.5} />
      </div>
      <h1 className="text-page font-semibold text-fg-1">
        {t("empty.title")}
      </h1>
      <p className="max-w-sm text-caption text-fg-2">
        {t("empty.description")}
      </p>
      <Button variant="solid" size="regular" onClick={onAddApp}>
        {t("empty.addRepository")}
      </Button>
      <p className="mt-1 font-mono text-caption text-fg-3">
        {t("empty.cliHint")}
      </p>
    </div>
  );
}
