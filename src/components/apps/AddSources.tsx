import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  appsApi,
  DEFAULT_UPDATE_POLICY,
  errorMessage,
  type TrackedApp,
} from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";

export function AddSources({
  onRecipe,
  onImported,
  onPanelChange,
}: {
  readonly onRecipe: (app: TrackedApp) => void;
  readonly onImported: (id?: string) => void;
  readonly onPanelChange?: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [panel, setPanelState] = useState<"inventory" | "recipes" | null>(null);
  const setPanel = (next: "inventory" | "recipes" | null) => {
    setPanelState(next);
    onPanelChange?.(next !== null);
  };
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const inventory = useQuery({
    queryKey: ["inventory"],
    queryFn: () => appsApi.discover(["brew", "npm"]),
    enabled: panel === "inventory",
    retry: false,
  });
  const recipes = useQuery({
    queryKey: ["recipes"],
    queryFn: appsApi.recipes,
    enabled: panel === "recipes",
  });
  const candidates = inventory.data?.candidates ?? [];
  const eligible = candidates.filter(
    (candidate) =>
      selected.has(candidate.key) &&
      !candidate.already_tracked &&
      candidate.conflicts.length === 0,
  );
  const importing = useMutation({
    mutationFn: (apps: TrackedApp[]) => appsApi.import(apps),
    onSuccess: async (_result, apps) => {
      await client.invalidateQueries({ queryKey: queryKeys.apps });
      await client.invalidateQueries({ queryKey: ["inventory"] });
      setSelected(new Set());
      onImported(apps[0]?.id);
    },
  });
  return (
    <section className="mb-5 space-y-3 border-b border-border pb-4">
      <div className="flex flex-wrap gap-2">
        <Button
          variant="neutral"
          onClick={() => setPanel(panel === "inventory" ? null : "inventory")}
          aria-expanded={panel === "inventory"}
        >
          {t("roadmap.importExisting")}
        </Button>
        <Button
          variant="neutral"
          onClick={() => setPanel(panel === "recipes" ? null : "recipes")}
          aria-expanded={panel === "recipes"}
        >
          {t("roadmap.recipes")}
        </Button>
      </div>
      {panel === "inventory" ? (
        <div className="space-y-3">
          <p className="text-caption text-fg-3">{t("roadmap.importHint")}</p>
          {inventory.isPending ? (
            <p role="status">{t("common.loading")}</p>
          ) : null}
          {inventory.error ? (
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(inventory.error)}{" "}
              <Button
                variant="neutral"
                onClick={() => void inventory.refetch()}
              >
                {t("roadmap.retry")}
              </Button>
            </p>
          ) : null}
          {inventory.data?.errors.map(({ source, error }) => (
            <p
              role="alert"
              key={source}
              className="break-words text-caption text-danger-text"
            >
              {source}: {errorMessage(error)}
            </p>
          ))}
          {inventory.data && candidates.length === 0 ? (
            <p className="text-caption">{t("roadmap.inventoryEmpty")}</p>
          ) : null}
          {candidates.map((candidate) => (
            <label
              key={candidate.key}
              className="flex items-start gap-3 rounded-control bg-subtle p-3"
            >
              <Checkbox
                aria-label={candidate.app.name}
                disabled={
                  candidate.already_tracked ||
                  candidate.conflicts.length > 0 ||
                  importing.isPending
                }
                checked={selected.has(candidate.key)}
                onCheckedChange={(checked) =>
                  setSelected((current) => {
                    const next = new Set(current);
                    if (checked === true) next.add(candidate.key);
                    else next.delete(candidate.key);
                    return next;
                  })
                }
              />
              <span className="min-w-0 flex-1 text-caption">
                <span className="block break-words font-medium">
                  {candidate.app.name} · {candidate.app.source}
                </span>
                <span className="block break-all text-fg-3">
                  {candidate.installed_version || "—"} · {candidate.app.id}
                </span>
                {candidate.already_tracked ? (
                  <span className="block text-fg-3">
                    {t("roadmap.alreadyTracked")}
                  </span>
                ) : null}
                {candidate.conflicts.map((conflict) => (
                  <span
                    className="block break-words text-danger-text"
                    key={conflict}
                  >
                    {conflict}
                  </span>
                ))}
              </span>
            </label>
          ))}
          {importing.error ? (
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(importing.error)}
            </p>
          ) : null}
          <Button
            variant="solid"
            disabled={eligible.length === 0 || importing.isPending}
            onClick={() =>
              importing.mutate(
                eligible.map(({ app }) => ({
                  ...app,
                  auto_update: false,
                  update_policy: DEFAULT_UPDATE_POLICY,
                })),
              )
            }
          >
            {t("roadmap.importSelected", { count: eligible.length })}
          </Button>
        </div>
      ) : null}
      {panel === "recipes" ? (
        <div className="space-y-3">
          {recipes.isPending ? (
            <p role="status">{t("common.loading")}</p>
          ) : null}
          {recipes.error ? (
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(recipes.error)}{" "}
              <Button variant="neutral" onClick={() => void recipes.refetch()}>
                {t("roadmap.retry")}
              </Button>
            </p>
          ) : null}
          {recipes.data?.length === 0 ? (
            <p className="text-caption">{t("roadmap.recipesEmpty")}</p>
          ) : null}
          {recipes.data?.map((recipe) => (
            <div
              key={recipe.id}
              className="space-y-2 rounded-control bg-subtle p-3"
            >
              <h3 className="text-body font-medium">{recipe.name}</h3>
              <p className="break-words text-caption text-fg-3">
                {recipe.description}
              </p>
              <Button
                variant="neutral"
                onClick={() => {
                  onRecipe({
                    ...recipe.app,
                    auto_update: false,
                    update_policy: DEFAULT_UPDATE_POLICY,
                  });
                  setPanel(null);
                }}
              >
                {t("roadmap.useRecipe")}
              </Button>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
