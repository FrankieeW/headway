# Headway — guide for agents working on this repo

Headway keeps apps and CLI tools built from GitHub current on macOS, following each repository
commit by commit or release by release. One engine, two front ends: the `headway` CLI and a Tauri
menu-bar app. To *use* Headway as an agent, read `skills/headway/SKILL.md` instead.

## Layout

```
crates/headway-core   engine: config/state store, GitHub client, checks, source builds, asset installs
crates/headway-cli    `headway` binary: clap args, output (human / --json / --ai XML), e2e tests
src-tauri             desktop app: commands (thin wrappers), tray, scheduler, file watcher
src                   React 18 + TS + Vite + Tailwind 3 + shadcn/Radix UI (cc-switch-desktop design system)
skills/headway        agent skill for operating the CLI
```

`headway_core::Headway` (`crates/headway-core/src/service/mod.rs`) is the only API both front ends call.

## Commands

```sh
cargo test                                   # core + CLI (default members), incl. e2e with real git
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
pnpm install && pnpm build:renderer          # needed before building/testing src-tauri
cargo test -p headway-desktop                # IPC contract tests (mock Tauri runtime)
pnpm typecheck && pnpm test                  # frontend
pnpm tauri dev                               # run the app
pnpm dev:renderer  →  http://localhost:1420/?mock   # UI preview with fixture data (&theme=dark&lang=en)
```

Use `HEADWAY_HOME=$(mktemp -d)` when running the CLI or app by hand so the real config is untouched.

## Self-hosting builds

`./scripts/build-headway.sh app|cli` builds the exact checkout with locked dependencies and
commit/dirty/target metadata. It stages `.headway-out/Headway.app` (including its matching CLI)
or `.headway-out/headway`, preserving build caches and leaving installation to Headway. The app
recipe ad-hoc signs and verifies the staged bundle. Use the `headway-app` / `headway-cli` presets
for commit tracking; see `examples/self-update.toml`. Only a known clean installed commit is a
valid baseline for `mark`; dirty local builds must not be presented as clean remote revisions.

## Rules

- **Parity.** A new capability goes into `Headway` first, then gets: a CLI command (with `--json`/`--ai`
  output and `next` hints), a Tauri command, an entry in `src/lib/api/index.ts` + types in
  `src/lib/api/types.ts`, a case in `src-tauri/src/ipc_tests.rs`, and a line in `SKILL.md`.
- **Stable machine contract.** `Error::code()` strings and `Error::exit_code()` values are public API
  (documented in `SKILL.md`); add codes, never rename them. JSON field names are snake_case and shared
  by the CLI and the UI.
- JSON/XML stdout carries one result document; progress and logs go to stderr. `ok` must agree with
  the exit code, including partial failures. Handle broken pipes without panicking. Help/version
  retain conventional text output; signals may end read-only commands before any result.
- Lints are strict (`unwrap_used`, `expect_used`, `panic` denied outside tests; clippy pedantic).
- Prefer returning new values over mutating shared state (`State::with_app`, `Settings::with_key`).
- Never pass the GitHub token to build commands or put it in argv/URLs (git gets it via
  `GIT_CONFIG_*` env vars, the keychain via `security -i` on stdin). `Cmd::new` strips
  `HEADWAY_GITHUB_TOKEN`, `GITHUB_TOKEN` and `GH_TOKEN` from everything run through `process::Cmd`
  (git, builds, archive tools); run new subprocesses through it. OS adapters that cannot use the
  async runner must apply the same credential removal and bound non-interactive work explicitly;
  OS-owned keychain approval prompts are interactive, not build/version probes.
- `install::install` only ever replaces a bundle with a bundle; never relax that guard.
- UI strings go through i18next with keys in both `en.json` and `zh.json`; untrusted text (commit
  messages, release notes, logs) is rendered as text, never HTML. No `console.log`.
- Files stay under ~400 lines; split by feature.
- Commit with conventional commits (`feat(cli): …`, `fix(core): …`). Release with
  `./scripts/bump-version.sh patch|minor|major|beta`, then push the `vX.Y.Z` tag.

## Companion repo

`../devtools/agent-skills` (FrankieeW/agent-skills) can carry a copy of `skills/headway/SKILL.md`;
keep them in sync when the CLI surface changes.
