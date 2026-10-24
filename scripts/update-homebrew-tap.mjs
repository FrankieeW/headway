#!/usr/bin/env node
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const SOURCE = "FrankieeW/headway";
const TAP = "FrankieeW/homebrew-tap";
const FILES = ["Formula/headway.rb", "Casks/headway.rb"];

class PublicationError extends Error {
  constructor(message) { super(message); this.name = "PublicationError"; }
}

function version(tag) {
  const match = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(tag);
  if (!match) throw new PublicationError(`Not a stable version: ${tag}`);
  return match.slice(1).map(Number);
}

function compare(a, b) {
  const left = version(a), right = version(b);
  for (let index = 0; index < 3; index++) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function currentVersion(formula, cask) {
  const left = /tag:\s*"v([^"\n]+)"/.exec(formula)?.[1];
  const right = /\bversion\s+"([^"\n]+)"/.exec(cask)?.[1];
  if (!left || !right || left !== right) throw new PublicationError("Formula/cask versions disagree");
  version(right);
  return right;
}

export function planTapUpdate(release, checksums, commit, formula, cask) {
  if (release?.draft || release?.prerelease) return { changed: false, reason: "not_stable" };
  if (typeof release?.tag_name !== "string" || !Array.isArray(release.assets)) throw new PublicationError("Invalid release metadata");
  const target = version(release.tag_name).join(".");
  const caskVersion = currentVersion(formula, cask);
  if (compare(target, caskVersion) <= 0) return { changed: false, reason: "up_to_date", version: target };
  if (!/^[a-f0-9]{40}$/.test(commit)) throw new PublicationError("Invalid source commit");
  if (!formula.includes(`https://github.com/${SOURCE}.git`) || !cask.includes(`https://github.com/${SOURCE}/releases/download/`)) throw new PublicationError("Unexpected recipe repository");
  const sums = new Map();
  for (const line of checksums.trim().split(/\r?\n/)) {
    const match = /^([a-f0-9]{64})\s+\*?(.+)$/.exec(line);
    if (!match || sums.has(match[2])) throw new PublicationError("Invalid or duplicate checksum entry");
    sums.set(match[2], match[1]);
  }
  const hashes = ["arm64", "x64"].map(arch => {
    const name = `Headway-${target}-${arch}.dmg`;
    const sha = sums.get(name);
    const asset = release.assets.find(asset => asset.name === name);
    if (!sha || !asset || !(asset.size > 0) || asset.browser_download_url !== `https://github.com/${SOURCE}/releases/download/v${target}/${name}`) throw new PublicationError(`Missing expected asset: ${name}`);
    if (asset.digest && asset.digest !== `sha256:${sha}`) throw new PublicationError(`Release digest disagrees: ${name}`);
    return sha;
  });
  const nextFormula = formula.replace(/tag:\s*"v[^"\n]+"/, `tag:      "v${target}"`)
    .replace(/revision:\s*"[a-f0-9]{40}"/, `revision: "${commit}"`);
  const nextCask = cask.replace(/\bversion\s+"[^"\n]+"/, `version "${target}"`)
    .replace(/(sha256 arm:\s+")[a-f0-9]{64}("\s*,\s*intel:\s*")[a-f0-9]{64}(")/,
      (_, a, b, c) => `${a}${hashes[0]}${b}${hashes[1]}${c}`);
  if (!nextFormula.includes(commit) || !nextCask.includes(hashes[0]) || !nextCask.includes(hashes[1])) throw new PublicationError("Unexpected recipe layout");
  return { changed: true, version: target, previous: caskVersion, commit, formula: nextFormula, cask: nextCask, hashes };
}

function command(program, args, cwd) {
  const env = { ...process.env };
  delete env.HEADWAY_GITHUB_TOKEN;
  return execFileSync(program, args, { cwd, env, encoding: "utf8", maxBuffer: 2 * 1024 * 1024 }).trimEnd();
}

function sourceCommit(tag, github) {
  let object = JSON.parse(github(["api", `repos/${SOURCE}/git/ref/tags/${tag}`])).object;
  for (let hops = 0; hops < 5; hops++) {
    if (!object || !/^[a-f0-9]{40}$/.test(object.sha)) throw new PublicationError("Invalid tag object");
    if (object.type === "commit") return object.sha;
    if (object.type !== "tag") throw new PublicationError("Tag does not resolve to a commit");
    object = JSON.parse(github(["api", `repos/${SOURCE}/git/tags/${object.sha}`])).object;
  }
  throw new PublicationError("Too many annotated tag indirections");
}

