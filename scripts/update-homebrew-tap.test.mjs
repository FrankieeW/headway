import assert from "node:assert/strict";
import { test } from "node:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { planTapUpdate, run } from "./update-homebrew-tap.mjs";

const sha = "1".repeat(40), arm = "a".repeat(64), intel = "b".repeat(64);
const formula = `class Headway < Formula\n  url "https://github.com/FrankieeW/headway.git",\n      tag: "v0.2.1",\n      revision: "${"0".repeat(40)}"\nend\n`;
const cask = `cask "headway" do\n  version "0.2.1"\n  sha256 arm: "${"0".repeat(64)}",\n         intel: "${"0".repeat(64)}"\n  url "https://github.com/FrankieeW/headway/releases/download/v#{version}/Headway-#{version}-#{arch}.dmg"\nend\n`;
function release(tag = "v0.3.0") {
  return { tag_name: tag, draft: false, prerelease: false, assets: ["arm64", "x64"].map((arch, index) => ({ name: `Headway-${tag.slice(1)}-${arch}.dmg`, size: 123, digest: `sha256:${[arm, intel][index]}`, browser_download_url: `https://github.com/FrankieeW/headway/releases/download/${tag}/Headway-${tag.slice(1)}-${arch}.dmg` })) };
}
const sums = `${arm}  Headway-0.3.0-arm64.dmg\n${intel}  Headway-0.3.0-x64.dmg\n`;

test("updates only the release fields when both architecture digests agree", () => {
  const result = planTapUpdate(release(), sums, sha, formula, cask);
  assert.equal(result.changed, true);
  assert.ok(result.formula.includes(`revision: "${sha}"`));
  assert.ok(result.formula.includes('tag:      "v0.3.0"'));
  assert.ok(result.cask.includes(`intel: "${intel}"`));
  assert.ok(result.cask.includes(`arm: "${arm}"`));
});
test("does not downgrade or open duplicate updates", () => {
  assert.equal(planTapUpdate(release("v0.2.1"), "", sha, formula, cask).changed, false);
  assert.equal(planTapUpdate(release("v0.1.0"), "", sha, formula, cask).changed, false);
});
test("does not publish draft or prerelease artifacts", () => {
  for (const flag of ["draft", "prerelease"]) assert.equal(planTapUpdate({ ...release(), [flag]: true }, sums, sha, formula, cask).changed, false);
});
test("rejects missing, tampered or foreign architecture artifacts", () => {
  assert.throws(() => planTapUpdate(release(), `${arm}  Headway-0.3.0-arm64.dmg`, sha, formula, cask));
  const bad = release(); bad.assets[0].digest = `sha256:${intel}`;
  assert.throws(() => planTapUpdate(bad, sums, sha, formula, cask));
  const foreign = release(); foreign.assets[0].browser_download_url = "https://example.org/download.dmg";
  assert.throws(() => planTapUpdate(foreign, sums, sha, formula, cask));
});
test("requires a real commit and aligned source recipes", () => {
  assert.throws(() => planTapUpdate(release(), sums, "main", formula, cask));
  assert.throws(() => planTapUpdate(release(), sums, sha, formula, cask.replace('version "0.2.1"', 'version "0.2.0"')));
});

