#!/usr/bin/env bash
# Source only for portable app/release builds. Cargo tracks this SDK search flag,
# so an artifact linked previously against LIBRARY_PATH is not silently reused.
unset LIBRARY_PATH
headway_macos_sdk="$(xcrun --sdk macosx --show-sdk-path)"
CARGO_ENCODED_RUSTFLAGS="$(node --input-type=module - "$headway_macos_sdk" <<'JS'
const flags = process.env.CARGO_ENCODED_RUSTFLAGS !== undefined
  ? process.env.CARGO_ENCODED_RUSTFLAGS.split('\x1f').filter(Boolean)
  : (process.env.RUSTFLAGS || '').trim().split(/\s+/).filter(Boolean);
const sdk = `-Lnative=${process.argv[2]}/usr/lib`;
if (!flags.includes(sdk)) flags.push(sdk);
process.stdout.write(flags.join('\x1f'));
JS
)"
export CARGO_ENCODED_RUSTFLAGS
unset headway_macos_sdk
