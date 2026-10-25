# Architecture and invariants

Headway is one local engine with two front ends: the `headway` CLI and a Tauri menu-bar app. The
React renderer talks only to Tauri IPC; it does not fetch GitHub data or execute commands directly.

## Boundaries

| Layer | Responsibility | Must not do |
|---|---|---|
| `headway-core::Headway` | Validate configuration, observe installed software, plan/check/update/restore | Depend on a UI or terminal |
| `service/*` | Orchestrate policy and persistence around engine mechanisms | Duplicate install mechanisms |
| `update::Job` | Execute one build/download/install target and return evidence | Independently invent revision baselines |
| `recovery/*` + `install` | Stage payloads, protect destinations, retain backups, recover interrupted replacements | Replace a plain directory or trust unvalidated cache paths |
| CLI commands | Parse flags, invoke `Headway`, render one human/JSON/XML result | Maintain a different validation path for previews |
| Tauri commands | Adapt IPC arguments/results and desktop task admission | Reimplement shared result aggregation |
| React API/hooks | Typed IPC, cache invalidation, loading/error states | Treat untrusted text as HTML or assume optimistic state is persisted |

`HeadwayBuilder` injects paths, secret storage, GitHub API base and command environment. Tests use
an isolated `HEADWAY_HOME`, in-memory secrets, wiremock servers, real temporary git repositories,
and fake package-manager executables. Nothing should require a developer's real token or config.

## Concurrency and storage

Long-running operations use a per-app lock shared by the CLI and desktop. Configuration/state
read-modify-write operations use a separate store lock and reload the file after acquiring it.
Acquire app locks before store locks, never the reverse. Do not await network calls or probe slow
payload volumes while holding the store lock. Presentation values should be built after a mutation
has left its critical section.

Configuration expresses user intent; state records observed or installer-proven facts. Existing
bytes at a destination do not establish their revision. Switching repositories preserves the old
payload's origin separately and must not relabel its SHA as belonging to the new source. Legacy
configuration/state must either migrate additively or fail explicitly without discarding provenance.

Atomic replacement of a state file is not the same as a transaction spanning both an installed
payload and its state record. An interrupted replacement must retain enough validated evidence to
reconcile the two; it must not mark uncertain bytes as a successfully installed revision.

## Machine contract

- Error `code` strings and their exit codes are public API. Add codes; never repurpose existing ones.
- JSON fields are snake_case and shared across the CLI, desktop IPC and TypeScript types.
- JSON/XML `ok` agrees with the exit status, including partial batch failures; retain per-item errors
  and any completed auto-update steps. Help/version and early signal/closed-pipe termination are
  explicit transport exceptions, not additional result documents.
- Dry runs use the same validation and target resolution as real commands, but do not save app or
  installed/check state. They do not reserve a path or freeze upstream state.
- IPC tests exercise the real handler; a separate binding check compares renderer invoke names and
  argument keys against registered Rust commands. Runtime tests still cover serialization and errors.

## External code and credentials

See [the security model](../SECURITY.md). Build commands execute trusted publisher code with the
user's privileges; environment sanitization and output bounds do not create a sandbox. Git receives
scoped credentials, while ordinary build processes do not inherit Headway's token-source variables.
Never log token values or pre-signed redirect query strings.

## Desktop lifecycle

Admit an update before returning success from its IPC command. A registered run owns its cancellation
token and releases admission on every completion/error path. Restart admission closes new updates
before consuming a pending restart request. Quit must allow cancellation to terminate process groups
before releasing update locks; repeated quit requests must not skip that discipline.

Backend change events refresh renderer query data; interval polling is a fallback, not a second
source of truth. Poll expensive self-update probes only when needed. Native tray text and
notifications follow the same persisted language preference as the renderer.

## Release checklist

1. Run renderer typecheck/tests/build, IPC binding checks, formatting, clippy and workspace tests.
2. Verify self-hosted build metadata, portable Mach-O dependencies and bundle signatures.
3. Bump all version locations with `scripts/bump-version.sh`, commit a clean tree, and tag that commit.
4. Push the branch, verify CI for the same SHA, then push the version tag.
5. Wait for both native architecture builds, artifact checksums and release publication. Inspect the
   published artifact set and build identity; a tag or a green local test alone is not a release.
6. State whether artifacts are ad-hoc signed or Developer ID signed/notarized. Do not claim unavailable
   external signing credentials or an unsigned checksum file as publisher authentication.
