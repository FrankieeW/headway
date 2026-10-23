import type { Install, TokenSource, Track } from "./api/types";

/** Minimal shape of the i18next `t` function the helpers need. */
export type TranslateFn = (
  key: string,
  options?: Record<string, unknown>,
) => string;

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const MONTH = 30 * DAY;
const YEAR = 365 * DAY;

/** A git object name: hex only, at least 7 characters. */
const SHA_PATTERN = /^[0-9a-f]{7,40}$/i;

/**
 * Shorten a commit SHA to 7 characters. Anything that is not a SHA
 * (a tag like `v1.4.0`, a branch name) is returned unchanged.
 */
export function shortSha(id: string | null | undefined): string {
  if (!id) return "";
  const value = id.trim();
  return SHA_PATTERN.test(value) ? value.slice(0, 7) : value;
}

function relativeTimeFormatter(lang: string): Intl.RelativeTimeFormat {
  try {
    return new Intl.RelativeTimeFormat(lang, { numeric: "auto" });
  } catch {
    return new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  }
}

/**
 * "3 days ago" style label. `lang` selects the locale ("en", "zh", …);
 * an unknown locale falls back to English.
 */
export function timeAgo(iso: string | null | undefined, lang = "en"): string {
  if (!iso) return "—";
  const timestamp = Date.parse(iso);
  if (Number.isNaN(timestamp)) return "—";

  const formatter = relativeTimeFormatter(lang);
  // negative delta = the moment is in the past
  const delta = timestamp - Date.now();
  const absolute = Math.abs(delta);

  if (absolute < MINUTE) {
    return formatter.format(Math.round(delta / SECOND), "second");
  }
  if (absolute < HOUR) {
    return formatter.format(Math.round(delta / MINUTE), "minute");
  }
  if (absolute < DAY) {
    return formatter.format(Math.round(delta / HOUR), "hour");
  }
  if (absolute < MONTH) {
    return formatter.format(Math.round(delta / DAY), "day");
  }
  if (absolute < YEAR) {
    return formatter.format(Math.round(delta / MONTH), "month");
  }
  return formatter.format(Math.round(delta / YEAR), "year");
}

const BYTE_UNITS = ["KB", "MB", "GB", "TB", "PB"];

/** Human readable file size, e.g. `1.5 MB`. */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "—";
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;

  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }

  const rounded = value >= 100 ? value.toFixed(0) : value.toFixed(1);
  return `${rounded.replace(/\.0$/, "")} ${BYTE_UNITS[unit]}`;
}

/** Human readable duration, e.g. `1m 05s`. */
export function formatDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  if (!Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;

  const seconds = Math.floor(ms / SECOND);
  if (seconds < 60) return `${(ms / SECOND).toFixed(1)}s`;

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  }

  const hours = Math.floor(minutes / 60);
  return `${hours}h ${String(minutes % 60).padStart(2, "0")}m`;
}

/** The first non-empty line of a text — used for commit and release titles. */
export function firstLine(text: string | null | undefined): string {
  if (!text) return "";
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed) return trimmed;
  }
  return "";
}

/**
 * Short description of what the app follows:
 * "Every commit · main", "Releases", "Tags".
 */
export function trackLabel(track: Track, t: TranslateFn): string {
  switch (track.mode) {
    case "commit": {
      const branch = track.branch?.trim();
      return branch
        ? t("track.commitWithBranch", { branch })
        : t("track.commit");
    }
    case "release":
      return t("track.release");
    case "tag":
      return t("track.tag");
    default:
      return "";
  }
}

/** How a new version gets onto the machine. */
export function installLabel(install: Install, t: TranslateFn): string {
  return install.method === "asset"
    ? t("install.asset")
    : t("install.source");
}

/** Where the GitHub token comes from. */
export function tokenSourceLabel(
  source: TokenSource | undefined,
  t: TranslateFn,
): string {
  switch (source) {
    case "keychain":
      return t("token.source.keychain");
    case "headway_env":
      return "HEADWAY_GITHUB_TOKEN";
    case "github_env":
      return "GITHUB_TOKEN";
    case "gh_env":
      return "GH_TOKEN";
    case "gh_cli":
      return t("token.source.ghCli");
    default:
      return t("token.source.none");
  }
}

/** `owner/name` → `name`, used for default ids and binary names. */
export function repoName(repo: string | null | undefined): string {
  if (!repo) return "";
  const parts = repo.replace(/^https?:\/\/[^/]+\//, "").split("/");
  return parts[parts.length - 1] ?? "";
}

/** Turn a display name into a valid id: lowercase, dashes, no leading dash. */
export function slugify(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^[^a-z0-9]+/, "")
    .replace(/-+$/, "");
}
