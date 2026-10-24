import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { Switch } from "@/components/ui/switch";
import { Field } from "./FormField";
import {
  RevisionAssetFields,
  RevisionSourceFields,
} from "./RevisionRecipeFields";
import {
  revisionDraft,
  requestFromDraft,
  type RevisionDraft,
} from "./revisionDraft";
import { appsApi, errorMessage, systemApi, type AppView } from "@/lib/api";
import {
  useRevisionInstall,
  type RevisionJob,
} from "@/hooks/useRevisionInstall";

export function RevisionDialog({
  view,
  reference,
  onClose,
  onVariant,
}: {
  readonly view: AppView;
  readonly reference: string;
  readonly onClose: () => void;
  readonly onVariant: (id: string) => void;
}) {
  const { t } = useTranslation();
  const options = useQuery({
    queryKey: [
      "revision-options",
      view.app.id,
      view.app.source === "github" ? view.app.repo : "",
      reference,
    ],
    queryFn: () => appsApi.revisionOptions(view.app.id, reference),
    staleTime: 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const [draft, setDraft] = useState<RevisionDraft | null>(null);
  const initialised = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const install = useRevisionInstall();
  const [reviewedJob, setReviewedJob] = useState<RevisionJob | null>(null);
  const plan = useMutation({
    mutationFn: (job: RevisionJob) =>
      "variant" in job
        ? appsApi.planVariant(job.parentId, job.variant)
        : appsApi.planRevision(job.id, job.revision),
  });
  useEffect(() => {
    if (options.data && !initialised.current) {
      initialised.current = true;
      setDraft(revisionDraft(view.app, options.data));
    }
  }, [options.data, view.app]);
  const result = options.data;
  const activeInstall = view.app.source === "github" ? view.app.install : null;
  const configuredAllowed =
    activeInstall?.method === "source" || Boolean(result?.can_download);
  const review = () => {
    setError(null);
    if (!draft || !result) return;
    if (draft.method === "source" && !draft.source.build.trim()) {
      setError(t("versions.buildRequired"));
      return;
    }
    if (draft.method === "asset" && !result.can_download) {
      setError(t("versions.noReleaseAsset"));
      return;
    }
    if (draft.separate && !/^[a-z0-9][a-z0-9._-]{0,63}$/.test(draft.id)) {
      setError(t("form.invalidId"));
      return;
    }
    const source =
      draft.method === "source"
        ? draft.source
        : draft.method === "configured" && activeInstall?.method === "source"
          ? activeInstall
          : null;
    if (draft.separate && source && !source.artifact?.trim()) {
      setError(t("versions.variantArtifactRequired"));
      return;
    }
    const revision = requestFromDraft(draft, result.commit, result.release_tag);
    const job: RevisionJob = draft.separate
      ? {
          parentId: view.app.id,
          variant: {
            id: draft.id.trim(),
            name: draft.name.trim() || undefined,
            revision,
          },
        }
      : { id: view.app.id, revision };
    plan.mutate(job, {
      onSuccess: () => {
        setReviewedJob(job);
        setConfirming(true);
      },
    });
  };
  const execute = () => {
    if (!reviewedJob || view.busy) return;
    install.mutate(reviewedJob, {
      onSuccess: () => {
        setConfirming(false);
        onClose();
        if ("variant" in reviewedJob) onVariant(reviewedJob.variant.id);
      },
    });
  };
  const reviewedVariant =
    reviewedJob && "variant" in reviewedJob ? reviewedJob.variant : null;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !install.isPending) onClose();
      }}
    >
      <DialogContent className="max-w-[640px]">
        <DialogHeader>
          <DialogTitle>{t("versions.installRevision")}</DialogTitle>
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 pb-2">
          <p className="break-all font-mono text-caption text-fg-3">
            {reference}
          </p>
          {options.isFetching ? (
            <p role="status" className="text-caption">
              {t("common.loading")}
            </p>
          ) : null}
          {options.error ? (
            <div
              role="alert"
              className="space-y-2 text-caption text-danger-text"
            >
              <p>{errorMessage(options.error)}</p>
              <Button variant="neutral" onClick={() => void options.refetch()}>
                {t("roadmap.retry")}
              </Button>
            </div>
          ) : null}
          {result && draft ? (
            <fieldset
              disabled={plan.isPending || install.isPending}
              className="space-y-4"
            >
              <div className="space-y-1 rounded-control bg-subtle p-3">
                <p className="break-words text-body">
                  {result.candidate.title}
                </p>
                <p className="break-all font-mono text-caption">
                  {result.commit}
                </p>
                <p className="break-all text-caption text-fg-3">
                  {result.release_tag
                    ? t("versions.actualRelease", { tag: result.release_tag })
                    : t("versions.noRelease")}
                </p>
                {result.release_url ? (
                  <Button
                    variant="ghost"
                    onClick={() => {
                      if (result.release_url)
                        void systemApi.openUrl(result.release_url);
                    }}
                  >
                    {t("versions.openRelease")}
                  </Button>
                ) : null}
              </div>
              <Field label={t("versions.installMethod")}>
                <SegmentedControl
                  aria-label={t("versions.installMethod")}
                  size="sm"
                  value={draft.method}
                  onValueChange={(method) => setDraft({ ...draft, method })}
                  items={[
                    {
                      value: "configured",
                      label: t("versions.configured"),
                      disabled: !configuredAllowed,
                    },
                    { value: "source", label: t("form.installSource") },
                    {
                      value: "asset",
                      label: t("form.installAsset"),
                      disabled: !result.can_download,
                    },
                  ]}
                  className="flex-wrap h-auto"
                />
              </Field>
              {draft.method === "configured" ? (
                <pre className="whitespace-pre-wrap break-all rounded-control bg-subtle p-3 font-mono text-caption">
                  {activeInstall?.method === "source"
                    ? activeInstall.build
                    : activeInstall?.pattern || t("versions.automaticAsset")}
                </pre>
              ) : null}
              {draft.method === "source" ? (
                <>
                  <p className="text-caption text-fg-3">
                    {result.source_build
                      ? t("versions.editRecipe")
                      : t("versions.missingRecipe")}
                  </p>
                  <RevisionSourceFields
                    value={draft.source}
                    onChange={(source) => setDraft({ ...draft, source })}
                  />
                </>
              ) : null}
              {draft.method === "asset" ? (
                <RevisionAssetFields
                  value={draft.asset}
                  onChange={(asset) => setDraft({ ...draft, asset })}
                />
              ) : null}
              <label className="flex items-center justify-between gap-3 border-t border-border pt-3">
                <span className="text-body">
                  {t("versions.separateInstall")}
                </span>
                <Switch
                  aria-label={t("versions.separateInstall")}
                  checked={draft.separate}
                  onCheckedChange={(separate) =>
                    setDraft({ ...draft, separate })
                  }
                />
              </label>
              {draft.separate ? (
                <>
                  <p className="text-caption text-fg-2">
                    {t("versions.sharedData")}
                  </p>
                  <p className="text-caption text-fg-3">
                    {t("versions.independentDestination")}
                  </p>
                  <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <Field label={t("versions.variantId")}>
                      <Input
                        aria-label={t("versions.variantId")}
                        value={draft.id}
                        onChange={(event) =>
                          setDraft({ ...draft, id: event.target.value })
                        }
                        className="font-mono"
                      />
                    </Field>
                    <Field label={t("form.name")}>
                      <Input
                        aria-label={t("form.name")}
                        value={draft.name}
                        onChange={(event) =>
                          setDraft({ ...draft, name: event.target.value })
                        }
                      />
                    </Field>
                  </div>
                </>
              ) : (
                <p className="text-caption text-fg-3">
                  {t("versions.replaceHint")}
                </p>
              )}
            </fieldset>
          ) : null}
          {plan.error ? (
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(plan.error)}
            </p>
          ) : null}
          {error ? (
            <p role="alert" className="text-caption text-danger-text">
              {error}
            </p>
          ) : null}
          {install.error && !confirming ? (
            <p role="alert" className="text-caption text-danger-text">
              {errorMessage(install.error)}
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button
            variant="outline"
            disabled={install.isPending}
            onClick={onClose}
          >
            {t("common.cancel")}
          </Button>
          <Button
            variant="solid"
            disabled={
              !draft ||
              options.isFetching ||
              Boolean(options.error) ||
              view.busy ||
              install.isPending ||
              plan.isPending
            }
            onClick={review}
          >
            {t(plan.isPending ? "common.loading" : "versions.reviewInstall")}
          </Button>
        </DialogFooter>
        <ConfirmDialog
          scrollable
          isOpen={confirming}
          title={t(
            reviewedVariant
              ? "versions.confirmVariant"
              : "versions.confirmReplace",
          )}
          message={[
            t(
              reviewedVariant
                ? "versions.variantConfirm"
                : "versions.replaceConfirm",
              {
                name: view.app.name,
                commit: plan.data?.to.sha ?? plan.data?.to.id,
                id: reviewedVariant?.id,
              },
            ),
            reviewedVariant ? t("versions.sharedData") : null,
            ...(plan.data?.steps ?? []),
            install.error ? errorMessage(install.error) : null,
          ]
            .filter(Boolean)
            .join("\n\n")}
          variant="default"
          confirmText={t("versions.startInstall")}
          pending={install.isPending}
          onConfirm={execute}
          onCancel={() => setConfirming(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
