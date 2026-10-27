---
name: headway
description: Use when keeping apps or CLI tools from GitHub, npm, Homebrew or official updater scripts up to date on this Mac with Headway — tracking a repo, checking for new commits or releases, updating one commit at a time, rolling back, setting the GitHub token, or diagnosing a failed build or download.
---

# Headway

Headway follows a GitHub repository as one subscription. What to follow (every commit, releases, or
tags) is separate from how a revision is installed: build it from source, or download its release
asset. Both recipes can be saved. CLI tools go into `~/.local/bin` and `.app` bundles into
`/Applications`. The `headway` CLI and the Headway menu-bar app share config, state, logs and locks,
so whatever you do here shows up in the app.

## Rules

- Always pass `--ai` (XML) or `--json` and read **stdout only**; progress and build output go to
  stderr. Results contain `ok`, `command`, `data` (or `error`) and `next`. `ok` agrees with the exit
  status. Nested commands also expose a `subcommand` path without renaming the existing `command`.
- Branch on the exit code and `error.code`, never message text. Batch failures retain their data:
  inspect `data.results[].ok` and `data.results[].error` even when top-level `ok` is false.
  Auto-update failures can include completed `outcomes`; do not say no update occurred without
  reading them. `error.hint` and `next` are advisory, not authorization or installation evidence.
- Help/version keep conventional text output. A consumer closing stdout early exits quietly;
  signals on read-only commands can terminate before a document is written. `--ai` takes precedence
  over `--json`, which takes precedence over `--output` and then `HEADWAY_OUTPUT`.
- Run `headway update <id> --dry-run --ai` before the first real update of an app and tell the user
  what will be built or downloaded and where it will be installed. A dry run validates without saving
  subscriptions or installed/check state, but can run read-only network/version probes; it is not a
  reservation against concurrent file, permission or upstream-ref changes.
- Source builds execute code from the repository. Only add repositories the user asked for.
- GitHub and npm support `--next` and `--to <ref-or-version>` (including rollback). Homebrew
  and script sources only support the latest version. Plain `update` installs the latest.
- For GitHub tools already installed by other means, record a baseline only when the installed
  version/ref is known. Finding an artifact alone does not prove it is the latest remote commit.
- Never put a token on the command line. Pipe it: `printf %s "$TOKEN" | headway token set --stdin`,
  or reuse the GitHub CLI login with `headway token set --from-gh`.
- Change apps with `headway edit`, not by editing `state.json`. `config.toml` may be hand-edited
  (`headway paths`), but `edit` validates.
- `--dest` is the full path of the installed file or `.app` (`~/.local/bin/rg`), never a directory.
- `remove` never deletes the installed program; `--purge` deletes its cached checkout, logs and backups.

## Workflow

1. Look around:

```sh
headway --ai list
headway --ai doctor          # git, token, PATH, build tools per app
headway --ai token status
```

2. Track a repository. `add` inspects it (build system, binary names, macOS release assets) and
   picks commit + source build, or release + asset download; flags override the suggestion.
   Without `--method`, `add` builds when given `--build`, `--preset`, `--workdir` or `--artifact`:

```sh
headway --ai inspect owner/repo
headway --ai add owner/repo --dry-run
headway --ai add owner/repo                                   # accept the suggestion
headway --ai add owner/repo --track release    # download release assets instead of building
headway --ai add owner/repo --preset cargo --binary rg        # see `headway --ai presets`
headway --ai add owner/repo --build 'make release' --artifact build/tool --dest ~/.local/bin/tool
```

3. Check and update:

```sh
headway --ai check                    # all apps; or `check <id> ...`
headway --ai update <id> --dry-run
headway --ai update <id>              # latest
headway --ai update <id> --next       # one step: the commit/release right after the installed one
headway --ai update --all             # everything that is behind
headway --ai update <id> --method asset  # install that revision from its release asset
```

4. When something fails:

```sh
headway --ai show <id>                # status, last_error, pending, recent history
headway log <id> --tail 80            # plain text log of the last run
headway --ai edit <id> --build '<fixed command>'
headway --ai history <id>
headway --ai update <id> --to <sha-or-tag>   # roll back to a known-good version
```

