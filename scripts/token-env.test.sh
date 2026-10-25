#!/usr/bin/env bash
# Tests for scripts/token-env.sh. No secrets, no network.
set -uo pipefail

root="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
helper="$root/scripts/token-env.sh"
failures=0

pass() { printf 'ok: %s\n' "$1"; }
fail() { printf 'FAIL: %s\n' "$1" >&2; failures=$((failures + 1)); }

# Case: all three token variables are removed, everything else is preserved.
if ( set -euo pipefail
     export HEADWAY_GITHUB_TOKEN="ghp_headway_secret" GITHUB_TOKEN="ghp_github_secret" GH_TOKEN="ghp_gh_secret"
     export HEADWAY_UNRELATED="kept"
     . "$helper" >/dev/null 2>&1
     [ -z "${HEADWAY_GITHUB_TOKEN+x}" ] && [ -z "${GITHUB_TOKEN+x}" ] && [ -z "${GH_TOKEN+x}" ] &&
       [ "${HEADWAY_UNRELATED:-}" = "kept" ] ); then
  pass "all three token variables are stripped"
else
  fail "all three token variables are stripped"
fi

# Case: absent variables stay absent (idempotent and safe under `set -u`).
if ( set -euo pipefail
     unset HEADWAY_GITHUB_TOKEN GITHUB_TOKEN GH_TOKEN
     . "$helper" >/dev/null 2>&1
     . "$helper" >/dev/null 2>&1
     [ -z "${HEADWAY_GITHUB_TOKEN+x}" ] && [ -z "${GITHUB_TOKEN+x}" ] && [ -z "${GH_TOKEN+x}" ] ); then
  pass "absent token variables stay absent (idempotent)"
else
  fail "absent token variables stay absent (idempotent)"
fi

# Case: stripping never prints a token value.
output="$( set -euo pipefail
          export HEADWAY_GITHUB_TOKEN="ghp_leak_check_headway" GITHUB_TOKEN="ghp_leak_check_github" GH_TOKEN="ghp_leak_check_gh"
          . "$helper" 2>&1 )"
case "$output" in
  *ghp_leak_check*) fail "helper must not print token values" ;;
  *) pass "helper prints no token values" ;;
esac

# Case: direct execution succeeds (used by scripts that cannot `source`).
if env HEADWAY_GITHUB_TOKEN="x" GITHUB_TOKEN="y" GH_TOKEN="z" bash "$helper" >/dev/null 2>&1; then
  pass "direct execution succeeds"
else
  fail "direct execution succeeds"
fi

# Case: `bash -n` syntax check of every script that sources the helper.
for script in build-headway.sh macos-build-env.sh test-macos-packaging.sh; do
  if bash -n "$root/scripts/$script"; then
    pass "syntax: scripts/$script"
  else
    fail "syntax: scripts/$script"
  fi
done

if [ "$failures" -ne 0 ]; then
  printf 'token-env: %d test(s) failed\n' "$failures" >&2
  exit 1
fi
printf 'token-env: all tests passed\n'
