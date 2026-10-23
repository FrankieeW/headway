#!/usr/bin/env bash
#
# Bump the Headway version in every place it lives:
#
#   - Cargo.toml                  ([workspace.package] version)
#   - src-tauri/tauri.conf.json   ("version")
#   - package.json                ("version")
#
# Usage:
#   ./scripts/bump-version.sh patch   # 0.1.0       -> 0.1.1
#   ./scripts/bump-version.sh minor   # 0.1.0       -> 0.2.0
#   ./scripts/bump-version.sh major   # 0.1.0       -> 1.0.0
#   ./scripts/bump-version.sh beta    # 0.1.0       -> 0.1.0-beta0
#                                      # 0.1.0-beta3 -> 0.1.0-beta4
#   ./scripts/bump-version.sh patch   # 0.1.0-beta3 -> 0.1.0 (finalises the release)
#
# Options:
#   --dry-run       print the computed version and the changes, change nothing
#   --allow-dirty   run even when the git working tree has uncommitted changes
#
# The script only edits files. It never commits, tags or pushes: it prints the
# exact follow-up commands to run yourself.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

CARGO_TOML="Cargo.toml"
PACKAGE_JSON="package.json"
TAURI_CONF="src-tauri/tauri.conf.json"

usage() {
  cat <<'EOF'
Usage: scripts/bump-version.sh <patch|minor|major|beta> [--dry-run] [--allow-dirty]

  patch   0.1.0 -> 0.1.1        (0.1.0-beta3 -> 0.1.0, finalises the release)
  minor   0.1.0 -> 0.2.0
  major   0.1.0 -> 1.0.0
  beta    0.1.0 -> 0.1.0-beta0  (0.1.0-beta3 -> 0.1.0-beta4)

Options:
  --dry-run       print the computed version without touching any file
  --allow-dirty   do not refuse to run on a dirty git working tree
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

# Replaces the top-level "version" string of a JSON file, leaving every other
# byte of the file (indentation, key order, trailing newline) untouched.
JSON_VERSION_SCRIPT="$(cat <<'NODE_EOF'
const fs = require("fs");

// With `node -e script a b`, argv[0] is node itself and argv[1] is "a".
const [file, nextVersion] = process.argv.slice(1);
const src = fs.readFileSync(file, "utf8");

let depth = 0;
let i = 0;
let value = null;
let valueStart = -1;
let valueEnd = -1;

while (i < src.length) {
  const c = src[i];
  if (c === "{" || c === "[") {
    depth += 1;
    i += 1;
    continue;
  }
  if (c === "}" || c === "]") {
    depth -= 1;
    i += 1;
    continue;
  }
  if (c !== '"') {
    i += 1;
    continue;
  }

  // Read the string starting at i.
  let j = i + 1;
  let text = "";
  while (j < src.length) {
    if (src[j] === "\\") {
      text += src[j] + src[j + 1];
      j += 2;
      continue;
    }
    if (src[j] === '"') break;
    text += src[j];
    j += 1;
  }
  const stringEnd = j;

  // Is this string a key? Look for a ":" after it.
  let k = stringEnd + 1;
  while (k < src.length && /\s/.test(src[k])) k += 1;
  if (depth === 1 && text === "version" && src[k] === ":") {
    let v = k + 1;
    while (v < src.length && /\s/.test(src[v])) v += 1;
    if (src[v] !== '"') {
      console.error(file + ': the top-level "version" is not a string');
      process.exit(1);
    }
    const start = v + 1;
    let w = start;
    let raw = "";
    while (w < src.length) {
      if (src[w] === "\\") {
        raw += src[w] + src[w + 1];
        w += 2;
        continue;
      }
      if (src[w] === '"') break;
      raw += src[w];
      w += 1;
    }
    value = raw;
    valueStart = start;
    valueEnd = w;
    break;
  }
  i = stringEnd + 1;
}

if (valueStart < 0) {
  console.error(file + ': no top-level "version" field found');
  process.exit(1);
}
if (value === nextVersion) {
  console.log(file + ": version already " + nextVersion);
  process.exit(0);
}

const out = src.slice(0, valueStart) + nextVersion + src.slice(valueEnd);
JSON.parse(out);
fs.writeFileSync(file, out);
NODE_EOF
)"

read_workspace_version() {
  awk '
    /^\[workspace\.package\][[:space:]]*$/ { in_pkg = 1; next }
    /^\[/ { in_pkg = 0 }
    in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
      line = $0
      sub(/^[^=]*=[[:space:]]*/, "", line)
      gsub(/["[:space:]]/, "", line)
      print line
      exit
    }
  ' "$CARGO_TOML"
}

write_workspace_version() {
  local new_version="$1"
  local tmp
  tmp="$(mktemp)"
  awk -v new_version="$new_version" '
    /^\[workspace\.package\][[:space:]]*$/ { in_pkg = 1; print; next }
    /^\[/ { in_pkg = 0; print; next }
    in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
      indent = $0
      sub(/[^[:space:]].*$/, "", indent)
      print indent "version = \"" new_version "\""
      next
    }
    { print }
  ' "$CARGO_TOML" > "$tmp"
  # `cat >` keeps the original file mode and inode.
  cat "$tmp" > "$CARGO_TOML"
  rm -f "$tmp"
}

write_json_version() {
  local file="$1"
  local new_version="$2"
  node -e "$JSON_VERSION_SCRIPT" "$file" "$new_version"
}

# --- arguments ---------------------------------------------------------------

level=""
dry_run=0
allow_dirty=0

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry_run=1; shift ;;
    --allow-dirty) allow_dirty=1; shift ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) die "unknown option: $1 (try --help)" ;;
    *)
      if [ -n "$level" ]; then
        die "unexpected argument: $1 (try --help)"
      fi
      level="$1"
      shift
      ;;
  esac
