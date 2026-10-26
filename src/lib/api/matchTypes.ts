import type { CommandError } from "./types";

export type MatchSide = "github" | "brew";
export type MatchConfidence = "high" | "medium" | "low";
export type MatchEvidenceKind =
  | "source_url"
  | "head_url"
  | "download_url"
  | "homepage"
  | "homepage_match"
  | "name"
  | "search";

export interface MatchEvidence {
  readonly kind: MatchEvidenceKind;
  readonly value: string;
}
export interface RepositoryMatch {
  readonly repo: string;
  readonly html_url: string;
  readonly confidence: MatchConfidence;
  readonly evidence: MatchEvidence[];
  readonly description: string | null;
  readonly stars: number | null;
  readonly tracked_by: string[];
}
export interface PackageMatch {
  readonly package: string;
  readonly cask: boolean;
  readonly name: string;
  readonly description: string | null;
  readonly homepage: string | null;
  readonly installed: string | null;
  readonly repo: string;
  readonly confidence: MatchConfidence;
  readonly evidence: MatchEvidence[];
  readonly tracked_by: string[];
}
export interface MatchError {
  readonly source: string;
  readonly error: CommandError;
}
export interface SourceMatches {
  readonly query: string;
  readonly from: MatchSide;
  readonly repositories: RepositoryMatch[];
  readonly packages: PackageMatch[];
  readonly notes: string[];
  readonly errors: MatchError[];
}
