import { AddSources } from "./AddSources";
import { useEffect, useMemo, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { AppFormFields } from "@/components/apps/AppFormFields";
import {
  applyPreset,
  appFromForm,
  inspectedBinary,
  sourceRecipeNeedsBuild,
  useAppForm,
} from "@/components/apps/useAppForm";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAddApp, usePresets, useSaveApp } from "@/hooks/useApps";
import { useSettings } from "@/hooks/useSettings";
import { errorMessage, type TrackedApp } from "@/lib/api";

export interface AppFormDialogProps {
  open: boolean;
  /** the app to edit — without it the dialog adds a new one */
  app?: TrackedApp;
  onOpenChange: (open: boolean) => void;
  /** called after a successful save */
  onSaved?: (id: string) => void;
}

const ID_PATTERN = /^[a-z0-9][a-z0-9._-]{0,63}$/;

/**
 * The add / edit dialog of a tracked repository.
 */
export function AppFormDialog({
  open,
  app,
  onOpenChange,
  onSaved,
}: AppFormDialogProps) {
  const { t } = useTranslation();
  const form = useAppForm(app);
  const { data: presets } = usePresets();
  const { data: settings } = useSettings();

  // the errors are shown inside the dialog, not as a toast
  const addApp = useAddApp({ silent: true });
  const saveApp = useSaveApp({ silent: true });

  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [discoveryOpen, setDiscoveryOpen] = useState(false);

  // start over when the dialog opens — not on every refetch of the app
  const wasOpen = useRef(false);
  useEffect(() => {
    if (open !== wasOpen.current) {
      form.reset(app);
      setError(null);
      setDiscoveryOpen(false);
    }
    wasOpen.current = open;
  }, [open, app, form.reset]);

  const destPlaceholder = useMemo(() => {
    const name = form.form.name.trim() || t("form.fileName");
    if (form.form.kind === "app") {
      return `${settings?.apps_dir ?? "/Applications"}/${name}.app`;
    }
    return `${settings?.bin_dir ?? "~/.local/bin"}/${name}`;
  }, [form.form.kind, form.form.name, settings, t]);

  const handleApplyPreset = (preset: NonNullable<typeof presets>[number]) => {
    const bin =
      inspectedBinary(form.inspection) || form.form.repo.split("/").pop() || "";
    const next = applyPreset(form.form, preset, {
      bin,
      name: form.form.name.trim() || bin,
    });
    form.update("presetId", next.presetId);
    if (next.kind !== form.form.kind) form.update("kind", next.kind);
    form.update("build", next.build);
    form.update("artifact", next.artifact);
  };

  const handleSave = async () => {
    setError(null);

    const payload = appFromForm(form.form);
    if (!payload.id || !ID_PATTERN.test(payload.id)) {
      setError(t("form.invalidId"));
      return;
    }
    if (!payload.name) {
      setError(t("form.missingName"));
      return;
    }
    if (payload.source === "github" && !payload.repo) {
      setError(t("form.repoRequired"));
      return;
    }

    if (
      (payload.source === "npm" || payload.source === "brew") &&
      !payload.package
    ) {
      setError(t("form.packageRequired"));
      return;
    }
    if (payload.source === "script" && !payload.update_command) {
      setError(t("form.updateRequired"));
      return;
    }
    if (
      payload.source === "github" &&
      payload.repositories?.some((row) => !row.repo.trim())
    ) {
      setError(t("versions.repositoryRequired"));
      return;
    }
    if (sourceRecipeNeedsBuild(form.form)) {
      setError(t("form.sourceBuildRequired"));
      return;
    }
    setSaving(true);
    try {
      if (app) {
        await saveApp.mutateAsync(payload);
      } else {
        await addApp.mutateAsync(payload);
      }
      onOpenChange(false);
      onSaved?.(payload.id);
    } catch (caught) {
      // backend validation errors stay in the dialog
      setError(errorMessage(caught));
    } finally {
      setSaving(false);
    }
  };

  const busy = saving || addApp.isPending || saveApp.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[640px]">
        <DialogHeader>
          <DialogTitle>
            {app ? t("form.titleEdit") : t("form.titleAdd")}
          </DialogTitle>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-1">
          {error ? (
            <Alert className="mb-4 border-danger/40 bg-danger-soft">
              <AlertDescription className="text-caption text-danger-text">
                {error}
              </AlertDescription>
            </Alert>
          ) : null}

          {!app ? (
            <AddSources
              key={String(open)}
              onPanelChange={setDiscoveryOpen}
              onRecipe={form.reset}
              onImported={(id) => {
                onOpenChange(false);
                if (id) onSaved?.(id);
              }}
            />
          ) : null}
          {!discoveryOpen ? (
            <AppFormFields
              form={form.form}
              mode={app ? "edit" : "add"}
              presets={presets}
              inspection={form.inspection}
              lookup={form.lookup}
              inspecting={form.inspecting}
              inspectionError={form.inspectionError}
              destPlaceholder={destPlaceholder}
              onInspect={() =>
                void form.inspect(
                  form.form.source === "github"
                    ? form.form.repo
                    : form.form.package,
                )
              }
              onUpdate={form.update}
              onSetName={form.setName}
              onSetId={form.setId}
              onApplyPreset={handleApplyPreset}
            />
          ) : null}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            {t("common.cancel")}
          </Button>
          {!discoveryOpen ? (
            <Button
              variant="solid"
              onClick={() => void handleSave()}
              disabled={busy}
            >
              {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              {t("common.save")}
            </Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
