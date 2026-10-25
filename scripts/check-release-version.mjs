#!/usr/bin/env node
//! Checks that the release version is consistent in every place it is published from.
//!
//! Root versions are bumped by `scripts/bump-version.sh`; this guard makes sure a tag can
//! never produce artifacts that report a different version than the tag (or than the Homebrew
//! cask/DMG name). It runs in the release guard job *before* anything is built, and locally as
//! `pnpm run check:release-version -- --tag v0.4.2`.
//!
//! Checked locations:
//!   - Cargo.toml                  ([workspace.package] version)
//!   - package.json                (version)
//!   - src-tauri/tauri.conf.json   (version)
//!   - Cargo.lock                  (headway-core/headway-cli/headway-desktop package versions)
//!
//! Usage:
//!   node scripts/check-release-version.mjs [--tag vX.Y.Z] [--json]

import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;

function readText(relativePath) {
  return readFileSync(join(root, relativePath), "utf8");
}

function cargoWorkspaceVersion() {
  const match = /\[workspace\.package\][\s\S]*?^version = "([^"]+)"/m.exec(readText("Cargo.toml"));
  if (!match) throw new Error("Cargo.toml has no [workspace.package] version");
  return match[1];
}

function packageJsonVersion() {
  return JSON.parse(readText("package.json")).version;
}

function tauriVersion() {
  return JSON.parse(readText("src-tauri/tauri.conf.json")).version;
}

function lockVersions() {
  const lock = readText("Cargo.lock");
  const found = new Map();
  for (const block of lock.split("[[package]]").slice(1)) {
    const name = /name = "([^"]+)"/.exec(block)?.[1];
    const version = /version = "([^"]+)"/.exec(block)?.[1];
    if (name && version && name.startsWith("headway-")) found.set(name, version);
  }
  return found;
}

function parseArgs(argv) {
  const options = { tag: null, json: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--json") options.json = true;
    else if (arg === "--tag") {
      options.tag = argv[index + 1] ?? null;
      index += 1;
      if (!options.tag) throw new Error("--tag needs a value like v0.4.2");
    } else if (arg.startsWith("--tag=")) options.tag = arg.slice("--tag=".length);
    else throw new Error(`unknown argument: ${arg}`);
  }
  return options;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const sources = [
    ["Cargo.toml", cargoWorkspaceVersion()],
    ["package.json", packageJsonVersion()],
    ["src-tauri/tauri.conf.json", tauriVersion()],
  ];
  // Releases use --locked: the committed lockfile must already agree, not merely be
  // repairable by a later cargo invocation. Missing members must fail the guard too.
  const members = ["headway-core", "headway-cli", "headway-desktop"];
  const locked = lockVersions();
  const lockSources = members.map((name) => [`Cargo.lock ${name}`, locked.get(name)]);

  const errors = [];
  const warnings = [];
  const version = sources[0][1];
  if (!SEMVER.test(version)) errors.push(`Cargo.toml version \`${version}\` is not a release semver`);
  for (const [name, value] of sources.slice(1)) {
    if (value !== version) errors.push(`${name} has version \`${value}\`, expected \`${version}\``);
  }
  for (const [name, value] of lockSources) {
    if (value === undefined) {
      errors.push(`${name} is missing; regenerate and commit Cargo.lock before releasing`);
    } else if (value !== version) {
      errors.push(`${name} is stale (\`${value}\`), expected \`${version}\`; refresh and commit Cargo.lock`);
    }
  }

  let tagVersion = null;
  if (options.tag !== null) {
    const match = /^v?(.+)$/.exec(options.tag.trim());
    tagVersion = match ? match[1] : options.tag;
    if (!SEMVER.test(tagVersion)) {
      errors.push(`tag \`${options.tag}\` is not vX.Y.Z (stable) or vX.Y.Z-suffix (pre-release)`);
    } else if (tagVersion !== version) {
      errors.push(`tag \`${options.tag}\` does not match the checked-in version \`${version}\``);
    }
  }

  if (errors.length > 0) {
    for (const error of errors) console.error(`error: ${error}`);
    console.error(
      "release version mismatch: bump with `./scripts/bump-version.sh` and commit before tagging",
    );
    process.exitCode = 1;
    return;
  }

  for (const warning of warnings) console.error(`warning: ${warning}`);
  if (options.json) {
    console.log(
      JSON.stringify({
        version,
        tag: options.tag,
        sources: Object.fromEntries(sources),
        warnings,
      }),
    );
    return;
  }
  for (const [name, value] of [...sources, ...lockSources]) console.error(`ok: ${name} = ${value}`);
  if (options.tag !== null) console.error(`ok: tag ${options.tag} matches ${version}`);
  // The release workflow reads the version from stdout.
  console.log(version);
}

try {
  main();
} catch (error) {
  if (process.env.RELEASE_DEBUG) console.error(error.stack);
  console.error(`check-release-version could not run: ${error.message}`);
  process.exitCode = 2;
}
