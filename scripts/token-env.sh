#!/usr/bin/env bash
# Strips Headway/GitHub credential variables before running build or packaging commands.
#
# Headway's async runner (`process::Cmd::new`) removes HEADWAY_GITHUB_TOKEN, GITHUB_TOKEN and
# GH_TOKEN from every subprocess. Manual invocations of the build/packaging scripts must apply
# the same boundary: repository build scripts, npm lifecycle scripts, cargo build scripts and
# archive tools must never inherit a GitHub token.
#
# Source it (or execute it) before any untrusted build/package command:
#     . scripts/token-env.sh
#
# Deliberately NOT used by `scripts/update-homebrew-tap.mjs`: that release-automation script
# authenticates `gh`/`git` with the operator's own GH_TOKEN on purpose.
#
# Silently does nothing when the variables are already absent. Never prints their values.

strip_token_env() {
  unset HEADWAY_GITHUB_TOKEN GITHUB_TOKEN GH_TOKEN
}

strip_token_env
