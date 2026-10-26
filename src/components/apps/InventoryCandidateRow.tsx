import { useTranslation } from "react-i18next";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import type { InventoryCandidate } from "@/lib/api";
import {
  inventoryCategory,
  inventoryPackage,
  inventorySourceKeys,
} from "./importInventory";

export function InventoryCandidateRow({
  candidate,
  selected,
  importing,
  onSelectedChange,
  onUseRepository,
}: {
  readonly candidate: InventoryCandidate;
  readonly selected: boolean;
  readonly importing: boolean;
  readonly onSelectedChange: (selected: boolean) => void;
  readonly onUseRepository?: (repo: string) => void;
}) {
  const { t } = useTranslation();
  const sourceLabel = (category: ReturnType<typeof inventoryCategory>) =>
    t(inventorySourceKeys[category]);
  return (
    <div className="space-y-2 rounded-control bg-subtle p-3">
      <label className="flex items-start gap-3">
        <Checkbox
          aria-label={`${candidate.app.name} · ${sourceLabel(inventoryCategory(candidate.app))}`}
          disabled={
            candidate.already_tracked ||
            candidate.conflicts.length > 0 ||
            importing
          }
          checked={selected}
          onCheckedChange={(checked) => onSelectedChange(checked === true)}
        />
        <span className="min-w-0 flex-1 text-caption">
          <span className="flex flex-wrap items-baseline gap-x-2 gap-y-1 break-words font-medium">
            <span>{candidate.app.name}</span>
            <span className="text-fg-3">
              {sourceLabel(inventoryCategory(candidate.app))}
            </span>
          </span>
          <span className="block break-all text-fg-3">
            {candidate.installed_version || "—"} ·{" "}
            {inventoryPackage(candidate.app)}
          </span>
          {candidate.app.source === "brew" && candidate.app.install_path ? (
            <span className="block break-all text-fg-3">
              {candidate.app.install_path}
            </span>
          ) : null}
          {candidate.already_tracked ? (
            <span className="block text-fg-3">
              {t("roadmap.alreadyTracked")}
            </span>
          ) : null}
          {candidate.conflicts.map((conflict) => (
            <span className="block break-words text-danger-text" key={conflict}>
              {conflict}
            </span>
          ))}
        </span>
      </label>
      {candidate.app.source === "brew"
        ? candidate.repositories?.map((match) => (
            <div key={match.repo} className="space-y-2 text-caption">
              <p className="break-all text-fg-2">
                {t("matches.repository", { repo: match.repo })}
              </p>
              {match.tracked_by.length > 0 ? (
                <p className="break-words text-fg-3">
                  {t("matches.trackedBy", { ids: match.tracked_by.join(", ") })}
                </p>
              ) : null}
              {onUseRepository ? (
                <Button
                  type="button"
                  variant="neutral"
                  className="min-h-11"
                  aria-label={t("matches.useGithubTarget", {
                    target: match.repo,
                  })}
                  disabled={importing || match.tracked_by.length > 0}
                  onClick={() => onUseRepository(match.repo)}
                >
                  {t("matches.useGithub")}
                </Button>
              ) : null}
            </div>
          ))
        : null}
    </div>
  );
}
