#!/usr/bin/env bash
set -euo pipefail

: "${RATELLINE_RELEASE_PLATFORM:?RATELLINE_RELEASE_PLATFORM is required}"
: "${RATELLINE_RELEASE_TARGET:?RATELLINE_RELEASE_TARGET is required}"
: "${TAURI_SIGNING_PRIVATE_KEY:?TAURI_SIGNING_PRIVATE_KEY is required}"
: "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:?TAURI_SIGNING_PRIVATE_KEY_PASSWORD is required}"
: "${RATELLINE_UPDATER_PUBLIC_KEY:?RATELLINE_UPDATER_PUBLIC_KEY is required}"

cargo run --locked --manifest-path scripts/verify-updater/Cargo.toml -- --key-only
pnpm tauri "$@"

bundle_directory="src-tauri/target/${RATELLINE_RELEASE_TARGET}/release/bundle"
cargo run --locked --manifest-path scripts/verify-updater/Cargo.toml -- "$bundle_directory"
if [[ "$RATELLINE_RELEASE_PLATFORM" == "macos" ]]; then
  node scripts/release-notarize-macos.mjs --bundle-dir "$bundle_directory"
fi
node scripts/release-artifacts.mjs \
  --platform "$RATELLINE_RELEASE_PLATFORM" \
  --bundle-dir "$bundle_directory"
