# Review remediation

This tracks the October 2026 project-wide review as engineering outcomes rather than blindly
implementing every suggested refactor. Regression tests live next to the affected modules; release
publication is gated on the same checks. No existing public error code was renamed.

## Contracts and previews

- CLI result `ok` agrees with the exit code for ordinary and partially successful commands. Batch
  data/errors and completed automatic-update outcomes are retained instead of erased by a later failure.
- Broken pipes are handled without panics or stderr backtraces. Closing stdout does not turn a failed
  command into a successful operation. Help/version remain conventional transport exceptions.
- Asset-only `add --no-inspect` no longer preserves an invalid empty source fallback.
- Add/edit/import previews use shared engine validation. Update previews—including `--all`—observe
  versions without saving check/install state or taking recovery actions. Partial preview failures do
  not hide successful actionable plans for other apps.
- Input-derived validation failures and configuration files that were already invalid are classified
  separately. Shared check summaries replace desktop-only aggregation; per-item errors carry exit codes.
- XML collection names, nested command paths, empty output-format environment values, command metadata,
  help text, terminal behavior, purge previews and safe follow-up hints have regression coverage.

## Engine, storage and installed provenance

- Long-lived app locks live outside disposable cache storage. A transitional legacy guard also keeps
  a still-running v0.4 desktop mutually exclusive with the upgraded CLI during self-update. Store-lock
  contention is bounded; state writes use private atomic temporaries, file and directory synchronization,
  and a last-known-good backup.
- State schema 0 is migrated in memory, future schemas are rejected unchanged, and corrupt provenance
  is never silently reset. Diagnostics identify the recovery backup and preserve the original.
- Settings and knowable destinations are validated; slow canonicalization/observations stay outside
  store critical sections. Registration clears stale identity state, and source replacement preserves
  prior origin before publishing a changed subscription.
- Commit-prefix matching remains supported only in commit contexts; tag/version identities compare
  exactly. Homebrew linked-keg evidence and workspace-glob build suggestions are handled explicitly.
- Interrupted managed payload swaps retain validated recovery intent, and state certainty is withheld
  until the transaction is reconciled under the app lock. Removal/repointing must recover first rather
  than orphaning a transaction or attaching it to a reused app id.
- Multi-step automatic updates expose successful steps even when a subsequent step fails. Config races
  use additive, correctly classified errors rather than pretending another update is running.

## Execution, network and artifacts

- Build output has bounded individual lines, disk logs and renderer events. Captured probe output,
  network responses, downloaded files and archive preflight have explicit budgets and deadlines.
- Git transfers, including first clone and replayable checkout/submodule operations, use bounded retries.
  HTTP transient retries honor short server hints without sleeping through distant rate-limit resets.
- API endpoint policy requires HTTPS outside loopback tests; credential-bearing URLs and rejected input
  are redacted/bounded in diagnostics. Pagination follows self-constructed page numbers, not arbitrary
  server URLs; bounded tag scans expose truncation rather than falsely claiming completeness.
- Asset names must be safe path components. Explicit patterns still reject known non-installable
  checksum/platform artifacts; Apple Silicon under Rosetta selects the native host architecture.
- Artifact selection rejects ambiguous outputs and post-substitution/symlink escapes. Generic trusted
  build recipes remain responsible for producing their claimed output; an unchanged incremental build
  is not rejected merely because its mtime or bytes are unchanged. Known self-hosted builds verify stamps.
- Missing GitHub digests produce warning evidence in plans, logs, machine outcomes and desktop toasts.
  They are not called verified integrity. All three GitHub token-source environment variables are removed
  from build/tool processes, including standalone build/packaging recipes.

## Desktop and renderer

- Normal and revision updates share synchronous admission: refusal is observable before IPC success.
  Repeated quit cannot bypass the cancellation drain, and scheduled builds do not block new checks.
- Restart monitoring uses a cheap pending-request gate. Expensive status probes poll only during a
  handshake; the renderer relies on backend change/focus events rather than redundant whole-list polling.
- Tray and notifications use the persisted language. Optimistic settings roll back on rejection;
  diagnostics and system-action errors are visible and localized; document language follows i18n.
- Registered repository bindings, shared result/error types, realistic mock update/cancel events and
  bounded finished-run caches are covered. Warning-bearing updates are not shown as ordinary success.
- IPC contract tests cover the handler surface; the TypeScript-AST gate checks binding names/argument
  keys against registered Rust command signatures. Renderer text remains text, never untrusted HTML.
- Real vendor chunking reduces the initial application chunk without hiding size warnings.

## Release and supply chain

- Actions are pinned to resolved full commit identities, job permissions/timeouts are explicit, and
  dependency/security checks run in CI. Vulnerable development tooling is updated rather than ignored.
- A release guard checks the tag, commit and all version sources; both native Apple Silicon and Intel
  builds execute smoke/version checks. The publishing job waits for verification and both builds.
- Only the three intended artifact types are uploaded/attested, not unpacked staging directories.
  Build provenance is attached; checksums cover the combined release artifact set.
- Empty-but-present Apple secret variables are normalized before invoking Tauri. Partial signing/notary
  credentials fail early without disclosing values. This prevents bogus notarization attempts with
  empty credentials; tests cover the actual environment policy.
- Agent/tool scratch directories are ignored so local review artifacts do not contaminate clean build
  identity. The architecture, security model, agent skill and configuration migration guidance are updated.

## Deliberate qualifications, not unfulfilled security guarantees

- **No notarization credentials exist.** Artifacts remain explicitly ad-hoc signed and unnotarized.
  This cannot be fixed by code alone; a real Developer ID certificate/notary account is required.
- **Builds are not sandboxed.** Same-user keychain access and package-manager lifecycle scripts are part
  of the documented trust boundary. Credential environment removal is defense in depth, not isolation.
- **No universal `spctl` rejection.** Deliberately unsigned trusted source builds remain supported. Users
  can enforce their desired publisher/Team-ID policy with a verification command. `codesign --deep`
  is not removed from verification based on advice about its deprecation for signing.
- **No mtime-based revision proof.** Legitimate incremental output can stay byte-identical across commits;
  provenance cannot be inferred generically from modification timestamps.
- **No silent corrupt-state reset.** Losing installed origin/history without consent would be worse than
  a recoverable diagnostic. Backup recovery is explicit and documented.
- **Conservative polling is intentional.** The lightweight backend watcher still polls with an adaptive
  interval; adopting a filesystem-event dependency is optional optimization, not a correctness fix.
- **Conventional CLI exceptions remain.** Help/version are text; signals may interrupt read-only commands
  before a result. Implicit non-TTY token piping is preserved for compatibility and documented.

## Verification

The frozen remediation passed `cargo fmt --all --check`, locked workspace Clippy with warnings denied,
and the full locked workspace test run: **570 Rust tests passed** (448 core unit, 2 public inventory
contracts, 71 CLI and 49 desktop). Frontend typecheck, all **205 tests**, renderer build, the 55-command
IPC binding check, 16 release-planner/guard tests, signing/token environment tests and the native macOS
packaging regression passed. The full pnpm advisory audit reported no known vulnerabilities.

For the precise trust boundary, see [SECURITY.md](../SECURITY.md); for module/lifecycle invariants and
the release checklist, see [architecture.md](architecture.md).
