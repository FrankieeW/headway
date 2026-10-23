---
name: headway
description: Use when keeping apps or CLI tools built from GitHub repositories up to date on this Mac with Headway — tracking a repo, checking for new commits or releases, updating one commit at a time, rolling back, setting the GitHub token, or diagnosing a failed build or download.
---

# Headway

Headway follows GitHub repositories **commit by commit** (build from source) or **release by
release** (download a release asset or build the tag) and installs the result: CLI tools into
`~/.local/bin`, `.app` bundles into `/Applications`. The `headway` CLI and the Headway menu-bar app
share config, state, logs and locks, so whatever you do here shows up in the app.

## Rules

- Always pass `--ai` (XML) or `--json` and read **stdout only**; progress and build output go to
  stderr. Every result is one document: `ok`, `command`, `data` (or `error`) and `next`.
- Branch on `error.code` and the exit code, never on message text. Read `error.hint` and the
  `next` commands: they are the intended follow-ups.
- Run `headway update <id> --dry-run --ai` before the first real update of an app and tell the user
  what will be built or downloaded and where it will be installed.
- Source builds execute code from the repository. Only add repositories the user asked for.
- Use `--next` to install just the commit/release after the installed one, `--to <sha|tag>` for a
  specific version or a rollback. Plain `update` installs the latest.
- If the tool is already installed by other means, record a baseline with `headway mark <id> latest`
  instead of rebuilding it.
- Never put a token on the command line. Pipe it: `printf %s "$TOKEN" | headway token set --stdin`,
  or reuse the GitHub CLI login with `headway token set --from-gh`.
- Change apps with `headway edit`, not by editing `state.json`. `config.toml` may be hand-edited
  (`headway paths`), but `edit` validates.
- `remove` never deletes the installed program; `--purge` only deletes Headway's cached checkout and logs.

## Workflow

1. Look around:

```sh
headway --ai list
headway --ai doctor          # git, token, PATH, build tools per app
headway --ai token status
```

2. Track a repository. `add` inspects it (build system, binary names, macOS release assets) and
   picks commit + source build, or release + asset download; flags override the suggestion:

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
| `cancelled` | 7 | interrupted |
| `keychain_error`, `io_error` | 1 | environment problem; see the message |

`check` and multi-app `update` report per-app results in `data` and exit with the first failure's
code. `doctor` exits 1 when a check fails.

## Statuses

`list`/`show` give each app one `status`: `up_to_date`, `update_available` (`pending_count` newer
commits/releases), `not_installed` (nothing recorded yet), `diverged` (installed commit no longer on
the branch, e.g. after a force-push), `error` (see `last_error`), `busy`, `unchecked`.

## Environment

- `HEADWAY_OUTPUT=xml|json` sets the default output format.
- `HEADWAY_GITHUB_TOKEN` overrides the keychain token for one process.
- `HEADWAY_HOME=<dir>` keeps config, cache and logs under one directory (sandboxes, tests).
- Build commands run with `/bin/sh -c` in the checkout and get `HEADWAY_REF`, `HEADWAY_SHA`,
  `HEADWAY_VERSION`, `HEADWAY_BIN_DIR`, `HEADWAY_APPS_DIR`. The GitHub token is never passed to them.
