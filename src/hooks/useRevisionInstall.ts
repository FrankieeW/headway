import { useMutation, useQueryClient } from "@tanstack/react-query";
import { appsApi, type RevisionRequest, type VariantRequest } from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";
import { clearRun, markRunStarted } from "@/lib/updateRuns";

export type RevisionJob =
  | { readonly id: string; readonly revision: RevisionRequest }
  | { readonly parentId: string; readonly variant: VariantRequest };
export function useRevisionInstall() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (job: RevisionJob) =>
      "variant" in job
        ? appsApi.installVariant(job.parentId, job.variant)
        : appsApi.installRevision(job.id, job.revision),
    onMutate: (job) =>
      markRunStarted("variant" in job ? job.variant.id : job.id),
    onSuccess: async (_result, job) => {
      const id = "variant" in job ? job.variant.id : job.id;
      await Promise.all([
        client.invalidateQueries({ queryKey: queryKeys.apps }),
        client.invalidateQueries({ queryKey: queryKeys.app(id) }),
        client.invalidateQueries({ queryKey: queryKeys.history(id) }),
      ]);
    },
    onError: (_error, job) =>
      clearRun("variant" in job ? job.variant.id : job.id),
  });
}
