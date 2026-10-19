# Product review and improvement pass — 2026-10-06

Scope: preserve Ratelline's read-only, local monitoring concept while completing the approved provider expansion and improving performance, reliability, and the path from connection to investigation. User acceptance remains separate from implementation and automated verification.

Status: implementation, automated verification, macOS debug build, and independent review are complete. The current native interaction smoke check was blocked by UI automation's inability to acquire the test window. User acceptance remains pending.

## Joint review

At the user's request, Claude Code was started in a separate pane in the same Ratelline workspace. It performed a read-only product review, discussed priorities and tradeoffs with Codex, and re-reviewed the resulting changes. Its code review did not use real provider credentials or execute provider queries. The native independent reviewer separately examined provider/authentication, refresh, and history changes. Reviewers were read-only by instruction, not by OS-enforced filesystem isolation.

The agreed priorities were:

- Refresh on return when any successful target's last current-query success is at least 20 seconds old. History-page updates and another target's completion cannot postpone this refresh. Preserve failed-query backoff and coalesce repeat focus events.
- Keep the last observed active count, but add a menu-bar warning when a visible monitored target has a failed, partial, or stale query. Do not confuse a query problem with a failed execution.
- For DigitalOcean and Azure, warn about a local 20-record cap only when omitted rows can affect the supported history window or active runs. Omitting completed records older than seven days alone is not a failed query.
- Keep previously loaded completed history across ordinary polling, while protecting account identity, cursor chains, bounds, and newer observations.
- Reuse HTTP connections, run independent CLI startup probes concurrently, and discover the shared `gcloud` executable once.
- Load the dashboard and panel as separate JavaScript entry paths, preserve shared styles, offer recovery if a view fails to load, and stop relative-time rendering while hidden.
- Define the workflow/title row layout in one shared stylesheet so lazy loading cannot change it through CSS order.
- Use execution/run wording for generic monitoring controls. Keep deployment terminology where a provider resource actually represents a deployment.

We rejected two shortcuts: a blanket 20-second retry policy for failed providers, and treating old history as authoritative updates to current activity status. Long-lived credential caching and changing snapshot delivery/subscription ordering were not adopted without further evidence; the authentication and event contracts stay explicit.

## Evidence

- The baseline production JavaScript asset was 546.81 kB (166.53 kB gzip). The final production build has a 276.76 kB entry (85.46 kB gzip), 152.37 kB shared chunk (48.33 kB gzip), 113.40 kB dashboard chunk (34.09 kB gzip), and 7.57 kB panel chunk (2.52 kB gzip). The panel does not request the dashboard chunk. This is a transfer/parse split, not a measured wall-clock startup improvement; the entry size alone is not total loaded JavaScript.
- A fake-CLI startup test checks that independent probes start together and `gcloud` is probed only once. No installed login is used by that test.
- Browser regressions cover view-specific loading, failed-load retry, focus-triggered refresh coalescing, hidden/visible clock behavior, connection recovery, and existing panel navigation.
- `pnpm check` and `pnpm build` passed. The full `pnpm test` run passed 77 tests; the final target-freshness change passed all 10 tests in `src/queryHealth.test.ts`, including two new regressions.
- Full `pnpm test:e2e` passed 127 scenarios. The subsequently added Cloudflare saved-connection/reload-retry scenario passed separately. After the layout/freshness changes, 17 affected scenarios passed; the final per-target timestamp correction then passed the four product-quality browser scenarios.
- `CARGO_INCREMENTAL=0 cargo test` passed all 278 tests. The first integrated run exposed three EAS fixture deadlines too short for concurrent subprocess startup; the fixture budgets were corrected without increasing production deadlines, and the full rerun passed.
- After the final history-lock change, `CARGO_INCREMENTAL=0 cargo test state::` passed 76/76. `cargo fmt --check` and `CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings` passed. The strict lint findings were expression/import cleanup and were independently re-reviewed.
- Independent review found no remaining confirmed P1/P2 in the changed provider, authentication, refresh, history, startup, and tray paths. This is a scoped source/contract review, not a separate security audit or proof of live-provider compatibility.
- `CARGO_INCREMENTAL=0 pnpm tauri build --debug --no-bundle --config /tmp/ratelline-productcheck-config.json` passed on macOS Apple Silicon in 1m 02s. Its identifier was `io.ratelline.productcheck.20261006`, with separate empty-account settings and automatic update checking disabled. The test build did not replace the installed Ratelline app.

## Native and live-account limits

The temporary app process launched, but computer-use automation returned `cgWindowNotFound` when acquiring its window by both path and bundle ID. A bounded Finder fallback also failed to acquire a window. The test process was stopped afterward. This run therefore does not establish panel-to-dashboard navigation, real status-item clicks, focus ordering, native visibility events, or multi-monitor behavior. Earlier native evidence is separately dated in [the preceding UX QA record](qa-2026-10-06.md); it is not presented as verification of this build. Browser and Rust model tests cover the automated interaction contract.

No real Cloudflare, Firebase, or EAS account was queried. Windows execution, provider-specific real login, OS credential-store integration against an actual token, and native notification banners remain outside this verification. No release, notarization, deployment, or remote push was performed in this pass.

The next useful observations are actual menu-bar click/focus behavior, startup time on a machine with many installed CLIs, and request cost with many Firebase targets. The bundle/probe improvements above are measured structural changes; no unmeasured latency or CPU reduction is claimed.

## Provider boundaries

See [provider expansion](provider-expansion-plan-2026-10-06.md), [authentication](authentication.md), and [mobile provider notes](mobile-provider-notes.md). Fixture tests establish command/response and failure contracts; they do not establish successful authentication against real Cloudflare, Firebase, or EAS accounts.
