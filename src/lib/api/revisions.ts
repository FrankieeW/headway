import type {
  AssetInstall,
  Candidate,
  SourceInstall,
  Install,
  Track,
  InstalledRef,
} from "./types";

export interface RelatedRepository {
  readonly full_name: string;
  readonly default_branch: string;
}
export interface PullHead {
  readonly ref: string;
  readonly sha: string;
  readonly repo: { readonly full_name: string } | null;
}
export interface PullRequest {
  readonly number: number;
  readonly title: string;
  readonly html_url: string;
  readonly state: string;
  readonly draft: boolean;
  readonly head: PullHead;
  readonly base: PullHead;
}
export interface PullRequestPage {
  readonly repo: string;
  readonly page: number;
  readonly per_page: number;
  readonly pull_requests: readonly PullRequest[];
  readonly has_more: boolean;
}
export interface InstallationDetection {
  readonly presence: "unknown" | "present" | "missing" | "invalid";
  readonly path?: string | null;
  readonly baseline_matches: boolean;
  readonly recorded?: {
    readonly repo: string;
    readonly installed: InstalledRef;
  } | null;
}

export interface SourceStorage {
  readonly keep: boolean;
  readonly root?: string | null;
}
export interface SourceStorageStatus {
  readonly path: string;
  readonly keep: boolean;
  readonly exists: boolean;
  readonly custom_root: boolean;
}
export interface RepositoryOption {
  readonly repo: string;
  readonly label: string;
  readonly git_url?: string | null;
  readonly branch?: string | null;
  readonly settings?: {
    readonly track: Track;
    readonly install: Install;
    readonly source_build?: SourceInstall | null;
    readonly release_asset?: AssetInstall | null;
  } | null;
}
export interface VariantInfo {
  readonly parent_id: string;
  readonly reference: string;
  readonly release_tag?: string | null;
  readonly branch?: string | null;
}
export interface CommitPage {
  readonly repo: string;
  readonly branch: string;
  readonly anchor: string;
  readonly page: number;
  readonly per_page: number;
  readonly commits: Candidate[];
  readonly has_more: boolean;
  readonly next_page: number | null;
}
export interface RevisionOptions {
  readonly pull_request?: PullRequest | null;
  readonly reference: string;
  readonly commit: string;
  readonly candidate: Candidate;
  readonly release_tag: string | null;
  readonly release_url: string | null;
  readonly can_download: boolean;
  readonly source_build: SourceInstall | null;
  readonly release_asset: AssetInstall | null;
}
export interface RevisionRequest {
  readonly reference: string;
  readonly method: "configured" | "source" | "asset";
  readonly release_tag?: string;
  readonly source_build?: SourceInstall;
  readonly release_asset?: AssetInstall;
}
export interface VariantRequest {
  readonly id: string;
  readonly name?: string;
  readonly revision: RevisionRequest;
}
export interface LaunchOutcome {
  readonly app_id: string;
  readonly pid: number;
  readonly path: string;
}
