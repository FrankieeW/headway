# Security model

## Report a vulnerability

Please report exploitable vulnerabilities privately to the maintainer through GitHub's private
security advisory reporting, when available, before posting exploit details in a public issue.
Include the affected version/commit, platform, a minimal reproduction, and the trust boundary
crossed. Never include real tokens, private repository contents or unredacted build logs.

## What Headway trusts

Headway is a software installer and build orchestrator, **not a sandbox**. Adding a source or enabling
automatic updates authorizes code from that publisher to run as your macOS user:

- Source recipes and verification commands execute through `/bin/sh -c`.
- npm packages may run lifecycle scripts; Homebrew formulae/casks and official updater commands
  execute their normal installation logic.
- Builds can read your files, use the network and access services available to your user. Automatic
  commit tracking can execute new upstream code without another interactive approval.
- Independent installations use separate payload destinations, not isolated application data or
  user profiles. Applications may continue sharing their normal data directories.

Inspect commands and dry runs do not build or install repository code. They can perform read-only
network requests or installed-version probes. Preview success is not a reservation: upstream refs,
permissions and files can change before the real operation.

## Credentials

GitHub tokens are obtained from `HEADWAY_GITHUB_TOKEN`, the macOS login keychain, `GITHUB_TOKEN`,
`GH_TOKEN`, or the GitHub CLI, in that order. Prefer a fine-grained, read-only token restricted to
the repositories you need; unauthenticated access is sufficient for many public repositories.

Headway never puts a token in git URLs or process arguments. Git receives a scoped HTTP header
through `GIT_CONFIG_*`; ordinary build/tool processes do not receive that credential environment.
`HEADWAY_GITHUB_TOKEN`, `GITHUB_TOKEN` and `GH_TOKEN` are removed from tool environments. Keychain
writes send the secret on stdin to `/usr/bin/security -i`; public status exposes only a masked value.

This removal is defense in depth, **not credential isolation**. Same-user code can invoke
`/usr/bin/security` to access the keychain item, inspect other credentials you keep on disk, or run
the GitHub CLI. Do not build untrusted code with credentials you cannot afford to expose.

`HEADWAY_API_URL` changes the authenticated GitHub API destination. Treat it as credential-sensitive
configuration, not repository input. Use HTTPS for GitHub Enterprise; plaintext HTTP is reserved for
loopback test servers. Never point it at an untrusted host or log pre-signed asset URLs.

## Download integrity is not publisher authenticity

Headway computes SHA-256 while downloading. When GitHub publishes an asset's `sha256:` digest,
Headway compares it before extraction and rejects mismatches. Some older releases have no digest:
that condition is reported as a warning, **not** described as verified integrity. TLS and a digest
served by the same publisher cannot establish that the publisher or repository is trustworthy.

Downloads are not automatically quarantined. Gatekeeper therefore does not assess every app
installed by Headway. Native code-signature verification for Headway's own bundle checks integrity
(including nested signed code); an ad-hoc signature is not a Developer ID or notarization verdict.
Other trusted apps may use a user-configured `verify_command` to enforce the policy they require,
including `spctl` assessment or an expected signing identity. A source build may intentionally be
unsigned. Headway does not claim a universal publisher-authentication guarantee for these builds.

## Local files and interrupted updates

Configuration, state and logs belong to the current user. Keep `HEADWAY_HOME`, custom source roots
and install destinations out of directories writable by untrusted users. Do not delete active lock,
transaction or recovery files while an operation is running. A backup is valid only for its recorded
repository/tracking identity and digest; do not edit cache manifests to bypass these checks.

A plain directory is never replaced as an install destination. Files and bundles are staged beside
the destination, and interrupted managed replacements retain recovery evidence. These safeguards do
not undo arbitrary side effects performed by a custom build command or external package manager.
Build logs can contain secrets printed by the tools themselves; review and redact logs before
sharing them, even when Headway's own credential variables have been removed.

Resource guards limit decoded output rather than claiming sandboxing: individual log lines are
bounded, each run's disk log is capped at 16 MiB, and renderer log events have a separate budget.
API JSON bodies are capped at 8 MiB, release downloads at 4 GiB. Tar/ZIP preflight streams decoded
content to a counting sink before extraction (8 GiB and 100,000 entries), at the cost of an extra
decode pass. Selected mounted/raw payloads are also bounded before copying. The system decoder and
ZIP metadata-preserving extraction can differ; these guards prevent ordinary expansion bombs, not
all parser bugs or arbitrary side effects of trusted build code. Short probes, network operations,
builds and verification commands have separate finite deadlines.

## Releases and supply chain

Unless a release explicitly says otherwise, Headway's macOS artifacts are **ad-hoc signed and not
notarized**. Ad-hoc signing protects bundle consistency, not publisher identity. For a manually
downloaded app, follow macOS's explicit first-open approval flow; do not disable Gatekeeper globally.

Release checksums detect damaged or substituted files relative to the published checksum list.
Build provenance, when attached, records which repository/workflow produced the artifacts; neither
checksums nor provenance replace trust in that repository and its dependencies. Workflow actions
are pinned to reviewed commit identities, and lockfiles are used for release builds.

Only a known clean installed build with matching repository provenance is eligible for automatic
self-update baseline adoption. Dirty local builds must never be represented as clean remote commits.
