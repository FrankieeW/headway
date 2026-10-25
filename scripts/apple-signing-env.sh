#!/usr/bin/env bash
# Normalizes the Apple signing / notarization environment for tauri-bundler.
#
# tauri-bundler decides whether credentials exist with `std::env::var_os(..).is_some()`
# (tauri-bundler `src/bundle/macos/sign.rs`), so a variable that is *set to the empty
# string* counts as present credentials: the bundler then runs `notarytool` with empty
# APPLE_ID/PASSWORD/TEAM and the build fails. GitHub Actions materializes a missing
# repository secret as an empty string, so the release job must `unset` these variables
# instead of merely leaving them empty.
#
# Source it (never execute it) before any `pnpm tauri build` / bundling step:
#     . scripts/apple-signing-env.sh
#
# Contract:
#   * all of APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID empty or unset -> unset them (ad-hoc)
#   * all three non-empty                                        -> keep them (notarize)
#   * only some of them non-empty                                -> fail with the missing names
#   * empty APPLE_SIGNING_IDENTITY                               -> unset it (config falls back to "-")
#   * APPLE_SIGNING_IDENTITY required when notarization creds are set
#   * APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD kept only as a complete pair
#
# Never print a variable value: only names and presence.

normalize_apple_signing_env() {
  local id="${APPLE_ID-}"
  local password="${APPLE_PASSWORD-}"
  local team="${APPLE_TEAM_ID-}"
  local identity="${APPLE_SIGNING_IDENTITY-}"
  local certificate="${APPLE_CERTIFICATE-}"
  local certificate_password="${APPLE_CERTIFICATE_PASSWORD-}"

  local present=0 missing=""
  # `[ -n ]` keeps values out of the log; only the variable names are reported.
  if [ -n "$id" ]; then present=$((present + 1)); else missing="${missing:+$missing }APPLE_ID"; fi
  if [ -n "$password" ]; then present=$((present + 1)); else missing="${missing:+$missing }APPLE_PASSWORD"; fi
  if [ -n "$team" ]; then present=$((present + 1)); else missing="${missing:+$missing }APPLE_TEAM_ID"; fi

  if [ "$present" -eq 0 ]; then
    unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
  elif [ "$present" -eq 3 ]; then
    if [ -z "$identity" ]; then
      printf 'apple signing: notarization credentials are set but APPLE_SIGNING_IDENTITY is empty; notarization needs a Developer ID signed app\n' >&2
      printf '::error::APPLE_SIGNING_IDENTITY must be set when APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID are set\n' >&2
      return 1
    fi
  else
    printf 'apple signing: incomplete notarization credentials, missing %s\n' "$missing" >&2
    printf '::error::APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID must all be set or all be unset (missing: %s)\n' "$missing" >&2
    return 1
  fi

  if [ -z "$identity" ]; then
    unset APPLE_SIGNING_IDENTITY
  fi

  # An incomplete certificate pair makes tauri-bundler attempt a broken import, so only
  # keep it when both halves exist; export nothing when neither was provided.
  if [ -z "$certificate" ] && [ -z "$certificate_password" ]; then
    unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD
  elif [ -z "$certificate" ] || [ -z "$certificate_password" ]; then
    printf 'apple signing: APPLE_CERTIFICATE and APPLE_CERTIFICATE_PASSWORD must be set together\n' >&2
    printf '::error::incomplete Apple Developer ID certificate environment\n' >&2
    return 1
  elif [ -z "${APPLE_SIGNING_IDENTITY-}" ]; then
    # Importing a .p12 without an identity would still sign ad-hoc: refuse instead.
    printf 'apple signing: APPLE_CERTIFICATE is set but APPLE_SIGNING_IDENTITY is empty\n' >&2
    printf '::error::APPLE_SIGNING_IDENTITY must name the certificate to sign with\n' >&2
    return 1
  fi

  if [ "$present" -eq 3 ]; then
    printf 'apple signing: Developer ID identity + notarization credentials configured\n' >&2
  elif [ -n "$identity" ]; then
    printf 'apple signing: Developer ID identity configured (no notarization)\n' >&2
  else
    printf 'apple signing: ad-hoc (no Developer ID / notarization credentials)\n' >&2
  fi
  return 0
}

# Run immediately on source/execution: callers only need `. scripts/apple-signing-env.sh`
# (direct execution is supported for tests and diagnostics).
normalize_apple_signing_env