Discover every flag with `headway --ai commands` (the whole command tree as data).

## Headway self-hosting

For a branch that includes `scripts/build-headway.sh`, track the app with `headway --ai add
FrankieeW/headway --preset headway-app --branch main --dest ~/Applications/Headway.app`, inspect
`update headway --dry-run`, then install the first supported commit with `update headway`.
Future `update headway --next` builds one commit at a time. The app carries its matching CLI;
the app-managed CLI symlink follows it. `headway --ai self status` compares running and installed
builds. `self restart` queues a one-shot restart until the desktop updater is idle; the next startup
confirms the commit. Inspect `restart_error` on failure and request again to retry.

For a standalone CLI, use `add FrankieeW/headway --id headway-cli --preset headway-cli --branch
main`, then the same dry-run/update/--next workflow. `headway --ai version` exposes compile-time
`version`, full `commit`, `dirty`, and `target`; `--version` and the app's version label show a short
revision. Only use a known clean installed commit as a `mark` baseline. Older commits without the
helper require their historical recipe. Presets use stable `.headway-out` artifacts and preserve
build caches; source installs still go through Headway's staged replacement.
New build metadata also includes optional `repository`. Self adoption requires this origin to
match the tracked repository. Legacy builds without it keep only a matching existing baseline;
their clean SHA alone cannot establish a new repository baseline. Restart confirmation accepts
legacy metadata without inventing an origin.

`headway --ai self subscribe` registers or moves Headway to the first subscription. New entries
only notify. It preserves existing user settings and release/tag baselines, and adopts a clean
commit stamp only for the exact managed bundle under its update lock. `headway --ai cli status`
reports the bundled link and PATH shadowing; `cli fix --app /Applications/Headway/Headway.app`
repairs Headway-owned links after a move, refusing foreign files and Brew-owned links.

## Build candidates and commit evidence

`inspect` returns `build_candidates` from README instructions, source manifests and presets:
`id`, `source`, `file`, optional `line`, `build`, `workdir`, `artifact`, `kind`, `reason`, `recommended`.
Inspection never executes those commands. Review a candidate, then override it using `add`/`edit`
with `--build`, `--workdir`, `--artifact` and `--dest`. The UI's Apply action is explicit; fields
remain editable after detection.
For a recognized standard Tauri 2 project, the local app candidate skips generating signed updater
artifacts and defaults to ad-hoc app signing unless `APPLE_SIGNING_IDENTITY` is set in Headway's
environment; a signing identity in the project's Tauri config is not used.
`--preset tauri-local` provides the pnpm version explicitly. Existing recipes are not rewritten;
inspect and apply a candidate or edit the recipe. Changing app signing identity can affect macOS
keychain access; ad-hoc signing does not provide Developer ID signing or notarization.

`headway --ai commit-info <id> <sha> [--refresh]` returns `version`, `version_source` (`tag` or
`manifest`), tags, nullable `is_release`, release/CI URLs and notes. A manifest is read at that exact
SHA, and its version does not imply a Release. `ci_state` is `success`, `failure`, `pending`,
`not_run` or `unknown`; API failure must not be described as a passing or absent check.

## Homebrew ⇄ GitHub matching

```sh
headway --ai match brew:ripgrep
headway --ai match cask:zed
headway --ai match BurntSushi/ripgrep
headway --ai match <app-id> --offline
```

Matching only suggests alternatives: it never installs, tracks, saves matches or switches a source.
`--offline` skips GitHub repository details/search and token resolution; it still queries local
Homebrew metadata and package names. Formulae.brew.sh URLs and tracked GitHub/Homebrew app IDs work
too. npm and script sources are unsupported.

Every suggestion includes ordered, deduplicated `evidence` and derived `confidence`: `high` means
Homebrew's own source, HEAD, download or repository homepage URL names the repository; `medium`
means normalized homepage equality with GitHub metadata; `low` means name/search hints only.
GitHub Pages and documentation URLs do not prove repository identity. Confirm low-confidence
suggestions with the user before switching sources. `tracked_by` lists existing app IDs, across
all GitHub tracking modes or the same Homebrew package and Formula/Cask type.

