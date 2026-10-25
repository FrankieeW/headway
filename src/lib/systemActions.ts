import { toast } from "sonner";

import { errorMessage, systemApi } from "@/lib/api";

/**
 * Open a URL in the browser. The backend rejects unsupported schemes and
 * empty URLs; a rejected promise used to become a silent no-op, so every
 * failure becomes a toast instead.
 */
export function openUrl(url: string): void {
  systemApi.openUrl(url).catch((error: unknown) => {
    toast.error(errorMessage(error));
  });
}

/**
 * Reveal a path in Finder. The backend rejects paths that do not exist;
 * surface that instead of dropping the rejection.
 */
export function revealPath(path: string): void {
  systemApi.revealPath(path).catch((error: unknown) => {
    toast.error(errorMessage(error));
  });
}
