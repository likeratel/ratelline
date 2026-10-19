#!/usr/bin/env bash
set -euo pipefail
# Used as tauri-action's tauriScript. The action supplies `build` and its args.
# release.yml passes --config src-tauri/tauri.release.conf.json so both the CLI
# and tauri-action's artifact discovery see createUpdaterArtifacts=true.
: "${TAURI_SIGNING_PRIVATE_KEY:?TAURI_SIGNING_PRIVATE_KEY is required}"
: "${RATELLINE_UPDATER_PUBLIC_KEY:?RATELLINE_UPDATER_PUBLIC_KEY is required}"
: "${RATELLINE_RELEASE_TARGET:?RATELLINE_RELEASE_TARGET is required}"
cargo run --locked --manifest-path scripts/verify-updater/Cargo.toml -- --key-only
pnpm tauri "$@"
cargo run --locked --manifest-path scripts/verify-updater/Cargo.toml -- "src-tauri/target/${RATELLINE_RELEASE_TARGET}/release/bundle"
