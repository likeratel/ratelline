# Desktop release engineering

## Distribution status

There is no public production update service yet. Local builds and direct file handoff work without a
GitHub release, and the application does not attach repository credentials to updater requests, so a
release endpoint cannot serve updates to users until a reachable update service has been designed and
implemented. Do not publish a workflow draft or describe it as a production update channel before
that exists.

The **Build signed validation draft** workflow is an internal packaging gate. It accepts an explicit
`macos`, `windows`, or `macos-and-windows` scope and creates a private draft only after the selected
jobs build and pass platform trust checks. A green run proves the artifacts from that run passed the
checks below; it does not make an updater endpoint usable. Publishing, distribution, and changing
repository visibility remain manual decisions outside the workflow.

Ordinary `pnpm desktop:build` remains a local developer build. It uses
`createUpdaterArtifacts: false` and needs none of the release secrets.

## Signing evidence to date

The public **v0.1.0** release on GitHub Releases (2026-10-11) is `crowline_0.1.0_universal.dmg` (published under the former
name crowline), a universal (Apple Silicon and Intel) build from source commit `41559d7` signed with Developer ID and
hardened runtime. The app bundle and the disk image were each notarized and stapled, and both passed
the repository's `scripts/release-artifacts.mjs` checks: strict signature verification, secure
timestamp, staple validation, and Gatekeeper assessment as **Notarized Developer ID**. The app inside
the mounted image carries its own staple. DMG SHA-256:
`5f8412b84b84a13ed9e9e7b65ca0c2551b95b3310a535327c2ddec9cc950e344`. It was built locally with
`pnpm tauri build --target universal-apple-darwin`; no Windows installer was published.

A Developer ID candidate built on 2026-10-04 from the reviewed working tree passed app and DMG
notarization, strict Developer ID signature verification with a secure timestamp, staple validation,
and Gatekeeper assessment as **Notarized Developer ID**, with hardened runtime enabled. The DMG was
mounted read-only and contained no bundled provider tools. Machine-readable verification output for
that candidate is kept outside the repository: `artifacts/` and `src-tauri/target/` are ignored local
directories and are not durable repository evidence.

That earlier candidate predates the 2026-10-06 provider expansion (Cloudflare, Firebase, EAS) and
the refresh/history hardening; the v0.1.0 release above is the first signed build that includes them.

Version text is not an artifact identity. Several different local files report version `0.1.0`. Use
the source commit when known, the full path or release asset name, and SHA-256 together. Signing
evidence for one hash must never be used to describe another build, and any new handoff needs a new
artifact hash and its own verification record.

## Upgrading from crowline

ratelline was previously named crowline. The bundle identifier changed from `io.crowline.desktop` to
`io.ratelline.desktop`, which moves both the application-data folder and the OS credential service
names. Upgrades keep existing data as follows:

- **App data.** On startup, if the new `io.ratelline.desktop` data folder is missing or empty and the
  sibling `io.crowline.desktop` folder exists, its files (`settings.json`, per-account CLI folders) are
  copied. Symbolic links are skipped. The old folder is never modified or deleted. Per-file failures
  are written to stderr and startup continues; a partially copied folder is not retried.
- **Credentials.** Bitbucket and Cloudflare tokens are now saved in `credentials.json` in the app data
  folder instead of the OS credential store. On startup, each Bitbucket or Cloudflare connection without
  a file entry or a recorded earlier lookup is looked up in the Keychain under `com.likeratel.ratelline.*`, then under the former
  `com.likeratel.crowline.bitbucket` and `com.likeratel.crowline.cloudflare` names. A value found there
  is written to the file, and both Keychain entries are deleted only after that write succeeds.
  Every lookup, whether it moved a token, found nothing, or was denied, is recorded in the file's
  `keychainChecked` list, so each connection reads the Keychain at most once; connections that already
  have a file entry never touch it. Credential lookups wait until this step finishes. macOS may ask once
  whether ratelline may read a Keychain item; if access is denied, the item stays in place and the
  connection reports its token as unavailable until the token is entered again. If the token or the
  lookup record cannot be written to the file, the lookup is retried on the next start. Keychain items
  that cannot be deleted stay in the Keychain; removing a connection no longer accesses the Keychain.
  Failures are written to stderr without token values, which is not visible when the app is opened
  from Finder. If the app quits after saving the file but before deleting the Keychain items, those
  items also stay; the next start sees the file entry and does not touch the Keychain again.
