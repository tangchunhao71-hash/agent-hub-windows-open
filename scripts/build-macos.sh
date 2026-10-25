#!/bin/bash
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  echo "A macOS build host is required. This script does not cross-compile." >&2
  exit 2
fi
repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo/source"
for tool in node pnpm cargo rustc bun xcrun git; do
  command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool" >&2; exit 2; }
done
git rev-parse --show-toplevel >/dev/null 2>&1 || {
  echo "Initialize a local source baseline as described in README.md before building." >&2
  exit 2
}
case "$(uname -m)" in
  arm64) triple=aarch64-apple-darwin ;;
  x86_64) triple=x86_64-apple-darwin ;;
  *) echo "Unsupported Mac architecture" >&2; exit 2 ;;
esac
xcrun --find clang >/dev/null
if [[ -n "${CARGO_BUILD_TARGET:-}" && "$CARGO_BUILD_TARGET" != "$triple" ]]; then
  echo "Use a native Rust toolchain; remove the cross target." >&2; exit 2
fi
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  echo "Unset CARGO_TARGET_DIR so the app and receipt use source/target." >&2; exit 2
fi
node src/tooling/build.mjs --platform macos --check
receipt=".build/macos-$(date -u +%Y%m%dT%H%M%SZ)-$$"
mkdir -p "$receipt"
# Regenerate the target's notices; do not ship a Windows-only inventory on Mac.
pnpm licenses list --prod --json > "$receipt/npm-licenses.json"
cargo metadata --locked --format-version 1 --filter-platform "$triple" > "$receipt/cargo-metadata.json"
node ../scripts/collect-release-notices.mjs "$receipt/npm-licenses.json" "$receipt/cargo-metadata.json" docs/dependency-notices.txt "$triple"
node --test src/tooling/test/*.test.mjs
node src/tooling/build.mjs --platform macos --bundle --receipt "$receipt/build.json"
cd "$repo"
node scripts/package-macos.mjs "source/$receipt/build.json"