Results contain `query`, `from`, `repositories`, `packages`, `notes` and `errors`. Partial provider
failures retain useful data with `ok: false` and exit 1; unreadable Homebrew input is a fatal error.
`discover` also includes `repositories` from installed Homebrew metadata without GitHub requests,
and reports a conflict for each repository already tracked by an app.

## Discovery and automatic policy

```sh
headway --ai discover --source brew,npm
headway --ai import <key-from-discover> --dry-run
headway --ai import <key-from-discover>
headway --ai recipes
headway --ai import recipe:headway-app --dry-run
headway --ai edit <id> --auto-update true --update-mode next --max-update-steps 1 --pause-on-failure true
headway --ai auto-update <id>
headway --ai resume-auto-update <id>
```

Discovery is read-only and reports partial provider errors. Import validates all selected entries
before writing and disables auto-update. Homebrew candidates distinguish `cask: true` from Formulae;
optional `install_path` is read-only metadata from an unambiguous absolute Homebrew artifact target,
never a guessed destination. Missing or ambiguous locations do not block import. Changing a saved
package name or Cask/Formula type clears this metadata. The desktop import panel filters Cask,
Formula and npm entries and searches names, package identifiers and reported paths locally;
selection persists across filters, including entries outside the visible results.
Recipe entries can be edited after import. Saved
`update_policy` has `mode: notify|next|latest`, `max_steps: 1..20`, `pause_on_failure: bool`.
Notify never installs; next applies a bounded sequence; latest installs once. Paused apps remain
paused until resumed. `auto-update [ids...]` runs one saved-policy cycle (empty means all);
manual `update --all` remains an explicit latest update. Policy/source changes are checked again
between steps. A verification/update failure may set `auto_update_paused`. If a later step fails,
completed `outcomes` remain in the result; report both the successful steps and the error.
`check` and desktop `check_all` share `results`, `checked` and `with_updates`; `failed` is an additive
compatibility projection. Per-item errors include the same `code`, `exit_code`, `hint` and `details`
as ordinary errors.

## Verification and restore

`edit <id> --verify-command '<command>'` configures post-install verification for either GitHub
install method; an empty string clears it. The command receives `HEADWAY_INSTALLED_PATH`,
`HEADWAY_REF`, `HEADWAY_SHA`, `HEADWAY_VERSION`. It is not an installation-detection probe.
Managed GitHub payloads retain the last two prior artifacts; `headway --ai backups <id>` lists
them and `headway --ai restore <id> <backup-id>` restores without building. Restoration holds the
app lock, validates paths/digests and repository/track identity, and records history. Foreign
source backups are hidden from listing and cannot be restored; malformed or legacy unbound
manifests remain explicit errors. Purge removes them. Package-manager/script installs and
self-installing builds do not use this cache. A failed verifier rolls back the payload/ref; an
emergency rollback failure reports the rescue path and clears installed certainty. Warnings in
plans and update `warnings` must be relayed: an asset without GitHub's SHA-256 digest is not described
as verified. A digest or native code-signature integrity check does not authenticate the publisher.
Other apps can opt into a `verify_command`; unsigned source builds are not universally rejected.
Interrupted managed replacements retain recovery evidence; do not delete transaction files or
active locks to force a retry.

## Source retention, paginated commits and alternate repositories

`edit <id> --keep-source true|false --source-root <base>` controls source builds. Default keep=true
uses the existing cache checkout. A custom base contains the app ID's managed child; keep=false
uses a unique temporary checkout under the base and cleans up on success/failure/cancellation.
Changing roots or disabling keep preserves old retained paths. `--source-root ''` resets the base.
`headway --ai source-storage <id>` reports `path`, `keep`, `exists`, `custom_root`; the path is the
configured retained location, while disabled retention builds use a temporary child instead.
Only Headway-owned checkout directories can be reused/purged; the base and siblings remain.