- **Not migrated.** Login-item (autostart) registration and macOS notification permission are tied to
  the app identity, so re-enable them in General settings and re-allow notifications. The macOS menu-bar
  position of the tray icon resets.
- **Build configuration.** The updater public key variable is now `RATELLINE_UPDATER_PUBLIC_KEY`, and
  release scripts read `RATELLINE_RELEASE_TARGET` and `RATELLINE_RELEASE_PLATFORM`. Rename the GitHub
  repository variable before running the release workflow.

## Trust layers

Tauri updater signing, Apple Developer ID signing/notarization, and Windows Authenticode signing are
separate controls:

- Tauri updater signatures let an installed ratelline build verify an update artifact. Tauri requires
  these signatures and does not allow verification to be disabled.
- Apple Developer ID signing proves the macOS publisher. Hardened runtime, notarization, stapling,
  strict code-signature verification, and Gatekeeper assessment are all required by this workflow.
- Windows Authenticode proves the Windows publisher. The release uses SHA-256 and an HTTPS RFC 3161
  timestamp service, then checks the signer thumbprint and timestamp on every generated installer.

The configuration follows the official Tauri guidance for
[updater signing](https://v2.tauri.app/plugin/updater/),
[macOS signing and notarization](https://v2.tauri.app/distribute/sign/macos/), and
[Windows signing](https://v2.tauri.app/distribute/sign/windows/).

## GitHub release configuration

Generate the Tauri updater key outside GitHub. Release jobs require an encrypted private key; an
empty updater-key password is rejected. Configure these Actions secrets:

| Secret                               | Purpose                                                                                    |
| ------------------------------------ | ------------------------------------------------------------------------------------------ |
| `TAURI_SIGNING_PRIVATE_KEY`          | Full encrypted Tauri updater private key.                                                  |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the updater private key.                                                      |
| `APPLE_CERTIFICATE`                  | Base64 PKCS#12 export containing the Developer ID Application certificate and private key. |
| `APPLE_CERTIFICATE_PASSWORD`         | Password for the PKCS#12 export.                                                           |
| `APPLE_API_PRIVATE_KEY`              | PEM contents of the App Store Connect API `.p8` private key used for notarization.         |
| `WINDOWS_CERTIFICATE`                | Base64 PKCS#12/PFX code-signing certificate and private key.                               |
| `WINDOWS_CERTIFICATE_PASSWORD`       | Password for the Windows PFX.                                                              |

Configure these Actions variables:

| Variable                         | Required format                                                                                      |
| -------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `RATELLINE_UPDATER_PUBLIC_KEY`    | Full public key paired with `TAURI_SIGNING_PRIVATE_KEY`; this value is compiled into release builds. |
| `APPLE_SIGNING_IDENTITY`         | Full `Developer ID Application: … (TEAMID)` identity.                                                |
| `APPLE_TEAM_ID`                  | Ten-character Apple team ID matching the signing identity.                                           |
| `APPLE_API_ISSUER`               | App Store Connect issuer UUID.                                                                       |
| `APPLE_API_KEY`                  | Ten-character App Store Connect API key ID.                                                          |
| `WINDOWS_CERTIFICATE_THUMBPRINT` | Forty hexadecimal characters identifying the PFX leaf certificate.                                   |
| `WINDOWS_TIMESTAMP_URL`          | HTTPS RFC 3161 timestamp endpoint supplied by the certificate provider.                              |

Do not place the private keys, certificate passwords, or PFX contents in files, shell history, issue
text, or workflow output. Rotating the updater key requires a staged application release trusted by
the old key; replacing the public key immediately prevents installed builds from accepting later
updates.

## What the workflow enforces

Before reserving a platform runner, the preflight job tests the validation scripts, requires every
secret and variable for the selected scope, validates their non-secret formats, and requires the
same semantic version in `package.json`, `src-tauri/Cargo.toml`, and
`src-tauri/tauri.conf.json`. Missing or malformed configuration fails without printing values.

The platform jobs run sequentially. The workflow pins third-party actions by commit and gives
write access to repository contents only to the job that creates the draft.

For macOS, Tauri imports the PKCS#12 certificate, signs the app with the configured Developer ID
identity and hardened runtime, submits the app for notarization, and staples it. The release wrapper
then submits the signed DMG separately, waits for acceptance, and staples it. Before the Tauri action
can upload files, both the app and DMG must pass strict `codesign`, exact identity and team checks,
secure timestamp checks, `stapler validate`, and Gatekeeper assessment as
`Notarized Developer ID`.

For Windows, the job imports the PFX into the ephemeral runner store and generates a release-only
Tauri overlay with the configured thumbprint, SHA-256 digest, HTTPS timestamp URL, and RFC 3161
mode. Before upload, every `.msi` and installer `.exe` under the bundle directory must report a
valid Authenticode signature from the configured thumbprint with a timestamp.

For every selected platform, the existing standalone updater verifier first validates the public
key format, then verifies every generated `.sig` against its corresponding artifact. Missing
artifacts, orphan signatures, a mismatched key pair, or zero verified signatures fail the build.
The workflow release title and body explicitly identify the result as a validation draft.

A failed later platform can leave a partial private draft from an earlier successful job. Never
publish a partial draft. Delete it or rerun the same version only after correcting the failure.

## Local Developer ID candidate

A local keychain identity and `notarytool` profile can be used for a private QA candidate without
putting Apple credentials in environment variables. This does not repair an ad-hoc or invalid app:
sign and notarize the app first, sign the DMG, and then submit the exact DMG that will be handed off.
For an already signed DMG, the repository helper can use the existing keychain profile without
printing its credentials:

```sh
node scripts/release-notarize-macos.mjs \
  --bundle-dir src-tauri/target/release/bundle \
  --keychain-profile ratelline
```

After submission succeeds, set the expected public identity and team ID and run the same artifact
checks used by CI:

```sh
APPLE_SIGNING_IDENTITY='Developer ID Application: NAME (TEAMID)' \
APPLE_TEAM_ID='TEAMID' \
node scripts/release-artifacts.mjs \
  --platform macos \
  --bundle-dir src-tauri/target/release/bundle
```

Generate the handoff checksum only after signing, notarization, and stapling, because those steps
change the artifact bytes.

## Updater boundary

ratelline's current updater code requests
`https://github.com/likeratel/ratelline/releases/latest/download/latest.json` and verifies downloads
with the public key compiled into the application. A normal local build has no public key, keeps the
updater disabled, and reports `updaterConfigured: false`.

The release-only overlay enables `createUpdaterArtifacts` and embeds
`RATELLINE_UPDATER_PUBLIC_KEY`. This is enough to generate and verify signed updater artifacts, but it
does not make the GitHub release endpoint a reachable update service. Do not invent or document a public URL
until a real distribution service exists.

At runtime, `install_update` installs only the update previously returned by `check_update`. The
install IPC requires `{ "expectedVersion": "<version shown to the user>" }`; the backend compares
that value with the pending update and rejects stale approvals. Failed downloads or installations
retain the pending update for a deliberate retry.

## Draft procedure

1. Set the same release version in `package.json`, `src-tauri/Cargo.toml`, and
   `src-tauri/tauri.conf.json`, then commit the version change.
2. Run **Build signed validation draft** on the intended commit and select only platforms whose
   credentials are configured.
3. Require every selected job to pass. A skipped, cancelled, or failed job is not a supported
   release artifact.
4. Record each asset name, SHA-256, source commit, platform, architecture, and the successful
   workflow run. Inspect `latest.json` only as generated metadata; it is not currently a reachable
   customer update service.
5. Keep the result private until product QA, platform installation testing, and the distribution
   decision are complete.

Release-script checks that do not use signing credentials or external services:

```sh
node --test scripts/release-scripts.test.mjs
for file in scripts/release-*.mjs; do node --check "$file"; done
bash -n scripts/release-build.sh
cargo test --locked --manifest-path scripts/verify-updater/Cargo.toml
cargo fmt --manifest-path scripts/verify-updater/Cargo.toml -- --check
```

Remote workflow execution, actual Windows signing, Apple submission by the workflow, and end-user
update installation remain unverified until their corresponding runs are completed and recorded.
