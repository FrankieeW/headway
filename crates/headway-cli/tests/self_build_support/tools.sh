#!/bin/bash
set -euo pipefail
[ "${HEADWAY_GITHUB_TOKEN+x}" != x ] || exit 90
name="$(basename "$0")"
printf '%s\n' "$name $*" >> "$FIXTURE_LOG"
case "$name" in
rustc)
  [ "$*" = '-vV' ] || exit 91
  printf 'rustc 1.90.0\nbinary: rustc\ncommit-hash: fixture\ncommit-date: 2025-09-18\nhost: aarch64-apple-darwin\nrelease: 1.90.0\nLLVM version: 20.1.8\n'
  ;;
cargo)
  [ "$*" = "build --release --locked -p headway-cli --target $HEADWAY_BUILD_TARGET" ] || exit 92
  case "$CARGO_TARGET_DIR" in /*) ;; *) exit 93 ;; esac
  [ "${FAIL_BUILD:-0}" = 0 ] || exit 42
  release="$CARGO_TARGET_DIR/$HEADWAY_BUILD_TARGET/release"
  mkdir -p "$release"
  cat > "$release/headway" <<CLI
#!/bin/bash
[ "\$*" = '--json version' ] || exit 94
printf '%s\\n' '{"commit":"$HEADWAY_BUILD_SHA","dirty":$HEADWAY_BUILD_DIRTY,"target":"$HEADWAY_BUILD_TARGET"}'
CLI
  chmod +x "$release/headway"
  ;;
pnpm)
  if [ "$*" = 'install --frozen-lockfile' ]; then exit 0; fi
  [ "$#" = 12 ] || exit 95
  [ "$1 $2 $3 $4 $5 $6 $7 $8 $9" = "exec tauri build --target $HEADWAY_BUILD_TARGET --bundles app --ci --config" ] || exit 96
  [ "${11} ${12}" = '-- --locked' ] || exit 97
  cli="$(node -e 'const fs=require("fs");const c=JSON.parse(fs.readFileSync(process.argv[1]));if(c.build.beforeBuildCommand!=="pnpm build:renderer"||c.bundle.macOS.signingIdentity!=="-")process.exit(98);process.stdout.write(c.bundle.macOS.files["MacOS/headway"]);' "${10}")"
  bundle="$CARGO_TARGET_DIR/$HEADWAY_BUILD_TARGET/release/bundle/macos/Headway.app/Contents/MacOS"
  mkdir -p "$bundle"
  cp "$cli" "$bundle/headway"
  # Signing may change bytes without changing version output.
  printf '\n# signed fixture\n' >> "$bundle/headway"
  if [ "${MISMATCH:-0}" = 1 ]; then
    printf '#!/bin/bash\nprintf '\''{}\\n'\''\n' > "$bundle/headway"
  fi
  printf '#!/bin/bash\nexit 0\n' > "$bundle/headway-desktop"
  chmod +x "$bundle/headway" "$bundle/headway-desktop"
  ;;
codesign)
  [ "$#" = 4 ] || exit 100
  [ "$1 $2 $3" = '--verify --deep --strict' ] || exit 101
  case "$4" in "$PWD"/.headway-out/.build-*/Headway.app) ;; *) exit 102 ;; esac
  [ -x "$4/Contents/MacOS/headway" ] || exit 103
  [ -x "$4/Contents/MacOS/headway-desktop" ] || exit 104
  [ "${FAIL_SIGNATURE:-0}" = 0 ] || exit 43
  ;;
*) exit 99 ;;
esac