`headway --ai commits <id> --page 1 --per-page 20` returns a `CommitPage` with `repo`, `branch`,
immutable `anchor`, `page`, `per_page`, `commits`, `has_more`, `next_page`, `filter` and
`filtered_out`. Later pages require `--anchor <returned-sha>`. Read only requested pages; restart at
page 1 without an anchor to refresh.

Every commit of a page names the release it published: `commits[].release_tag`, `release_url`,
`tags` and `can_download` (the release has an asset for this Mac). Narrow the page with
`--release` (only commits with a published release), `--tag '<regex>'` (only commits carrying a
matching tag) and `--ci success|failure|pending|not-run|unknown` (only commits with that CI state).
The filters combine, `filtered_out` counts the commits they removed, and the `next` hint repeats
them so the following page keeps the same narrowing. `--ci` reads the check runs of every commit of
the page that survives the release/tag filters; the other filters cost one repository scan.
The scan covers the newest 100 tags/releases and is cached for five minutes. `notes` reports
incomplete or unavailable metadata; missing annotations in that case do not prove a commit has
no release. CI lookup warnings identify their commit even when the filter removes that row.
A filtered page can be empty while `next_page` still points to more history.

`headway --ai repo list <id>` lists current and saved origins. `repo add <id> <owner/repo>
--label 'My fork' --branch work [--git-url ...]` saves an alternative. `repo use <id> <owner/repo>`
explicitly activates a configured origin and forgets foreign revision certainty while retaining
the actual installed payload. `repo remove <id> <owner/repo>` removes an inactive choice. Do not
report the old source's SHA as the newly selected repository's installed revision.

Repository inspection (`inspect`, or `add` without `--no-inspect`) reads GitHub's fork,
parent and root source metadata and includes those origins in the suggested config.
`repo refresh <id>` adds parent/root choices to an existing subscription without activating
them or replacing saved custom choices. Switching saves each origin's branch/tracking mode
and build/download recipes, preserves the current destination, and turns off auto-update.

`headway --ai installation <id>` reads filesystem evidence without executing the payload.
`presence` is `present`, `missing`, `invalid` (wrong type/permissions), or `unknown` (no literal
path evidence). `baseline_matches` indicates whether a recorded revision applies at that path;
`recorded` retains the previous origin after a repository switch and is not a new baseline.
This is a presence check, not a checksum or version probe. It never invents a revision from
the upstream head. `show` and the desktop expose the same `installation` evidence.

`headway --ai update <id> --method configured|source|asset` decides how that one run installs.
`configured` (the default) uses the saved install method, `source` builds the selected commit and
`asset` downloads the release that published it. The method never rewrites the saved recipes, so a
commit-tracked app can be updated from its release without switching what it follows. With
`--method asset` the run fails when the resolved commit has no published release or no installable
asset; it never falls back to a build. `--dry-run` shows the same plan.

`edit` can store both GitHub install recipes at once. `--build`, `--preset`, `--workdir` and
`--artifact` update the source recipe. `--pattern` and `--binary` update the release recipe.
Recipe edits keep the current default; only `--method source|asset` changes which one ordinary
updates use. A preset sets the app kind only when the source build is (or becomes) the default.
`--workdir`, `--artifact`, `--keep-source` and `--source-root` need a source recipe with a build
command and are rejected otherwise. With an asset default, `edit <id> --build ''` removes the
optional source recipe; the active source recipe still requires a non-empty build command.
`install` holds the active recipe, with only the opposite method retained in `source_build` or
`release_asset`; saving does not retain a duplicate of the active recipe. One-off
`update --method` does not rewrite either recipe.

## Exact revisions and independent installations

PR builds use the same revision engine:

```sh
headway --ai prs <id> --page 1 --per-page 20
headway --ai commit-options <id> pr:123
headway --ai install-pr <id> 123@<head-sha> --as <test-id> --dry-run
headway --ai install-pr <id> 123@<head-sha> --as <test-id>
```

