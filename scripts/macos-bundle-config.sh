#!/usr/bin/env bash
# Keep local and CI signing policy identical; Developer ID builds retain hardened runtime.
set -euo pipefail
node --input-type=module - "$@" <<'JS'
const identity = process.env.APPLE_SIGNING_IDENTITY || '-';
const cli = process.argv[2];
process.stdout.write(JSON.stringify({
  build: { beforeBuildCommand: 'pnpm build:renderer' },
  bundle: { macOS: {
    signingIdentity: identity,
    hardenedRuntime: identity !== '-',
    ...(cli ? { files: { 'MacOS/headway': cli } } : {}),
  } },
}) + '\n');
JS
