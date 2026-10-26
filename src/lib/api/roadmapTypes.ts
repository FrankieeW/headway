import type { RepositoryMatch } from "./matchTypes";
import type {
  AppKind,
  BuildInfo,
  CommandError,
  InstalledRef,
  TrackedApp,
} from "./types";

export interface UpdatePolicy {
  readonly mode: "notify" | "next" | "latest";
  readonly max_steps: number;
  readonly pause_on_failure: boolean;
}
export const DEFAULT_UPDATE_POLICY: UpdatePolicy = {
  mode: "latest",
  max_steps: 1,
  pause_on_failure: true,
};
export interface BuildCandidate {
  readonly id: string;
  readonly source: "readme" | "manifest" | "preset";
  readonly file: string;
  readonly line?: number | null;
  readonly build: string;
  readonly workdir?: string | null;
  readonly artifact?: string | null;
  readonly kind: AppKind;
  readonly reason: string;
  readonly recommended: boolean;
}
export interface BackupEntry {
  readonly id: string;
  readonly app_id: string;
  readonly created_at: string;
  readonly installed?: InstalledRef | null;
  readonly artifact_path: string;
}
export interface RestoreOutcome {
  readonly app_id: string;
  readonly backup_id: string;
  readonly installed?: InstalledRef | null;
  readonly restored_path: string;
}
export interface InventoryCandidate {
  readonly repositories?: RepositoryMatch[];
  readonly key: string;
  readonly app: TrackedApp;
  readonly installed_version?: string | null;
  readonly already_tracked: boolean;
  readonly conflicts: string[];
}
export interface InventoryReport {
  readonly candidates: InventoryCandidate[];
  readonly errors: { source: string; error: CommandError }[];
}
export interface AppRecipe {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly app: TrackedApp;
}
export interface SelfUpdateStatus {
  readonly running: BuildInfo;
  readonly installed?: BuildInfo | null;
  readonly pending_restart: boolean;
  readonly installed_path?: string | null;
  readonly app_path?: string | null;
  readonly restart_requested: boolean;
  readonly confirmed: boolean;
  readonly restart_error?: string | null;
}
