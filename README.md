# Headway

Keep apps and CLI tools built from GitHub current on your Mac: **every commit, or every release**.

Headway tracks GitHub repositories and installs new versions for you: a CLI tool into `~/.local/bin`,
an app bundle into `/Applications`. Each repository is followed one of two ways:

- **Every commit** on a branch: clone once, check out the new commit, run your build command, install
  the result. Step through one commit at a time (`--next`) or jump to the latest.
- **Releases** (or version tags): download the macOS asset for your CPU, verify GitHub's SHA-256
  digest, unpack the `.tar.gz`/`.zip`/`.dmg`, and install it. Release tags can also be built from source.

It comes as a menu-bar app and a CLI that share the same state. The CLI is built to be driven by AI
agents: structured XML/JSON output, stable error codes, self-description, and an agent skill.

![Headway](docs/screenshot.png)

## Quick start (CLI)

```sh
headway token set --from-gh                  # or: printf %s "$TOKEN" | headway token set --stdin
headway add BurntSushi/ripgrep               # inspects the repo: Cargo build, binary `rg`, follows master
headway add zed-industries/zed --track release   # download release assets (.dmg → /Applications)
headway check                                # what's new?
headway update ripgrep --dry-run             # see exactly what would happen
headway update ripgrep --next                # install only the next commit after the installed one
headway update --all                         # bring everything up to date
headway mark ripgrep latest                  # "I already have the latest", without building
headway update ripgrep --to 4a1b2c3          # roll back to any commit or tag
```

`headway --help` lists every command. `headway doctor` checks git, the token, `PATH` and build tools.

## The desktop app

- Menu-bar icon with the number of apps that are behind, and a menu to check, update all, or jump to an app.
- Background checks every hour (configurable), notifications for new commits/releases, and per-app
  automatic updates.
- Per app: installed vs latest, the list of newer commits or releases (install or mark any of them),
  history with one-click reinstall, the live build log, and the config.
- Settings: GitHub token (stored in the macOS Keychain, or taken from `gh`), check interval, install
  locations, launch at login, theme, 中文/English, diagnostics, and an "Install `headway` command" button
  that links the CLI shipped inside the app into your `bin_dir`.
- Closing the window keeps Headway in the menu bar. Changes made with the CLI appear in the app within
  seconds.

## For AI agents

```sh
headway --ai list            # XML (or --json); HEADWAY_OUTPUT=xml makes it the default
headway --ai commands        # every command and flag, as data
```

- stdout carries exactly one document: `ok`, `command`, `data` or `error`, and `next` (suggested
  follow-up commands). Progress and build output go to stderr.
- Errors have a stable `code`, a `hint`, the last output lines of a failed build in `details`, and a
  stable exit code: `2` usage, `3` not found, `4` GitHub/network, `5` build/install failed, `6` busy,
  `7` cancelled, `1` other.
- `--dry-run` on `update`, `add`, `edit` and `remove` shows the plan without doing anything.
- The skill at [`skills/headway/SKILL.md`](skills/headway/SKILL.md) teaches an agent the workflow
  (install it next to your other skills).

## Configuration

`~/Library/Application Support/Headway/config.toml` (see `headway paths`), shared by the CLI and the app:

```toml
[settings]
check_interval_minutes = 60      # background checks in the app; 0 = off
notify = true
bin_dir = "~/.local/bin"         # default destination for CLI tools
apps_dir = "/Applications"       # default destination for .app bundles
gh_fallback = true               # use `gh auth token` when no token is set

[[app]]
id = "ripgrep"
name = "ripgrep"
repo = "BurntSushi/ripgrep"
kind = "cli"                     # or "app"
auto_update = false
track = { mode = "commit" }      # branch = "…" | { mode = "release", prerelease = false, tag_filter = "^v" } | { mode = "tag" }

[app.install]
method = "source"                # or "asset" (release mode): pattern = "*aarch64-apple-darwin.tar.gz", binary = "rg"
build = "cargo build --release --locked"
artifact = "target/release/rg"   # relative to the checkout (or `workdir`); `*` allowed; omit if the build installs itself
# dest = "~/bin/rg"              # default: bin_dir/apps_dir + the artifact's name
```

Build presets (`headway presets`): Cargo, `cargo install`, Go, SwiftPM, Xcode, Tauri, npm, Make.
Build commands run with `/bin/sh -c` in the checkout with `HEADWAY_REF`, `HEADWAY_SHA`,
`HEADWAY_VERSION`, `HEADWAY_BIN_DIR` and `HEADWAY_APPS_DIR` set.

| Variable | Effect |
|---|---|
| `HEADWAY_HOME` | keep config, cache and logs under one directory |
| `HEADWAY_GITHUB_TOKEN` | token for this process (overrides the keychain) |
| `HEADWAY_OUTPUT` | default output format: `human`, `json`, `xml` |
| `HEADWAY_API_URL` | GitHub API base (GitHub Enterprise, tests) |

Token lookup order: `HEADWAY_GITHUB_TOKEN` → Keychain → `GITHUB_TOKEN` → `GH_TOKEN` → `gh auth token`.
Public repositories work without a token (60 API requests/hour); for private ones a fine-grained
token with read-only *Contents* and *Metadata* access is enough.

## Security notes

- Source installs run the repository's build command on your machine. Track repositories you trust.
- The token lives in the login keychain and is read through `/usr/bin/security` (secret on stdin). Git
  gets it through `GIT_CONFIG_*` environment variables, never URLs or arguments. Build commands never
  get it, and `HEADWAY_GITHUB_TOKEN` is stripped from git and build environments; variables you
  export yourself (such as `GITHUB_TOKEN`) are inherited as in your shell.
- Builds run as you, so like any program you run they could read that keychain item through
  `/usr/bin/security`. Use a fine-grained token with read-only access.
- Downloads are not quarantined, so Gatekeeper does not vet installed apps. The SHA-256 digest proves
  the file is what the release published, not that the publisher is trustworthy.
- Release downloads are checked against the SHA-256 digest GitHub publishes for each asset.
- Installs are staged next to the destination and renamed into place, so a failed copy leaves the old
  version working. A plain directory is never replaced: `dest` names the file or the `.app` itself.

## Development

```sh
pnpm install
pnpm tauri dev                 # the app (Vite on :1420 + Rust)
cargo run -p headway-cli -- list
cargo test                     # engine + CLI, including end-to-end runs against real git
cargo test -p headway-desktop  # IPC contract tests (needs `pnpm build:renderer` once)
pnpm test && pnpm typecheck
```

UI preview without the backend: `pnpm dev:renderer`, then open
`http://localhost:1420/?mock` (`&theme=dark`, `&lang=en`).

Releases: `./scripts/bump-version.sh patch|minor|major|beta`, commit, tag `vX.Y.Z`, push; CI builds
the app (`.dmg`, `.app.zip`) and CLI tarballs for Apple Silicon and Intel and publishes a GitHub Release.

## 中文简介

Headway 是一个 macOS 菜单栏应用 + 命令行工具：跟踪 GitHub 仓库，**逐个 commit** 从源码构建，或**跟随版本号**
下载 Release 资源，自动安装到 `~/.local/bin` 或 `/Applications`。GitHub Token 保存在钥匙串里（也可以直接用
`gh` 的登录）。CLI 为 AI Agent 设计：`--ai` 输出 XML、`--json` 输出 JSON、稳定的错误码与退出码、
`headway commands` 自描述，并附带 `skills/headway/SKILL.md`。

## License

MIT
