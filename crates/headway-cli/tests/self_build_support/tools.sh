#!/bin/bash
set -euo pipefail
[ "${HEADWAY_GITHUB_TOKEN+x}" != x ] || exit 90
name="$(basename "$0")"
printf '%s\n' "$name $*" >> "$FIXTURE_LOG"
case "$name" in
xcrun)
  [ "$*" = '--sdk macosx --show-sdk-path' ] || exit 108
  printf '/fixture/MacOSX.sdk\n'
  ;;
otool)
  [ "$1" = '-L' ] || exit 109
  library=/usr/lib/libSystem.B.dylib
  [ "${FOREIGN_DEPENDENCY:-0}" = 0 ] || library=/opt/homebrew/opt/libiconv/lib/libiconv.2.dylib
  printf '%s:\n\t%s (compatibility version 1.0.0, current version 1.0.0)\n' "$2" "$library"
  ;;
rustc)
  [ "$*" = '-vV' ] || exit 91
  printf 'rustc 1.90.0\nbinary: rustc\ncommit-hash: fixture\ncommit-date: 2025-09-18\nhost: aarch64-apple-darwin\nrelease: 1.90.0\nLLVM version: 20.1.8\n'
  ;;
cargo)
  if [ "${CHECK_APP_ENV:-0}" = 1 ]; then
    [ "${LIBRARY_PATH+x}" != x ] || exit 110
    case "$CARGO_ENCODED_RUSTFLAGS" in *-Lnative=/fixture/MacOSX.sdk/usr/lib*) ;; *) exit 111 ;; esac
  fi
  if [ "${CHECK_CLI_ENV:-0}" = 1 ]; then
    [ "$LIBRARY_PATH" = /fixture/inherited ] || exit 112
    [ "${CARGO_ENCODED_RUSTFLAGS+x}" != x ] || exit 113
  fi
  [ "$*" = "build --release --locked -p headway-cli --target $HEADWAY_BUILD_TARGET" ] || exit 92
  case "$CARGO_TARGET_DIR" in /*) ;; *) exit 93 ;; esac
  [ "${FAIL_BUILD:-0}" = 0 ] || exit 42
  release="$CARGO_TARGET_DIR/$HEADWAY_BUILD_TARGET/release"
  mkdir -p "$release"
  repository=null
  [ -z "${HEADWAY_BUILD_REPOSITORY:-}" ] || repository="\"$HEADWAY_BUILD_REPOSITORY\""
  cat > "$release/headway" <<CLI
#!/bin/bash
[ "\$*" = '--json version' ] || exit 94
printf '%s\\n' '{"commit":"$HEADWAY_BUILD_SHA","dirty":$HEADWAY_BUILD_DIRTY,"target":"$HEADWAY_BUILD_TARGET","repository":$repository}'
CLI
  chmod +x "$release/headway"
  ;;
pnpm)
  if [ "$*" = 'install --frozen-lockfile' ]; then exit 0; fi
  [ "$#" = 12 ] || exit 95
  [ "$1 $2 $3 $4 $5 $6 $7 $8 $9" = "exec tauri build --target $HEADWAY_BUILD_TARGET --bundles app --ci --config" ] || exit 96
  [ "${11} ${12}" = '-- --locked' ] || exit 97
  cli="$(node -e 'const fs=require("fs");const c=JSON.parse(fs.readFileSync(process.argv[1]));if(c.build.beforeBuildCommand!=="pnpm build:renderer"||c.bundle.macOS.signingIdentity!=="-"||c.bundle.macOS.hardenedRuntime!==false)process.exit(98);process.stdout.write(c.bundle.macOS.files["MacOS/headway"]);' "${10}")"
  [ "$(tail -n 1 "$cli")" = '# fixture-adhoc-signature' ] || exit 105
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
  if [ "$1" = --display ]; then
    [ "$#" = 3 ] && [ "$2" = --verbose=4 ] || exit 114
    flags='0x2(adhoc)'
    [ "${WRONG_RUNTIME:-0}" = 0 ] || flags='0x10002(adhoc,runtime)'
    printf 'CodeDirectory v=20400 flags=%s\nSignature=adhoc\n' "$flags" >&2
    exit 0
  fi
  case "$1 $2 $3" in
    '--force --sign -')
      [ "$#" = 6 ] && [ "$4 $5" = '--options 0' ] || exit 100
      [ "$6" = "$CARGO_TARGET_DIR/$HEADWAY_BUILD_TARGET/release/headway" ] || exit 106
      [ -x "$6" ] || exit 107
      printf '\n# fixture-adhoc-signature\n' >> "$6"
      ;;
    '--verify --deep --strict')
      [ "$#" = 4 ] || exit 100
      case "$4" in "$PWD"/.headway-out/.build-*/Headway.app) ;; *) exit 102 ;; esac
      [ -x "$4/Contents/MacOS/headway" ] || exit 103
      [ -x "$4/Contents/MacOS/headway-desktop" ] || exit 104
      [ "${FAIL_SIGNATURE:-0}" = 0 ] || exit 43
      ;;
    *) exit 101 ;;
  esac
  ;;
*) exit 99 ;;
esac
