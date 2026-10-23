import { describe, expect, it } from "vitest";

import {
  firstLine,
  formatBytes,
  formatDuration,
  installLabel,
  repoName,
  shortSha,
  slugify,
  timeAgo,
  trackLabel,
  tokenSourceLabel,
} from "./format";
import type { Install, Track } from "./api/types";

/** The i18n function is only used for its keys here. */
const t = (key: string, options?: Record<string, unknown>): string =>
  options ? `${key}:${JSON.stringify(options)}` : key;

describe("shortSha", () => {
  it("shortens a sha to 7 characters", () => {
    expect(shortSha("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0")).toBe("a1b2c3d");
  });

  it("keeps short hex untouched when it is already 7 characters", () => {
    expect(shortSha("a1b2c3d")).toBe("a1b2c3d");
  });

  it("leaves tags and names alone", () => {
    expect(shortSha("v1.4.0")).toBe("v1.4.0");
    expect(shortSha("release-2024")).toBe("release-2024");
    expect(shortSha("main")).toBe("main");
  });

  it("handles empty input", () => {
    expect(shortSha("")).toBe("");
    expect(shortSha(null)).toBe("");
    expect(shortSha(undefined)).toBe("");
  });
});

describe("timeAgo", () => {
  it("describes the past", () => {
    const twoHoursAgo = new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString();
    expect(timeAgo(twoHoursAgo, "en")).toContain("2 hours");
  });

  it("uses the given locale", () => {
    const twoHoursAgo = new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString();
    expect(timeAgo(twoHoursAgo, "zh")).toContain("2");
  });

  it("returns a dash for unusable input", () => {
    expect(timeAgo("", "en")).toBe("—");
    expect(timeAgo("not a date", "en")).toBe("—");
    expect(timeAgo(null, "en")).toBe("—");
  });

  it("does not throw on an unknown locale", () => {
    const past = new Date(Date.now() - 60 * 1000).toISOString();
    expect(() => timeAgo(past, "not-a-locale")).not.toThrow();
  });
});

describe("formatBytes", () => {
  it("formats small sizes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1 KB");
  });

  it("formats larger sizes", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024)).toBe("1 MB");
    expect(formatBytes(1024 * 1024 * 1024 * 2.5)).toBe("2.5 GB");
  });

  it("rejects nonsense", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});

describe("formatDuration", () => {
  it("formats sub second durations", () => {
    expect(formatDuration(120)).toBe("120ms");
  });

  it("formats seconds", () => {
    expect(formatDuration(1500)).toBe("1.5s");
  });

  it("formats minutes", () => {
    expect(formatDuration(65_000)).toBe("1m 05s");
  });

  it("formats hours", () => {
    expect(formatDuration(3_720_000)).toBe("1h 02m");
  });

  it("rejects nonsense", () => {
    expect(formatDuration(-5)).toBe("—");
    expect(formatDuration(null)).toBe("—");
  });
});

describe("firstLine", () => {
  it("returns the first non-empty line", () => {
    expect(firstLine("\n\nHello\nWorld")).toBe("Hello");
  });

  it("trims the line", () => {
    expect(firstLine("   spaced   \nnext")).toBe("spaced");
  });

  it("returns an empty string for empty input", () => {
    expect(firstLine("")).toBe("");
    expect(firstLine(null)).toBe("");
    expect(firstLine("\n\n")).toBe("");
  });
});

describe("trackLabel", () => {
  it("names the commit mode with its branch", () => {
    const track: Track = { mode: "commit", branch: "main" };
    expect(trackLabel(track, t)).toBe(
      "track.commitWithBranch:{\"branch\":\"main\"}",
    );
  });

  it("names the commit mode without a branch", () => {
    const track: Track = { mode: "commit", branch: null };
    expect(trackLabel(track, t)).toBe("track.commit");
  });

  it("names releases and tags", () => {
    const release: Track = { mode: "release", prerelease: false };
    const tag: Track = { mode: "tag", prerelease: true };
    expect(trackLabel(release, t)).toBe("track.release");
    expect(trackLabel(tag, t)).toBe("track.tag");
  });
});

describe("installLabel", () => {
  it("names the install method", () => {
    const source: Install = { method: "source", build: "make" };
    const asset: Install = { method: "asset" };
    expect(installLabel(source, t)).toBe("install.source");
    expect(installLabel(asset, t)).toBe("install.asset");
  });
});

describe("tokenSourceLabel", () => {
  it("names the source", () => {
    expect(tokenSourceLabel("keychain", t)).toBe("token.source.keychain");
    expect(tokenSourceLabel("gh_cli", t)).toBe("token.source.ghCli");
    expect(tokenSourceLabel("none", t)).toBe("token.source.none");
    expect(tokenSourceLabel(undefined, t)).toBe("token.source.none");
  });

  it("keeps the environment variable names untranslated", () => {
    expect(tokenSourceLabel("headway_env", t)).toBe("HEADWAY_GITHUB_TOKEN");
    expect(tokenSourceLabel("github_env", t)).toBe("GITHUB_TOKEN");
    expect(tokenSourceLabel("gh_env", t)).toBe("GH_TOKEN");
  });
});

describe("repoName", () => {
  it("takes the last part of owner/name", () => {
    expect(repoName("owner/name")).toBe("name");
    expect(repoName("https://github.com/owner/name")).toBe("name");
  });

  it("handles empty input", () => {
    expect(repoName("")).toBe("");
    expect(repoName(null)).toBe("");
  });
});

describe("slugify", () => {
  it("lowercases and dashes", () => {
    expect(slugify("My Tool")).toBe("my-tool");
  });

  it("removes leading characters that are not allowed", () => {
    expect(slugify("  --Hello")).toBe("hello");
  });

  it("keeps dots, dashes and underscores", () => {
    expect(slugify("my.tool_v2")).toBe("my.tool_v2");
  });
});
