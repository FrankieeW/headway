#!/usr/bin/env bash
# Build-only recipes for Headway's own commit-tracking presets. Never installs or relaunches.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-headway.sh app|cli [--target aarch64-apple-darwin|x86_64-apple-darwin]

app  Build Headway.app, including its matching headway CLI, into .headway-out/Headway.app.
cli  Build only the command-line tool into .headway-out/headway (no Node/pnpm needed).

The native Rust target is used by default. CARGO_TARGET_DIR and CARGO_BUILD_TARGET are
honored. Cargo and pnpm use their lockfiles; existing build caches are reused.
EOF
}

die() { printf 'headway build: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "required tool is missing: $1"; }

mode="${1:-}"
case "$mode" in
  -h|--help) usage; exit 0 ;;
  app|cli) shift ;;
  *) usage >&2; exit 2 ;;
esac
requested_target="${CARGO_BUILD_TARGET:-}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target)
      [ "$#" -ge 2 ] || die '--target needs a Rust target triple'
      requested_target="$2"
      shift 2
      ;;
    *) die "unexpected argument: $1" ;;
  esac
done

project_root="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
# Direct invocations get the same token boundary as Headway's process::Cmd.
unset HEADWAY_GITHUB_TOKEN
# App artifacts must resolve SDK libraries, not a developer's Homebrew search path.
# Keep the Rust-only CLI recipe's environment contract unchanged.
if [ "$mode" = app ]; then
  need node
  need xcrun
  . "$project_root/scripts/macos-build-env.sh"
fi
need git
need cargo
need rustc
host_target="$(rustc -vV | awk '/^host: / { print $2 }')"
build_target="${requested_target:-$host_target}"
case "$build_target" in
  aarch64-apple-darwin|x86_64-apple-darwin) ;;
  *) die "unsupported macOS Rust target: $build_target" ;;
esac

HEADWAY_BUILD_SHA="$(git rev-parse --verify HEAD)"
headway_origin="$(git remote get-url origin 2>/dev/null || true)"
HEADWAY_BUILD_REPOSITORY=''
case "$headway_origin" in
  https://github.com/*) HEADWAY_BUILD_REPOSITORY="${headway_origin#https://github.com/}" ;;
  http://github.com/*) HEADWAY_BUILD_REPOSITORY="${headway_origin#http://github.com/}" ;;
  git@github.com:*) HEADWAY_BUILD_REPOSITORY="${headway_origin#git@github.com:}" ;;
  ssh://git@github.com/*) HEADWAY_BUILD_REPOSITORY="${headway_origin#ssh://git@github.com/}" ;;
esac
HEADWAY_BUILD_REPOSITORY="${HEADWAY_BUILD_REPOSITORY%/}"
HEADWAY_BUILD_REPOSITORY="${HEADWAY_BUILD_REPOSITORY%.git}"
if [[ ! "$HEADWAY_BUILD_REPOSITORY" =~ ^[A-Za-z0-9-]+/[A-Za-z0-9_.-]+$ ]]; then
  HEADWAY_BUILD_REPOSITORY=''
fi
export HEADWAY_BUILD_REPOSITORY
HEADWAY_BUILD_DIRTY=false
checkout_status="$(git status --porcelain --untracked-files=normal)"
if [ -n "$checkout_status" ]; then
  HEADWAY_BUILD_DIRTY=true
fi
HEADWAY_BUILD_TARGET="$build_target"
export HEADWAY_BUILD_SHA HEADWAY_BUILD_DIRTY HEADWAY_BUILD_TARGET

target_dir="${CARGO_TARGET_DIR:-$project_root/target}"
case "$target_dir" in
  /*) ;;
  *) target_dir="$project_root/$target_dir" ;;
esac
export CARGO_TARGET_DIR="$target_dir"
release_dir="$target_dir/$build_target/release"
output_dir="$project_root/.headway-out"
mkdir -p "$output_dir"
scratch="$(mktemp -d "$output_dir/.build-XXXXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT

printf 'Building %s from %s (dirty=%s) for %s\n' "$mode" "$HEADWAY_BUILD_SHA" "$HEADWAY_BUILD_DIRTY" "$build_target" >&2
cargo build --release --locked -p headway-cli --target "$build_target"
[ -x "$release_dir/headway" ] || die "CLI artifact is missing: $release_dir/headway"

if [ "$mode" = cli ]; then
  cp "$release_dir/headway" "$scratch/headway"
  chmod +x "$scratch/headway"
  # Rename instead of truncating a potentially running CLI artifact.
  mv -f "$scratch/headway" "$output_dir/headway"
  printf '%s\n' "$output_dir/headway"
  exit 0
fi

[ "$(uname -s)" = Darwin ] || die 'building the app requires macOS'
signing_identity="${APPLE_SIGNING_IDENTITY:--}"
signing_options=runtime
[ "$signing_identity" != - ] || signing_options=0
codesign --force --sign "$signing_identity" --options "$signing_options" "$release_dir/headway"
need node
need pnpm
pnpm install --frozen-lockfile
# Resolve the CLI path from the chosen target/cache. Never bundle a stale host CLI.
"$project_root/scripts/macos-bundle-config.sh" "$release_dir/headway" > "$scratch/tauri.json"
pnpm exec tauri build --target "$build_target" --bundles app --ci --config "$scratch/tauri.json" -- --locked
bundle="$release_dir/bundle/macos/Headway.app"
[ -x "$bundle/Contents/MacOS/headway-desktop" ] || die "app artifact is missing: $bundle"
[ -x "$bundle/Contents/MacOS/headway" ] || die 'the app has no bundled CLI'
"$project_root/scripts/verify-macos-artifacts.sh" "$bundle/Contents/MacOS/headway-desktop" "$bundle/Contents/MacOS/headway"
if [ "$build_target" = "$host_target" ]; then
  # Signing may change Mach-O bytes; compare the compiled revision instead.
  HEADWAY_HOME="$scratch/home" "$release_dir/headway" --json version > "$scratch/cli-version.json"
  HEADWAY_HOME="$scratch/home" "$bundle/Contents/MacOS/headway" --json version > "$scratch/bundled-version.json"
  cmp -s "$scratch/cli-version.json" "$scratch/bundled-version.json" || die 'the bundled CLI has different build metadata'
fi
/usr/bin/ditto "$bundle" "$scratch/Headway.app"
codesign --verify --deep --strict "$scratch/Headway.app"
if [ -e "$output_dir/Headway.app" ] || [ -L "$output_dir/Headway.app" ]; then
  mv "$output_dir/Headway.app" "$scratch/previous.app"
fi
if ! mv "$scratch/Headway.app" "$output_dir/Headway.app"; then
  if [ -e "$scratch/previous.app" ]; then
    mv "$scratch/previous.app" "$output_dir/Headway.app"
  fi
  die 'could not publish the app artifact'
fi
printf '%s\n' "$output_dir/Headway.app"
