import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  errorMessage,
  opsApi,
  type AppView,
  type BackupEntry,
} from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";

export function RecoverySection({ view }: { readonly view: AppView }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [selected, setSelected] = useState<BackupEntry | null>(null);
  const managed =
    view.app.source === "github" &&
    (view.app.install.method === "asset" ||
      Boolean(view.app.install.artifact?.trim()));
  const backups = useQuery({
    queryKey: ["backups", view.app.id],
    queryFn: () => opsApi.backups(view.app.id),
    enabled: managed,
  });
  const restore = useMutation({
    mutationFn: (backup: BackupEntry) =>
      opsApi.restoreBackup(view.app.id, backup.id),
    onSuccess: () => setSelected(null),
    onSettled: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: queryKeys.apps }),
        client.invalidateQueries({ queryKey: queryKeys.app(view.app.id) }),
        client.invalidateQueries({ queryKey: queryKeys.history(view.app.id) }),
        client.invalidateQueries({ queryKey: ["backups", view.app.id] }),
      ]);
    },
  });
  if (!managed) return null;
  const entries = [...(backups.data ?? [])]
    .sort((a, b) => b.created_at.localeCompare(a.created_at))
    .slice(0, 2);
  return (
    <section className="mt-4 space-y-3 rounded-panel border border-border bg-surface p-4">
      <h2 className="text-section">{t("roadmap.recovery")}</h2>
      <p className="text-caption text-fg-3">{t("roadmap.recoveryHint")}</p>
      {backups.isPending ? <p role="status">{t("common.loading")}</p> : null}
      {backups.error ? (
        <p role="alert" className="text-caption text-danger-text">
          {errorMessage(backups.error)}{" "}
          <Button variant="neutral" onClick={() => void backups.refetch()}>
            {t("roadmap.retry")}
          </Button>
        </p>
      ) : null}
      {!backups.isPending && !backups.error && entries.length === 0 ? (
        <p className="text-caption text-fg-3">{t("roadmap.noBackups")}</p>
      ) : null}
      {entries.map((backup) => (
        <div
          key={backup.id}
          className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-3"
        >
          <div className="min-w-0 flex-1 text-caption">
            <p>
              {backup.installed?.version || backup.installed?.id || backup.id}
            </p>
            <p className="text-fg-3">
              {new Date(backup.created_at).toLocaleString()}
            </p>
            <p className="break-all font-mono text-fg-3">
              {backup.artifact_path}
            </p>
          </div>
          <Button
            variant="neutral"
            disabled={view.busy || restore.isPending}
            onClick={() => {
              restore.reset();
              setSelected(backup);
            }}
          >
            {t("roadmap.restore")}
          </Button>
        </div>
      ))}
      {restore.error ? (
        <p role="alert" className="text-caption text-danger-text">
          {errorMessage(restore.error)}
        </p>
      ) : null}
      {restore.data ? (
        <p role="status" className="break-all text-caption">
          {t("roadmap.restored", { path: restore.data.restored_path })}
        </p>
      ) : null}
      <ConfirmDialog
        isOpen={selected !== null}
        title={t("roadmap.restore")}
        message={[
          t("roadmap.restoreConfirm", {
            name: view.app.name,
            version:
              selected?.installed?.version ||
              selected?.installed?.id ||
              selected?.id,
          }),
          restore.error ? errorMessage(restore.error) : null,
        ]
          .filter(Boolean)
          .join("\n\n")}
        confirmText={t("roadmap.restore")}
        variant="default"
        pending={restore.isPending}
        onCancel={() => setSelected(null)}
        onConfirm={() => {
          if (selected && !view.busy) restore.mutate(selected);
        }}
      />
    </section>
  );
}
