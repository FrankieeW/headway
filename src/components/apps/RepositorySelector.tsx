import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  appsApi,
  errorMessage,
  type AppView,
  type RepositoryOption,
} from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";

export function RepositorySelector({ view }: { readonly view: AppView }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [selected, setSelected] = useState(
    view.app.source === "github" ? view.app.repo : "",
  );
  const [confirming, setConfirming] = useState(false);
  const activate = useMutation({
    mutationFn: () => appsApi.selectRepository(view.app.id, selected),
    onSuccess: async () => {
      setConfirming(false);
      await Promise.all([
        client.invalidateQueries({ queryKey: queryKeys.apps }),
        client.invalidateQueries({ queryKey: queryKeys.app(view.app.id) }),
        client.invalidateQueries({ queryKey: ["commits", view.app.id] }),
        client.invalidateQueries({ queryKey: ["source-storage", view.app.id] }),
      ]);
    },
  });
  if (view.app.source !== "github") return null;
  const app = view.app;
  const current: RepositoryOption = {
    repo: app.repo,
    label:
      app.repositories?.find(
        (row) => row.repo.toLowerCase() === app.repo.toLowerCase(),
      )?.label || t("versions.currentRepository"),
    git_url: app.git_url,
    branch: app.track.mode === "commit" ? app.track.branch : null,
  };
  const rows = [current, ...(app.repositories ?? [])].filter(
    (row, i, all) =>
      all.findIndex(
        (item) => item.repo.toLowerCase() === row.repo.toLowerCase(),
      ) === i,
  );
  return (
    <section className="space-y-2 rounded-panel border border-border bg-surface p-4">
      <h2 className="text-section">{t("versions.activeRepository")}</h2>
      <div className="flex flex-wrap gap-2">
        <div className="min-w-0 flex-1">
          <Select
            value={selected}
            onValueChange={(repo) => {
              activate.reset();
              setSelected(repo);
            }}
          >
            <SelectTrigger aria-label={t("versions.repositoryChoice")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {rows.map((row) => (
                <SelectItem key={row.repo} value={row.repo}>
                  {row.label} · {row.repo}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <Button
          variant="neutral"
          disabled={view.busy || selected === app.repo || activate.isPending}
          onClick={() => setConfirming(true)}
        >
          {t("versions.activateRepository")}
        </Button>
      </div>
      <p className="text-caption text-fg-3">{t("versions.activationHint")}</p>
      {activate.error ? (
        <p role="alert" className="break-words text-caption text-danger-text">
          {errorMessage(activate.error)}
        </p>
      ) : null}
      <ConfirmDialog
        isOpen={confirming}
        title={t("versions.activateRepository")}
        message={[
          t("versions.activateConfirm", { repo: selected }),
          activate.error ? errorMessage(activate.error) : null,
        ]
          .filter(Boolean)
          .join("\n\n")}
        variant="default"
        pending={activate.isPending}
        onConfirm={() => activate.mutate()}
        onCancel={() => setConfirming(false)}
      />
    </section>
  );
}
