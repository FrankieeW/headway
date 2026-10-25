#!/usr/bin/env bash
# Tests for scripts/apple-signing-env.sh. No secrets, no network.
set -uo pipefail

root="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
helper="$root/scripts/apple-signing-env.sh"
failures=0

pass() { printf 'ok: %s\n' "$1"; }
fail() { printf 'FAIL: %s\n' "$1" >&2; failures=$((failures + 1)); }

# Case: all empty strings (GitHub Actions materializes missing secrets this way) are unset.
if ( set -euo pipefail
     export APPLE_ID="" APPLE_PASSWORD="" APPLE_TEAM_ID="" APPLE_SIGNING_IDENTITY=""
     . "$helper" >/dev/null 2>&1
     [ -z "${APPLE_ID+x}" ] && [ -z "${APPLE_PASSWORD+x}" ] && [ -z "${APPLE_TEAM_ID+x}" ] &&
       [ -z "${APPLE_SIGNING_IDENTITY+x}" ] ); then
  pass "all-empty credentials are unset"
else
  fail "all-empty credentials are unset"
fi

# Case: nothing exported at all stays unset (idempotent).
if ( set -euo pipefail
     unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_SIGNING_IDENTITY
     . "$helper" >/dev/null 2>&1
     [ -z "${APPLE_ID+x}" ] && [ -z "${APPLE_TEAM_ID+x}" ] ); then
  pass "absent credentials stay absent"
else
  fail "absent credentials stay absent"
fi

# Case: a complete notarization set is preserved and values never leak to the log.
if output="$( set -euo pipefail
              export APPLE_ID="id@example.test" APPLE_PASSWORD="password-should-not-appear" APPLE_TEAM_ID="TEAM123456"
              export APPLE_SIGNING_IDENTITY="Developer ID Application: Example (TEAM123456)"
              . "$helper" 2>&1 1>/dev/null
              [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] &&
                [ -n "${APPLE_SIGNING_IDENTITY:-}" ] )"; then
  case "$output" in
    *"password-should-not-appear"*|*"id@example.test"*|*"TEAM123456"*)
      fail "helper output must not contain credential values" ;;
    *) pass "complete notarization credentials are preserved" ;;
  esac
else
  fail "complete notarization credentials are preserved"
fi

# Case: a partial notarization set fails and names only the missing variables.
if output="$( set -euo pipefail
              export APPLE_ID="id@example.test" APPLE_PASSWORD="" APPLE_TEAM_ID=""
              unset APPLE_SIGNING_IDENTITY
              . "$helper" 2>&1 )"; then
  fail "partial notarization credentials are rejected"
elif printf '%s' "$output" | grep -q 'APPLE_PASSWORD' &&
  printf '%s' "$output" | grep -q 'APPLE_TEAM_ID' &&
  ! printf '%s' "$output" | grep -q 'id@example.test'; then
  pass "partial notarization credentials are rejected"
else
  fail "partial notarization credentials are rejected (wrong message)"
fi

# Case: notarization credentials without a signing identity fail explicitly.
if ( set -euo pipefail
     export APPLE_ID="id@example.test" APPLE_PASSWORD="p" APPLE_TEAM_ID="T" APPLE_SIGNING_IDENTITY=""
     . "$helper" >/dev/null 2>&1 ); then
  fail "notarization requires a signing identity"
else
  pass "notarization requires a signing identity"
fi

# Case: an incomplete certificate pair fails explicitly.
if ( set -euo pipefail
     export APPLE_CERTIFICATE="base64-cert"
     unset APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_SIGNING_IDENTITY
     . "$helper" >/dev/null 2>&1 ); then
  fail "incomplete certificate pair is rejected"
else
  pass "incomplete certificate pair is rejected"
fi

# Case: a certificate without a signing identity is rejected.
if ( set -euo pipefail
     export APPLE_CERTIFICATE="base64-cert" APPLE_CERTIFICATE_PASSWORD="cert-password" APPLE_SIGNING_IDENTITY=""
     unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
     . "$helper" >/dev/null 2>&1 ); then
  fail "certificate without a signing identity is rejected"
else
  pass "certificate without a signing identity is rejected"
fi

# Case: executing the helper directly is supported (same normalization).
if env APPLE_ID="" APPLE_PASSWORD="" APPLE_TEAM_ID="" APPLE_SIGNING_IDENTITY="" bash "$helper" >/dev/null 2>&1; then
  pass "direct execution succeeds for the ad-hoc case"
else
  fail "direct execution succeeds for the ad-hoc case"
fi

if [ "$failures" -ne 0 ]; then
  printf 'apple-signing-env: %d test(s) failed\n' "$failures" >&2
  exit 1
fi
printf 'apple-signing-env: all tests passed\n'