function publicationFixture(t, extraFile = false, existingPr = false, shallow = false) {
  const temporary = mkdtempSync(join(tmpdir(), "headway-publication-test-"));
  t.after(() => rmSync(temporary, { recursive: true, force: true }));
  let root = join(temporary, "tap");
  const remote = join(temporary, "remote.git");
  const invoke = (program, args, cwd) => execFileSync(program, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trimEnd();
  invoke("git", ["init", "--bare", "--initial-branch=main", remote]);
  mkdirSync(root);
  const git = args => invoke("git", args, root);
  git(["init", "--initial-branch=main"]);
  git(["config", "user.name", "Publication Fixture"]);
  git(["config", "user.email", "fixture@example.invalid"]);
  for (const [file, text] of [["Formula/headway.rb", formula], ["Casks/headway.rb", cask]]) {
    mkdirSync(join(root, file.split("/")[0]), { recursive: true });
    writeFileSync(join(root, file), text);
  }
  git(["add", "."]); git(["commit", "-m", "fixture baseline"]);
  git(["remote", "add", "origin", remote]); git(["push", "origin", "main"]);
  const planned = planTapUpdate(release(), sums, sha, formula, cask);
  git(["checkout", "-b", "automation/headway-v0.3.0"]);
  writeFileSync(join(root, "Formula/headway.rb"), planned.formula);
  writeFileSync(join(root, "Casks/headway.rb"), planned.cask);
  if (extraFile) writeFileSync(join(root, "unrelated.rb"), "unrelated change\n");
  git(["add", "."]); git(["commit", "-m", "fixture automation"]);
  git(["push", "origin", "automation/headway-v0.3.0"]);
  git(["checkout", "main"]);
  if (shallow) {
    // Main advances after the automation branch; a depth-one checkout loses their ancestor.
    writeFileSync(join(root, "README.md"), "new main content\n");
    git(["add", "README.md"]); git(["commit", "-m", "advance main"]);
    git(["push", "origin", "main"]);
    const checkout = join(temporary, "shallow");
    invoke("git", ["clone", "--depth=1", "--branch=main", `file://${remote}`, checkout]);
    root = checkout;
    assert.equal(git(["rev-parse", "--is-shallow-repository"]), "true");
  }
  const metadata = join(temporary, "release.json"), manifest = join(temporary, "SHA256SUMS.txt");
  writeFileSync(metadata, JSON.stringify(release())); writeFileSync(manifest, sums);
  const calls = [];
  const execute = (program, args, cwd) => {
    if (program === "git" && args.join(" ") === "remote get-url origin") return "https://github.com/FrankieeW/homebrew-tap.git";
    if (program !== "gh") return invoke(program, args, cwd);
    calls.push(args);
    if (args[0] === "pr" && args[1] === "list") return JSON.stringify(existingPr ? [{ url: "https://github.com/FrankieeW/homebrew-tap/pull/1" }] : []);
    if (args[0] === "pr" && args[1] === "create") {
      assert.match(readFileSync(args[args.indexOf("--body-file") + 1], "utf8"), /Source commit:/);
      return "https://github.com/FrankieeW/homebrew-tap/pull/2";
    }
    throw new Error(`Unexpected GitHub call ${args}`);
  };
  return { invoke: () => run(["--tap-root", root, "--release-json", metadata, "--checksums", manifest, "--commit", sha, "--apply", "--create-pr"], execute), calls, git };
}

test("publication refuses an existing branch with unrelated changes before opening a PR", t => {
  const fixture = publicationFixture(t, true);
  assert.throws(fixture.invoke, /outside the two Headway recipes/);
  assert.equal(fixture.calls.some(args => args[1] === "create"), false);
  assert.equal(fixture.git(["status", "--porcelain"]), "");
});
test("publication validates and reuses a matching two-file branch", t => {
  const fixture = publicationFixture(t);
  assert.equal(fixture.invoke().pr_url, "https://github.com/FrankieeW/homebrew-tap/pull/2");
  assert.equal(fixture.calls.filter(args => args[1] === "create").length, 1);
});
test("publication validates an existing PR and avoids a duplicate", t => {
  const fixture = publicationFixture(t, false, true);
  assert.equal(fixture.invoke().reason, "pr_exists");
  assert.equal(fixture.calls.some(args => args[1] === "create"), false);
});
test("publication also rejects unrelated changes behind an existing PR", t => {
  const fixture = publicationFixture(t, true, true);
  assert.throws(fixture.invoke, /outside the two Headway recipes/);
});
test("publication recovers a valid automation branch from a shallow checkout after main advances", t => {
  const fixture = publicationFixture(t, false, false, true);
  assert.equal(fixture.invoke().pr_url, "https://github.com/FrankieeW/homebrew-tap/pull/2");
});
test("publication validates an existing PR from the scheduled shallow checkout", t => {
  const fixture = publicationFixture(t, false, true, true);
  assert.equal(fixture.invoke().reason, "pr_exists");
  assert.equal(fixture.calls.some(args => args[1] === "create"), false);
});
