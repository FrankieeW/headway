import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { test } from "node:test";

const source = resolve(import.meta.dirname, "check-release-version.mjs");
const members = ["headway-core", "headway-cli", "headway-desktop"];

function fixture(run, options = {}) {
  const base = realpathSync(tmpdir());
  const root = mkdtempSync(join(base, "headway-version-guard-"));
  try {
    mkdirSync(join(root, "scripts"));
    mkdirSync(join(root, "src-tauri"));
    copyFileSync(source, join(root, "scripts/check-release-version.mjs"));
    writeFileSync(join(root, "Cargo.toml"), '[workspace.package]\nversion = "0.5.0"\n');
    writeFileSync(join(root, "package.json"), JSON.stringify({ version: "0.5.0", type: "module" }));
    writeFileSync(join(root, "src-tauri/tauri.conf.json"), JSON.stringify({ version: "0.5.0" }));
    writeFileSync(join(root, "Cargo.lock"), members
      .filter((name) => name !== options.missing)
      .map((name) => `[[package]]\nname = "${name}"\nversion = "${options.stale === name ? "0.4.2" : "0.5.0"}"\n`)
      .join("\n"));
    run((...args) => spawnSync(process.execPath, [join(root, "scripts/check-release-version.mjs"), ...args], {
      cwd: root, encoding: "utf8", env: { ...process.env, RELEASE_DEBUG: "" }, timeout: 10_000,
    }), root);
  } finally {
    // Only remove this test's freshly allocated absolute temp directory.
    const target = realpathSync(root);
    assert.equal(dirname(target), base);
    assert.ok(target.startsWith(`${base}${sep}headway-version-guard-`));
    rmSync(target, { recursive: true });
  }
}

test("matching versions and tag print exactly one stdout version line", () => {
  fixture((invoke) => {
    const result = invoke("--tag", "v0.5.0");
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "0.5.0\n");
  });
});

test("a stale lockfile member fails before any cargo command can repair it", () => {
  fixture((invoke) => {
    const result = invoke("--tag", "v0.5.0");
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /Cargo\.lock headway-cli is stale/);
  }, { stale: "headway-cli" });
});

test("a missing lockfile member fails rather than silently checking the remaining two", () => {
  fixture((invoke) => {
    const result = invoke();
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Cargo\.lock headway-desktop is missing/);
  }, { missing: "headway-desktop" });
});

test("a mismatched tag and mismatched JSON version are both reported on stderr", () => {
  fixture((invoke, root) => {
    writeFileSync(join(root, "package.json"), JSON.stringify({ version: "0.4.2" }));
    const result = invoke("--tag", "v0.6.0");
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /package\.json has version/);
    assert.match(result.stderr, /tag `v0\.6\.0` does not match/);
  });
});

test("JSON output remains one parseable document", () => {
  fixture((invoke) => {
    const result = invoke("--json");
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout).warnings, []);
    assert.equal(JSON.parse(result.stdout).version, "0.5.0");
  });
});
