#!/usr/bin/env bash
# Native signing regression, without building or launching the Headway GUI.
# Pass an evidence directory to retain the tiny app, generated configs and logs.
set -euo pipefail
[ "$(uname -s)" = Darwin ] || { echo 'macOS required' >&2; exit 2; }
root="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# Same credential boundary as Headway's process::Cmd: the fixture build below must not
# inherit a GitHub token from the caller's shell.
. "$root/scripts/token-env.sh"
if [ "$#" -gt 0 ]; then
  mkdir -p "$1"
  evidence="$(cd "$1" && pwd)"
else
  evidence="$(mktemp -d)"
  trap 'rm -rf -- "$evidence"' EXIT
fi
fixture="$(mktemp -d "$evidence/native-XXXXXXXX")"
mkdir -p "$fixture/src-tauri/src" "$fixture/dist"
cat > "$fixture/src-tauri/Cargo.toml" <<'TOML'
[package]
name = "packaging-probe"
version = "0.1.0"
edition = "2024"
[workspace]
TOML
cat > "$fixture/src-tauri/src/main.rs" <<'RUST'
fn main() { println!("headway-packaging-runtime-ok"); }
RUST
cat > "$fixture/src-tauri/build.rs" <<'RUST'
fn main() { println!("cargo:rustc-link-lib=iconv"); }
RUST
printf '<!doctype html><title>Packaging probe</title>\n' > "$fixture/dist/index.html"
cat > "$fixture/src-tauri/tauri.conf.json" <<'JSON'
{"productName":"PackagingProbe","version":"0.1.0","identifier":"dev.headway.packaging-probe","build":{"frontendDist":"../dist"},"bundle":{"active":true,"targets":["app"]}}
JSON
export APPLE_SIGNING_IDENTITY=-
"$root/scripts/macos-bundle-config.sh" > "$fixture/adhoc.json"
APPLE_SIGNING_IDENTITY='Developer ID Application: Fixture' "$root/scripts/macos-bundle-config.sh" > "$fixture/developer.json"
node --input-type=module - "$fixture" <<'JS'
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const config = name => JSON.parse(readFileSync(`${process.argv[2]}/${name}.json`)).bundle.macOS;
assert.equal(config('adhoc').signingIdentity, '-');
assert.equal(config('adhoc').hardenedRuntime, false);
assert.equal(config('developer').hardenedRuntime, true);
JS
# Isolate the target cache even when the caller set CARGO_TARGET_DIR.
export CARGO_TARGET_DIR="$fixture/src-tauri/target"
# shellcheck source=scripts/macos-build-env.sh
. "$root/scripts/macos-build-env.sh"
cargo build --release --manifest-path "$fixture/src-tauri/Cargo.toml" > "$fixture/build.log" 2>&1
(cd "$fixture" && "$root/node_modules/.bin/tauri" bundle --bundles app --ci --config "$fixture/adhoc.json") > "$fixture/bundle.log" 2>&1
app="$fixture/src-tauri/target/release/bundle/macos/PackagingProbe.app"
binary="$app/Contents/MacOS/packaging-probe"
codesign --verify --deep --strict "$app" > "$fixture/verify.log" 2>&1
codesign --display --verbose=4 "$binary" > "$fixture/signature.log" 2>&1
"$root/scripts/verify-macos-artifacts.sh" "$binary" >> "$fixture/verify.log" 2>&1
printf 'PASS: strict signature, dependency allowlist, and ad-hoc runtime policy\n' >> "$fixture/verify.log"
"$binary" > "$fixture/runtime.log" 2>&1
grep -qx 'headway-packaging-runtime-ok' "$fixture/runtime.log"
otool -L "$binary" > "$fixture/dependencies.log"
printf 'PASS: generated signing policy, native Tauri signature flags, system dependencies, and runtime; artifacts=%s\n' "$fixture"
