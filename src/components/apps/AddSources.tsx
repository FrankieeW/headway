import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { SegmentedControl } from "@/components/ui/segmented-control";
import {
  filterInventory,
  inventoryCategory,
  inventoryPackage,
  inventorySourceKeys,
  type InventoryFilter,
} from "./importInventory";
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
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<InventoryFilter>("all");
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
  const sourceLabel = (category: ReturnType<typeof inventoryCategory>) =>
    t(inventorySourceKeys[category]);
  const visible = filterInventory(candidates, filter, search, sourceLabel);
  const matching = visible.filter(
    (candidate) =>
      !candidate.already_tracked && candidate.conflicts.length === 0,
  );
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
  const visibleKeys = new Set(visible.map((candidate) => candidate.key));
  const hiddenSelected = eligible.filter(
    (candidate) => !visibleKeys.has(candidate.key),
  ).length;
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
          <label
            htmlFor="installed-inventory-search"
            className="block text-caption font-medium"
          >
            {t("roadmap.inventorySearch")}
          </label>
          <Input
            id="installed-inventory-search"
            type="search"
            className="min-h-11"
            aria-label={t("roadmap.inventorySearch")}
            placeholder={t("roadmap.inventorySearchPlaceholder")}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          <div className="flex flex-wrap items-center gap-2">
            <SegmentedControl<InventoryFilter>
              aria-label={t("roadmap.inventoryFilter")}
              value={filter}
              onValueChange={setFilter}
              className="h-auto max-w-full flex-wrap"
              items={(
                [
                  { value: "all", label: t("roadmap.inventoryAll") },
                  { value: "cask", label: t("roadmap.inventoryFilterCask") },
                  {
                    value: "formula",
                    label: t("roadmap.inventoryFilterFormula"),
                  },
                  { value: "npm", label: t("roadmap.inventoryFilterNpm") },
                ] as const
              ).map((item) => ({
                ...item,
                className: "min-h-11 px-3 text-caption",
              }))}
            />
            {search || filter !== "all" ? (
              <Button
                variant="neutral"
                className="min-h-11"
                onClick={() => {
                  setSearch("");
                  setFilter("all");
                }}
              >
                {t("roadmap.inventoryClearFilters")}
              </Button>
            ) : null}
          </div>
          {inventory.data ? (
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p aria-live="polite" className="text-caption text-fg-3">
                {t("roadmap.inventoryCount", {
                  shown: visible.length,
                  total: candidates.length,
                })}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="neutral"
                  className="min-h-11"
                  disabled={matching.length === 0 || importing.isPending}
                  onClick={() =>
                    setSelected(
                      (current) =>
                        new Set([
                          ...current,
                          ...matching.map((candidate) => candidate.key),
                        ]),
                    )
                  }
                >
                  {t("roadmap.inventorySelectMatching")}
                </Button>
                {selected.size > 0 ? (
                  <Button
                    variant="neutral"
                    className="min-h-11"
                    disabled={importing.isPending}
                    onClick={() => setSelected(new Set())}
                  >
                    {t("roadmap.inventoryClearSelection")}
                  </Button>
                ) : null}
              </div>
            </div>
          ) : null}
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
          {inventory.data && candidates.length > 0 && visible.length === 0 ? (
            <p className="text-caption">{t("roadmap.inventoryNoMatches")}</p>
          ) : null}
          <div
            id="installed-inventory-list"
            role="group"
            aria-label={t("roadmap.inventoryResults")}
            className="max-h-72 space-y-2 overflow-y-auto"
          >
            {visible.map((candidate) => (
              <label
                key={candidate.key}
                className="flex items-start gap-3 rounded-control bg-subtle p-3"
              >
                <Checkbox
                  aria-label={`${candidate.app.name} · ${sourceLabel(inventoryCategory(candidate.app))}`}
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
                  {candidate.app.source === "brew" &&
                  candidate.app.install_path ? (
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
          </div>
          {hiddenSelected > 0 ? (
            <p aria-live="polite" className="text-caption text-fg-3">
              {t("roadmap.inventoryHiddenSelection", { count: hiddenSelected })}
            </p>
          ) : null}
          {importing.error ? (
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(importing.error)}
            </p>
          ) : null}
          <Button
            variant="solid"
            className="min-h-11"
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