export function run(args, execute = command) {
  const github = args => execute("gh", args);
  const options = { root: ".", apply: false, pr: false, release: null, checksums: null, commit: null };
  for (let index = 0; index < args.length; index++) {
    switch (args[index]) {
      case "--apply": options.apply = true; break;
      case "--create-pr": options.pr = true; break;
      case "--tap-root": case "--release-json": case "--checksums": case "--commit": {
        const key = { "--tap-root": "root", "--release-json": "release", "--checksums": "checksums", "--commit": "commit" }[args[index]];
        const value = args[++index];
        if (!value || value.startsWith("--")) throw new PublicationError(`Missing value for ${args[index - 1]}`);
        options[key] = value;
        break;
      }
      default: throw new PublicationError(`Unknown argument: ${args[index]}`);
    }
  }
  if (!options.root || (options.pr && !options.apply)) throw new PublicationError("--create-pr requires --apply");
  const root = resolve(options.root);
  const release = JSON.parse(options.release ? readFileSync(options.release, "utf8") : github(["api", `repos/${SOURCE}/releases/latest`]));
  const formula = readFileSync(join(root, FILES[0]), "utf8"), cask = readFileSync(join(root, FILES[1]), "utf8");
  const current = currentVersion(formula, cask);
  if (release.draft || release.prerelease) return { changed: false, reason: "not_stable" };
  if (current && typeof release.tag_name === "string" && compare(release.tag_name, current) <= 0) return { changed: false, reason: "up_to_date", version: current };
  const scratch = mkdtempSync(join(tmpdir(), "headway-tap-"));
  try {
    if (!options.checksums) github(["release", "download", release.tag_name, "--repo", SOURCE, "--pattern", "SHA256SUMS.txt", "--dir", scratch]);
    const sumFile = options.checksums || join(scratch, "SHA256SUMS.txt");
    const checksumBytes = readFileSync(sumFile);
    const sumAsset = release.assets?.find(asset => asset.name === "SHA256SUMS.txt");
    if (!options.checksums && sumAsset?.digest && sumAsset.digest !== `sha256:${createHash("sha256").update(checksumBytes).digest("hex")}`) throw new PublicationError("Checksum manifest digest disagrees");
    const plan = planTapUpdate(release, checksumBytes.toString("utf8"), options.commit || sourceCommit(release.tag_name, github), formula, cask);
    if (!plan.changed || !options.apply) return plan;
    const branch = `automation/headway-v${plan.version}`;
    if (options.pr) {
      const git = args => execute("git", args, root);
      const remote = git(["remote", "get-url", "origin"]);
      if (!/^https:\/\/github\.com\/FrankieeW\/homebrew-tap(?:\.git)?$/i.test(remote)) throw new PublicationError("PR publication requires the official tap checkout");
      if (git(["status", "--porcelain"])) throw new PublicationError("Tap checkout must be clean");
      const existing = JSON.parse(github(["pr", "list", "--repo", TAP, "--head", branch, "--state", "open", "--json", "url"]));
      const remoteBranch = git(["ls-remote", "--heads", "origin", `refs/heads/${branch}`]);
      if (remoteBranch) {
        const head = remoteBranch.split(/\s+/)[0];
        if (!/^[a-f0-9]{40}$/.test(head)) throw new PublicationError("Invalid automation branch commit");
        const depth = git(["rev-parse", "--is-shallow-repository"]);
        git(["fetch", ...(depth === "true" ? ["--unshallow"] : []), "--no-tags", "origin", "main"]);
        const base = git(["rev-parse", "FETCH_HEAD"]);
        git(["fetch", "--no-tags", "origin", head]);
        const changed = git(["diff", "--name-only", "-z", `${base}...${head}`]).split("\0").filter(Boolean);
        if (changed.length !== FILES.length || changed.some(file => !FILES.includes(file))) throw new PublicationError("Existing automation branch changes files outside the two Headway recipes");
        for (const [index, file] of FILES.entries()) {
          const body = git(["show", `${head}:${file}`]);
          if (body !== [plan.formula, plan.cask][index].trimEnd()) throw new PublicationError("Existing automation branch has different contents");
        }
        if (existing.length) return { changed: false, reason: "pr_exists", pr_url: existing[0].url };
      } else {
        if (existing.length) throw new PublicationError("Open automation PR has no matching remote branch");
        git(["checkout", "-b", branch]);
        writeFileSync(join(root, FILES[0]), plan.formula);
        writeFileSync(join(root, FILES[1]), plan.cask);
        git(["add", "--", ...FILES]);
        git(["commit", "-m", `chore: bump headway to v${plan.version}`]);
        git(["push", "--set-upstream", "origin", branch]);
      }
      const body = join(scratch, "body.md");
      writeFileSync(body, `Update Headway ${plan.previous} to ${plan.version}.\n\nSource commit: ${plan.commit}. DMG SHA-256 values match the published manifest and asset digests.\n\n${release.html_url}\n`);
      const url = github(["pr", "create", "--repo", TAP, "--head", branch, "--base", "main", "--title", `chore: bump headway to v${plan.version}`, "--body-file", body]);
      return { changed: true, version: plan.version, pr_url: url };
    }
    writeFileSync(join(root, FILES[0]), plan.formula);
    writeFileSync(join(root, FILES[1]), plan.cask);
    return { changed: true, version: plan.version, commit: plan.commit };
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { process.stdout.write(`${JSON.stringify(run(process.argv.slice(2)))}\n`); }
  catch (error) { if (!(error instanceof Error)) throw error; process.stderr.write(`${error.message}\n`); process.exitCode = 1; }
}
