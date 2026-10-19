# Source and distribution review

> Historical review of the revision dated below. This is not a security assessment of later adapters, Bitbucket credentials, or the current dependency set. See the [README](../README.md) for current verification limits.

Review date: 2026-09-29. This records a bounded review of the current source and local history, not a security certification. No release has been published.

## Authentication and correctness fixes

- Added GitHub accounts use a dedicated `gh-isolated` build. Upstream `GH_CONFIG_DIR` alone does not isolate its OS keyring. The small ratelline patch namespaces keyring services by validated account ID and preserves the original service for local CLI sessions. Missing or incorrectly versioned helpers fail closed; isolated slots never fall back to system `gh`.
- The pinned-source build runs a mock-keyring regression covering local/A/B accounts, empty-slot reads, login, switching, and logout. It never accesses real credentials. Actual additional-account browser login and Windows keyring behavior remain unverified.
- Account mutations and login publication are serialized. Removing an account prevents late login publication. Regression tests cover persisted legacy migration, forged account definitions, unknown connections, and concurrent mutations.
- Independent functional review found stale settings drafts and stale cross-account activity status. Both were fixed and re-reviewed. Latest activity state is reconciled separately from verified Mine attribution.
- Reviewed boundaries include IPC account/provider references, AWS profile binding, HTTPS destination validation, argument-separated process invocation, private Unix directories, and redacted CLI errors. No additional confirmed command-injection or credential disclosure path was found in this scope.

## Dependency findings

These counts are package-presence checks, not counts of demonstrated ratelline exploits. Advisories can change after this review.

| Input | Evidence and remaining limit |
| --- | --- |
| Frontend pnpm lock | `pnpm audit`: 91 dependencies, no reported advisories. |
| Pinned Vercel 50.29.0 lock | 320 package names checked; 39 advisory records across 10 package names (1 critical, 17 high, 17 moderate, 4 low). Reviewed development/build/archive paths are not called by ratelline's fixed API GET/login operations. The token-output advisory requires token arguments, which ratelline does not pass. This does not clear the whole bundled CLI. |
| Patched GitHub helper | OSV scan of 145 embedded Go modules found advisory matches in 19 module names. The Go 1.26.8 standard-library query returned none. Some findings concern unused command families or attacker-controlled enterprise hosts, but complete symbol-level reachability was not established. |
| Cargo lock | Final OSV check covered 539 packages. Removing unused legacy AWS HTTP/TLS features removed vulnerable `h2 0.3.27` and `rustls-webpki 0.101.7`. Remaining matches are `glib 0.18.5` and `proc-macro-error 1.0.4`, absent from the current macOS dependency tree; Linux is not a supported bundle target. Windows runtime remains unverified. |

Examples of primary evidence: [GitHub keyring implementation](https://github.com/cli/cli/blob/v2.65.0/internal/config/config.go), [Vercel token-output advisory](https://github.com/vercel/vercel/security/advisories/GHSA-pgf8-2hgj-grqg), [go-gh advisory](https://pkg.go.dev/vuln/GO-2025-3732), and [RustSec advisory database](https://rustsec.org/advisories/).

Known helper advisories remain. Updating only a top-level package or npm lock overrides does not necessarily replace code embedded in Vercel's bundled JavaScript. Binary distribution needs further dependency remediation and a complete transitive license/notice inventory; [the current inventory](../THIRD_PARTY_NOTICES.md) is explicitly limited.

## Privacy and publication

The review enumerated all 181 pre-review Git objects, including six commits and 135 blob versions; no unreachable objects were present. A redacted scan of commit objects, text blobs, and current source found no recognizable token, private-key, or JWT patterns. Dedicated secret-scanner tools were unavailable; arbitrary-format secrets and binary content are detection limits.

All six commits present at the start of the review contain a non-noreply author/committer email. Publishing that history exposes the email. History has not been rewritten; the owner should decide whether that identity is intended before the first push. Local app settings, CLI credentials, generated tools, screenshots, and operational handoff files are ignored and excluded from commits.

The repository has an MIT license, bilingual setup instructions, pinned install inputs, generated-artwork provenance, and a CI matrix. Source can be shared with these limitations disclosed; packaged binaries are not cleared for a general release. Signing, notarization, live AWS access, additional real account logins, Windows execution, and remote CI have not been verified.

## Local validation

The macOS Apple Silicon app and DMG build completed with the Go 1.26.8 helper. Frontend checks, 12 unit tests, 9 tool-preparation tests, 9 browser end-to-end tests, and the production frontend build passed. Rust formatting, all 40 tests, and Clippy with warnings denied passed. The helper's mock-keyring isolation test also passed.

A real macOS run exposed the panel and its static controls through accessibility, and window sampling observed focus followed by blur-to-hide. Native automation could not reliably complete the tray-toggle, dropdown, and settings/dashboard interaction sequence because the panel disappeared during focus changes; those interactions remain a manual acceptance check. Browser tests cover the corresponding synthetic UI flows. Existing target selections and saved scope/history were compared without recording account or target values.

Implementation and security reviews were performed by separate assigned reviewer roles. Those native roles followed read-only assignments; the session did not provide OS-enforced read-only filesystem isolation. User acceptance is separate from implementation and review completion.