`prs` lists open PRs; resolving an explicit number also supports closed PRs and deleted forks.
Use the `reference` returned by `commit-options` (`pr:<number>@<40-character-sha>`) for reviewed
operations; `prs` returns the head SHA as `pull_requests[].head.sha`, not a `reference` field.
The PR head is rechecked before installation; a moved/force-pushed head is rejected without
installing a different revision. PR versions require a source recipe and never use ordinary
release assets. `--as` keeps the parent payload; omit it for an in-place install. Successful
installs expose `pull_request` in `show` and history records the PR. Automatic updates are
disabled, and independent installs remain pinned. Application data still remains shared.

```sh
headway --ai commit-options <id> <sha-or-tag>
headway --ai install-commit <id> <sha-or-tag> --method source --dry-run
headway --ai install-commit <id> <tag> --method asset --dry-run
headway --ai install-commit <id> <sha> --as <new-id> --name 'Independent version' --dry-run
headway --ai install-commit <id> <sha> --as <new-id> --name 'Independent version'
headway --ai launch-variant <new-id>
```

The exact-revision action supports upgrade, downgrade and reinstall. `--method configured|source|asset`
chooses a one-off recipe. Source overrides: `--build`, `--artifact`, `--workdir`, `--verify-command`;
asset overrides: `--pattern`, `--binary`, `--verify-command`. A source build needs a valid build
recipe; a download needs a real published release whose tag resolves to that commit. Unknown
release evidence must not be treated as a confirmed release. Main subscription recipes are preserved.
When a prior lookup resolved a release tag to a SHA, pass `--release-tag <tag>` together with
that SHA. The tag is rechecked through its full tag ref before planning/installing, including
source builds, so older releases remain selectable without guessing from a bounded badge scan.
Moved or mismatched tag associations are rejected before installing.

`--as` registers a new ID with a separate destination and disabled auto-update, pinned to the
resolved commit. Apps use `apps_dir/Headway Versions/<id>.app`; CLI binaries use `bin_dir/<id>`.
Always review the dry-run's destination. Existing paths are refused, and source commands without
a managed artifact are ineligible because they cannot guarantee a separate install location.
Failed child installs remain registered for diagnosis and explicit retry. App children can be
launched with `launch-variant`; CLI children run from their installed path/name. Data remains
shared using the app's own normal behavior: no profile or data-dir isolation is provided.
Independent Headway startup preserves the main subscription and CLI entry. A Headway fork at
the managed bundle retains its own installer baseline; a clean SHA alone is not official-origin proof.

## Optional update source examples

GitHub remains the default for `owner/repo` and GitHub URLs. Other sources are opt-in:

```sh
headway --ai inspect npm:@openai/codex
headway --ai add npm:@openai/codex --dist-tag latest
headway --ai add https://www.npmjs.com/package/pnpm
headway --ai add brew:ripgrep
headway --ai add cask:zed
headway --ai add https://formulae.brew.sh/formula/jq
headway --ai add script:claude --update-command 'claude update' --version-command 'claude --version'
headway --ai edit codex --dist-tag beta
headway --ai edit zed --cask true
```

- `add --no-inspect` skips lookup for every source. GitHub still needs a build command/preset or
  release-asset setup; scripts require `--update-command`. npm defaults to `latest`; `brew:` defaults
  to a formula, `cask:` to a cask. `--cask` can also select a cask for a `brew:` input.
- npm uses the user's `npm` registry configuration and global installs. Homebrew uses the user's
  `brew` executable. Both read the system's installed version, including changes outside Headway.
- `--package` edits an npm/Homebrew package; `--dist-tag` is npm-only and `--cask` is Homebrew-only.
- Scripts run the explicitly supplied official updater with `/bin/sh -c`. Choose commands from the
  project's official instructions and only run what the user authorized. Headway does not discover
  or certify arbitrary scripts. `--version-command` reads the installed version;
  `--latest-command` accepts a command or HTTP(S) URL returning a version. Empty strings clear these
  two optional probes. Without enough version information, status is `unknown`: explicitly run
  `update <id> --dry-run`, then `update <id>`; `update --all` only selects known available updates.
- `mark` is GitHub-only. Build/asset/track flags are also GitHub-only; irrelevant flags return an
  error rather than being ignored. Homebrew and scripts reject `--next` and `--to`.
