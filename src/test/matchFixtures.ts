import type {
  InventoryCandidate,
  RepoInspection,
  SourceLookup,
  SourceMatches,
} from "@/lib/api";

export const repositoryMatches: SourceMatches = {
  query: "brew cask editor",
  from: "brew",
  repositories: [
    {
      repo: "acme/editor",
      html_url: "https://github.com/acme/editor",
      confidence: "high",
      evidence: [
        {
          kind: "download_url",
          value:
            "https://github.com/acme/editor/releases/download/v2/editor.dmg",
        },
      ],
      description: null,
      stars: null,
      tracked_by: [],
    },
  ],
  packages: [],
  notes: [],
  errors: [],
};

export const packageMatches: SourceMatches = {
  query: "acme/editor",
  from: "github",
  repositories: [],
  packages: [
    {
      package: "vendor/apps/editor",
      cask: true,
      name: "Editor",
      description: "A text editor",
      homepage: "https://editor.example",
      installed: "2.0",
      repo: "acme/editor",
      confidence: "high",
      evidence: [
        {
          kind: "download_url",
          value:
            "https://github.com/acme/editor/releases/download/v2/editor.dmg",
        },
      ],
      tracked_by: [],
    },
  ],
  notes: [],
  errors: [],
};

export const matchInspection: RepoInspection = {
  repo: "acme/editor",
  description: "Repository inspection",
  default_branch: "main",
  latest_release: null,
  notes: [],
  private: false,
  archived: false,
  html_url: "https://github.com/acme/editor",
  binaries: [],
  suggestion: {
    id: "editor",
    name: "Editor",
    source: "github",
    repo: "acme/editor",
    kind: "cli",
    auto_update: false,
    track: { mode: "commit" },
    install: { method: "source", build: "make" },
  },
};

export const matchLookup: SourceLookup = {
  source: "brew",
  package: "vendor/apps/editor",
  description: "Package lookup",
  installed: "2.0",
  latest: "2.1",
  notes: [],
  suggestion: {
    id: "editor",
    name: "Editor",
    source: "brew",
    package: "vendor/apps/editor",
    cask: true,
    kind: "app",
    auto_update: false,
  },
};

export const matchInventory: InventoryCandidate = {
  key: "cask:editor",
  app: matchLookup.suggestion,
  installed_version: "2.0",
  already_tracked: false,
  conflicts: [],
  repositories: repositoryMatches.repositories,
};
