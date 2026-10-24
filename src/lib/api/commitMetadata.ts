export type CiState = "success" | "failure" | "pending" | "not_run" | "unknown";

export interface CommitMetadata {
  readonly sha: string;
  readonly version?: string | null;
  readonly version_source?: "tag" | "manifest" | null;
  readonly tags: string[];
  readonly is_release?: boolean | null;
  readonly release_url?: string | null;
  readonly ci_state: CiState;
  readonly ci_url?: string | null;
  readonly checks_count: number;
  readonly checked_at: string;
  readonly notes: string[];
}