- Legacy GitHub config without `source` remains readable. Serialized app records now include
  `source: github|npm|brew|script` with source-specific fields flattened into the record.

## Error codes


| code | exit | meaning / what to do |
|---|---|---|
| `app_not_found` | 3 | unknown id: `headway --ai list` |
| `app_exists` | 2 | pick another `--id` or use `edit` |
| `invalid_input`, `usage_error` | 2 | fix the arguments; see `headway --ai commands` |
| `invalid_config`, `parse_error` | 1 | fix the app with `edit` (or the file from `headway paths`) |
| `auth_failed` | 4 | token rejected: `headway token set` |
| `rate_limited` | 4 | set a token (5000 requests/h instead of 60) or wait |
| `remote_not_found` | 3 | wrong repo/branch/ref, or a private repo without a token |
| `network_error`, `github_error` | 4 | retry later |
| `no_asset` | 3 | no matching release asset: `edit <id> --pattern '<glob>'` or `--method source` |
| `command_failed` | 5 | the build or install failed; `error.details` has the last output lines |
| `checksum_mismatch` | 5 | download corrupted or tampered with; do not install, retry |
| `busy` | 6 | another update of this app is running (CLI or app); wait |
| `store_busy` | 6 | another configuration write has not released the store lock; retry |
| `config_changed` | 1 | configuration changed during an observation; re-run with current config |
| `recovery_pending` | 1 | an interrupted install needs reconciliation; check the app before editing/reusing its id |
| `cancelled` | 7 | interrupted |
| `keychain_error`, `io_error` | 1 | environment problem; see the message |

`check`, multi-app `update` and `auto-update` report per-app results in `data` and exit with the
first failure's code. `doctor` exits 1 when a diagnostic fails; `discover` and `match` exit 1 on provider errors.
`ok` is false in these cases even though the successful per-item data is retained.

State schema versions are additive; unsupported future versions are rejected unchanged. Before a
state mutation, the previous valid state is saved as `state.json.bak` beside the state file. A corrupt
state is never silently reset: inspect `doctor` and the error, preserve the original, and restore the
known-good backup only with the user's authorization. Deleting state discards installed provenance.

## Statuses

`list`/`show` give each app one `status`: `up_to_date`, `update_available` (`pending_count` newer
commits/releases), `not_installed` (no detected managed artifact/baseline), `diverged` (installed commit no longer on
the branch, e.g. after a force-push), `error` (see `last_error`), `busy`, `unchecked`, or
`unknown` (insufficient version evidence). In particular, an existing GitHub artifact without a
recorded baseline is installed but unconfirmed. Never translate that into “not installed” or
invent a SHA. If the recorded managed artifact disappears, update can reinstall it.

## Environment

- `HEADWAY_OUTPUT=human|xml|json` sets the default output format; an empty value is treated as unset.
- `HEADWAY_GITHUB_TOKEN` overrides the keychain token for one process.
- `HEADWAY_HOME=<dir>` keeps config, cache and logs under one directory (sandboxes, tests).
- `HEADWAY_API_URL` changes the authenticated API destination (GitHub Enterprise or loopback tests).
  Only use a trusted HTTPS endpoint; plaintext HTTP is for loopback test servers, not arbitrary hosts.
- Build commands run with `/bin/sh -c` in the checkout and get `HEADWAY_REF`, `HEADWAY_SHA`,
  `HEADWAY_VERSION`, `HEADWAY_BIN_DIR`, `HEADWAY_APPS_DIR`. `HEADWAY_GITHUB_TOKEN`, `GITHUB_TOKEN` and
  `GH_TOKEN` are stripped. Builds still run as the user and can access the login keychain: only trust
  authorized publishers and prefer read-only fine-grained tokens.
- `headway settings get`/`settings set` inspect and edit shared settings (`headway --ai commands`
  describes the exact subcommands). `language` is `system`, `en` or `zh` for native desktop text.
  `completions` generates shell completion; `--limit`, `--tail`, `--quiet`, `--verbose`, `--no-color`
  and `--output` are described in the command tree. `log` is plain text only in human mode.