done
if [ $# -gt 0 ]; then
  die "unexpected argument: $1 (try --help)"
fi

case "$level" in
  patch|minor|major|beta) ;;
  "") usage >&2; die "missing version bump level (patch|minor|major|beta)" ;;
  *) die "unknown version bump level: $level (expected patch|minor|major|beta)" ;;
esac

# --- preconditions -----------------------------------------------------------

command -v node >/dev/null 2>&1 || die "node is required to update the JSON files"
command -v cargo >/dev/null 2>&1 || die "cargo is required to update Cargo.lock"

git rev-parse --git-dir >/dev/null 2>&1 || die "not inside a git repository"

if [ "$allow_dirty" -ne 1 ] && [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  die "the git working tree has uncommitted changes; commit them or pass --allow-dirty"
fi

for file in "$CARGO_TOML" "$PACKAGE_JSON" "$TAURI_CONF"; do
  [ -f "$file" ] || die "$file not found (run the script from the repository root)"
done

current_version="$(read_workspace_version)"
[ -n "$current_version" ] || die "could not read the [workspace.package] version from $CARGO_TOML"

# --- compute the new version -------------------------------------------------

if [[ "$current_version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
  cur_major="${BASH_REMATCH[1]}"
  cur_minor="${BASH_REMATCH[2]}"
  cur_patch="${BASH_REMATCH[3]}"
else
  die "cannot parse the current version '$current_version'"
fi

base_version="$cur_major.$cur_minor.$cur_patch"
pre_tag=""
pre_num=0
if [ "$current_version" != "$base_version" ]; then
  # A prerelease suffix such as "-beta3": the tag is the leading letters, the
  # number the trailing digits.
  pre="${current_version#"$base_version"-}"
  if [ -z "$pre" ] || [ "$pre" = "$current_version" ]; then
    die "cannot parse the prerelease in '$current_version'"
  fi
  if [[ "$pre" =~ ([0-9]+)$ ]]; then
    pre_num="${BASH_REMATCH[1]}"
    pre_tag="${pre%"$pre_num"}"
  else
    pre_tag="$pre"
  fi
  if [ -z "$pre_tag" ]; then
    die "cannot parse the prerelease in '$current_version'"
  fi
fi

case "$level" in
  patch)
    if [ -n "$pre_tag" ]; then
      # 0.1.0-beta3 -> 0.1.0, finalises the release.
      new_version="$cur_major.$cur_minor.$cur_patch"
    else
      new_version="$cur_major.$cur_minor.$((cur_patch + 1))"
    fi
    ;;
  minor)
    new_version="$cur_major.$((cur_minor + 1)).0"
    ;;
  major)
    new_version="$((cur_major + 1)).0.0"
    ;;
  beta)
    if [ "$pre_tag" = "beta" ]; then
      new_version="$cur_major.$cur_minor.$cur_patch-beta$((pre_num + 1))"
    else
      new_version="$cur_major.$cur_minor.$cur_patch-beta0"
    fi
    ;;
esac

if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z][A-Za-z0-9]*[0-9]+)?$ ]]; then
  die "the computed version '$new_version' is not a valid version"
fi

# --- apply -------------------------------------------------------------------

echo "==> current version: $current_version"
echo "==> new version:     $new_version"

if [ "$dry_run" -eq 1 ]; then
  echo "==> dry run, these files would be updated:"
  echo "      $CARGO_TOML ([workspace.package] version)"
  echo "      $TAURI_CONF (\"version\")"
  echo "      $PACKAGE_JSON (\"version\")"
  echo "      Cargo.lock (via cargo update)"
else
  write_workspace_version "$new_version"
  echo "==> updated $CARGO_TOML"

  write_json_version "$TAURI_CONF" "$new_version"
  echo "==> updated $TAURI_CONF"

  write_json_version "$PACKAGE_JSON" "$new_version"
  echo "==> updated $PACKAGE_JSON"

  echo "==> updating Cargo.lock"
  if ! cargo update --workspace --offline; then
    echo "    offline update failed, retrying online"
    cargo update --workspace
  fi
  echo "==> updated Cargo.lock"
fi

echo
echo "New version: $new_version"
echo "Next steps:"
echo "  git add -A && git commit -m 'chore: bump version to v$new_version' && git tag v$new_version && git push && git push --tags"
