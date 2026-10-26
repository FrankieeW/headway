export type MatchSelection =
  | { readonly source: "github"; readonly repo: string }
  | {
      readonly source: "brew";
      readonly package: string;
      readonly cask: boolean;
    };
