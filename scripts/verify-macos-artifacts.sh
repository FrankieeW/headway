#!/usr/bin/env bash
# Headway currently bundles no private dylibs. Reject machine-local dependencies,
# including unresolved @rpath references, until an explicit bundling policy exists.
set -euo pipefail
[ "$#" -gt 0 ] || { echo 'usage: verify-macos-artifacts.sh MACH_O ...' >&2; exit 2; }
for binary in "$@"; do
  dependencies="$(otool -L "$binary")"
  libraries="$(printf '%s\n' "$dependencies" | sed '1d; s/^[[:space:]]*//; s/ (compatibility version.*$//')"
  [ -n "$libraries" ] || { printf 'No Mach-O dependencies found: %s\n' "$binary" >&2; exit 1; }
  while IFS= read -r dependency; do
    case "$dependency" in
      /System/Library/*|/usr/lib/*) ;;
      *) printf 'Non-portable dependency in %s: %s\nRebuild with a clean linker environment (especially LIBRARY_PATH) and a portable Rust toolchain.\n' "$binary" "$dependency" >&2; exit 1 ;;
    esac
  done <<< "$libraries"
  signature="$(codesign --display --verbose=4 "$binary" 2>&1)"
  flags="$(printf '%s\n' "$signature" | sed -n '/^CodeDirectory .*flags=/p')"
  [ -n "$flags" ] || { printf 'Missing code signature flags: %s\n' "$binary" >&2; exit 1; }
  case "${APPLE_SIGNING_IDENTITY:--}:$flags" in
    -:*runtime*) printf 'Ad-hoc artifact unexpectedly enables hardened runtime: %s\n' "$binary" >&2; exit 1 ;;
    -:*) ;;
    *runtime*) ;;
    *) printf 'Developer ID artifact lacks hardened runtime: %s\n' "$binary" >&2; exit 1 ;;
  esac
done
