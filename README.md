# ratelline

English · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

[![Verify desktop builds](https://github.com/likeratel/ratelline/actions/workflows/verify.yml/badge.svg)](https://github.com/likeratel/ratelline/actions/workflows/verify.yml)

<img src="public/ratelline.png" alt="A honey badger patrolling a deploy line" width="160" />

ratelline is a menu-bar and system-tray app that patrols your deploy lines: recent runs at a glance, with failures shown as plainly as successes. It is named after the ratel, the honey badger that does not back off from a few stings. Check deployment and CI status in the panel, then open the dashboard to connect accounts, search history, and configure notifications. Built with Tauri 2, React, TypeScript, and Rust; service queries run on your computer. The interface follows your system language and supports English, Korean, Japanese, and Simplified Chinese.

> Built with AI. I set the direction and checked the integrations against my own accounts; the code, tests and docs were written in sessions with [Claude Code](https://claude.com/claude-code). See [Why I built this](#why-i-built-this).

**Download:** the signed and notarized macOS build (Apple Silicon and Intel) is on the [Releases page](https://github.com/likeratel/ratelline/releases). No Windows installer is published yet.

> **Verification status.** GitHub Actions, Vercel, and Bitbucket Cloud have been exercised with real accounts. Every other service is covered by command-contract tests against official CLI and API documentation using synthetic data, not by a live-account check. See [provider verification](docs/provider-verification.md) and the [2026-10-06 product review](docs/product-review-2026-10-06.md).

## Why I built this

My deployments were spread across GitHub Actions, Vercel, AWS and Bitbucket, and finding out whether the last push had landed meant opening four dashboards. I wanted a menu-bar app that answers that in a glance, shows a failed run as plainly as a successful one, and never holds a token anywhere but my own machine. It started as crowline (crows on a wire); it became ratelline when the honey badger took over as the mascot, because that is the attitude I want from a deploy monitor.

What I did: chose the services and the read-only scope, verified GitHub Actions, Vercel and Bitbucket Cloud against real accounts, kept the other providers honest with contract tests instead of claiming more, and ran every change through an independent review before it was committed. The implementation, the tests and these docs were written with Claude Code; the ratel artwork was generated from prompts and composed into the icons by hand.

## See it in action

A quick tour: menu bar panel → deployment overview → service filter → connections → project notification rules.

![Ratelline demo: filter runs by service, open connections, and choose notifications for your runs](docs/images/walkthrough.gif)

Captured from the app’s browser preview with fictional projects and accounts. The demo uses the Korean UI; it does not show live deployments or notification delivery.

<details>
<summary>Screenshots</summary>

**Menu-bar panel** — status, branch, and who started each run.

<img src="docs/images/menu-bar.png" alt="Compact panel showing active and completed runs with platform icons and actor names" width="420" />

**Dashboard** — active runs and recent results in one view.

![Deployment dashboard with fictional GitHub Actions and Vercel runs](docs/images/dashboard.png)

**Connections** — choose a service, account, and projects to monitor.

![Service selection and GitHub account connection settings](docs/images/connections.png)

**Notifications** — choose whose runs and which results matter for each project.

![Project notification settings with a My runs rule](docs/images/notifications.png)

</details>

## Development setup and run

Node.js 22.23.3 or later, pnpm 10, and Rust stable are required. Install [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) (Xcode Command Line Tools on macOS; Microsoft C++ Build Tools and WebView2 on Windows).

```sh
git clone https://github.com/likeratel/ratelline.git
cd ratelline
pnpm install --frozen-lockfile
pnpm tauri dev
```

## Build

```sh
pnpm desktop:build
```

Packages are written to `src-tauri/target/release/bundle/`. Build macOS apps on macOS and Windows installers on Windows. The repository contains CI configuration for macOS Apple Silicon, macOS Intel, and Windows x64; that is catalog/configuration only, not evidence of remote builds or published installers.

## Connect accounts

Open **Settings → Connections**, choose a service, connect an account, and select projects to monitor. Project choices apply immediately. Use **Edit alias → Save alias** to change an account name and see saved confirmation. Select or deselect all search results at once; hidden matches and other accounts keep their selections.

Display scope inherits from the app default to the service and then the project. Set **My runs / All runs** per service or project in Connections; project choices take priority and apply to dashboard/history/menu bar. Notification rules are separate. Newly selected projects default to enabled **My runs · All results** rules while the master notification switch and existing rules remain unchanged. The notification switch pauses delivery without erasing project rules. Account aliases use explicit Save/Cancel and show saved confirmation. Bulk project selection uses one Select all/Clear all action for the current search. [Provider verification evidence](docs/provider-verification.md) separates command-contract tests from authenticated live checks.

GitHub uses the installed `gh` CLI. Sign in with `gh auth login`, then select the CLI username for each connection. Vercel uses the installed `vercel` CLI and separate CLI configuration for additional accounts. No Ratelline OAuth registration or GitHub/Vercel token form is needed. AWS reuses shared profiles; other supported CLI integrations reuse their existing sessions.

Existing connection IDs, selected targets, aliases and notification rules are preserved. Reconnect API-era GitHub accounts to the matching CLI username and sign in to the corresponding Vercel CLI configuration. Bitbucket remains the token-authentication exception. See [authentication and provider access](docs/authentication.md).

## Supported monitoring

ratelline prefers installed official CLIs, retains AWS SDK shared-profile access and Bitbucket API-token access, and bundles no provider executable or Node runtime. It requires no deployment hooks or project instrumentation.

| Platform                                      | Monitoring                                     | Limits                                                                                                              |
| --------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| GitHub Actions                                | Workflow runs                                  | Installed gh; selected CLI user per connection, recent pages and active checks. |
| Vercel                                        | Project deployments                            | Installed vercel; separate additional-account configuration; `BLOCKED` excluded. |
| AWS                                           | CloudFormation, ECS, Amplify branch jobs       | Selected profile/region; actor is unknown.                                                                          |
| GitHub Deployments                            | Deployment records and latest status           | Separate from Actions; up to 20 status-detail reads per recent page.                                                |
| Railway, Netlify, Render                      | Deployment history                             | Installed official CLI and provider-specific recent-page limits.                                                    |
| Heroku, Fly.io                                | App releases                                   | Fly shows release status, not machine runtime status.                                                               |
| GitLab.com                                    | Deployments and CI pipelines                   | No self-managed GitLab or actor attribution.                                                                        |
| Bitbucket Cloud                               | Repository Pipelines                           | Email + scoped API token; recent pages, active checks, and triggerer-based ownership. Data Center is not supported. |
| DigitalOcean, Google Cloud, Azure App Service | App deployments, Cloud Build/Run, Kudu records | Provider-specific scope, output, and paging limits.                                                                 |
| Cloudflare Pages / Workers                    | Pages deployments; Worker deployments and Builds | Account ID plus an OS-vault API token; direct read-only HTTP. Pages, Workers, and Builds use separate token permissions. |
| Firebase                                      | Hosting published releases; App Hosting rollouts | Installed `gcloud` and its selected active account; short-lived process-only access token. |
| EAS / Expo                                    | Project-scoped Builds and published Updates     | Installed `eas`; enter project UUID, owner, and slug. No account-wide project discovery. |

Cloudflare tokens remain in the OS credential store and require **Cloudflare Pages Read** for Pages, **Workers Scripts Read** for Worker deployments, and **Workers CI Read** for Builds. Firebase binds each connection to the current `gcloud` account and uses `--account` for its access token and project listing; it does not use the Firebase CLI login. EAS creates its own read-only context and does not read or change the selected project's files. Published EAS Updates have no initiating actor and do not include failed publish attempts. Supabase Edge Functions and fastlane remain CI guidance: their trustworthy run history belongs to GitHub Actions, GitLab CI, or the CI system that ran fastlane. See the [provider decision matrix](docs/authentication.md#provider-decision-matrix) and [mobile provider notes](docs/mobile-provider-notes.md).

These provider paths passed synthetic adapter-fixture and browser checks, but have not been verified with a live account. No real credentials or provider responses are stored in tests. See the [joint product review and verification record](docs/product-review-2026-10-06.md) for results and limits.

### Bitbucket Cloud

Choose **Settings → Connections → Bitbucket**, enter your Atlassian email and an [API token](https://support.atlassian.com/bitbucket-cloud/docs/create-an-api-token/) with `read:user:bitbucket`, `read:workspace:bitbucket`, `read:repository:bitbucket`, and `read:pipeline:bitbucket`, then connect and select repositories. No separate CLI is needed. The service display name becomes the default account name; use the pencil to set an alias. **Update token** renews credentials for the same account without removing monitored repositories.

Credentials are stored in the OS credential store, not settings, logs, or installer files. Removing a Bitbucket connection also removes its stored credentials. Repository selection, menu-bar visibility, and project notification rules use the same controls as other services. Pipelines with an unidentified triggerer stay **Unknown** and do not count as **My runs**. Recent and active pipeline queries were verified against three already-connected repositories on 2026-10-02. Fresh-account sign-in remains unverified; automated tests use fictional data.

Recent Pipelines are loaded 20 at a time. Active checks query building, pending, paused, and halted states concurrently, up to 50 per state. Repository discovery stops at 1,000 repositories, 50 repository pages, or 10 workspace pages; truncation and partial failures are shown in the UI.

## Panel, notifications, and status

Use the X beside an execution to hide just that row permanently across refresh and restart; Undo restores it. New executions remain visible. Hidden rows do not contribute to tray counts or subsequent notifications.

- **Menu-bar panel:** recent runs with platform icons, status, branch, actor, and a compact per-target query-health summary. Execution names appear when the service supplies them: GitHub workflows, Vercel deployments, Bitbucket custom pipelines, GitLab pipelines/jobs, and Cloud Build triggers. Duplicate names are shown once. Click outside or press `Esc` to dismiss.
- **Dashboard:** filter by service, account, **My runs / All runs**, and local execution status; search and load older history where supported. Unknown actors appear in **All runs**. Status filters do not change saved display scope or notification rules. Open an execution's details to see collected context, the source-reported execution update time, and target query health.
- **Notifications:** choose **Off / My runs / All runs** per project, then **All results / Success only / Failed only / Custom**. Notifications include service, result, execution name/title, project, branch, and actor. Long text may be shortened by the OS banner. Initial results after startup do not trigger historical alerts.
- **Menu bar settings:** choose visible services/projects and 1–10 rows per section. Runs stay in newest-first order. On macOS, Command-drag the ratel icon to change its position among other menu-bar apps. While runs are in progress the macOS menu-bar ratel walks; turn this off with **Animate the menu bar icon while runs are in progress**. It stays still when the system Reduce motion setting is on. Windows and Linux keep a static tray icon.
- **General:** language, launch at login, update preferences, and app version.

Active runs remain visible after 24 hours and are labelled with their last observed state; this does not imply a failure. Vercel `BLOCKED` entries are hidden. Completed history supports 24-hour and 7-day views. ratelline is read-only: it cannot deploy, rerun, cancel, or change infrastructure.

## Updates, data, and limits

Queries run roughly every 30 seconds while jobs are active and every 120 seconds while idle, with backoff after errors. The app shows query state and freshness per monitored project; an activity's provider-reported update time is separate from the local query time. On return to the foreground or network, stale or incomplete results can request a coalesced poll. Timeouts, partial results, and provider paging limits appear in the UI. Delayed results can miss notifications; use the service’s own history for a complete record.

Settings and hidden execution keys live in `io.ratelline.desktop/settings.json` under the OS application-data directory. Bitbucket tokens stay in the OS credential store. GitHub account-specific tokens are obtained through gh only for the matching subprocess and are not saved by Ratelline. Provider credentials and executables are not bundled.

**Upgrading from crowline.** ratelline was previously named crowline. On first start, if the new `io.ratelline.desktop` data folder is empty, the app copies settings and account folders from `io.crowline.desktop`; the old folder is left in place and copy failures are logged without stopping startup. Bitbucket and Cloudflare tokens are read from the old `com.likeratel.crowline.*` credential entries on first use and saved under the new `com.likeratel.ratelline.*` names. macOS may ask once to allow ratelline to read the old Keychain item. Removing a connection deletes both the new and old entries. See [release engineering](docs/releases.md#upgrading-from-crowline).

If settings cannot be read or a migration cannot be saved, the app preserves the original file and locks settings/account changes with a recovery notice. Restore a valid backup or the folder permissions, then restart the app. Readable settings and monitoring results remain available.

Automatic updates are not configured in the current build. There is no public update endpoint yet. See [release engineering](docs/releases.md) before enabling distribution or updates.

The latest CLI-first multi-account changes have synthetic tests; live sign-in for every CLI version and Windows execution remain unverified.

Locally verified: macOS Apple Silicon build/startup, GitHub/Vercel panel data, Bitbucket pipeline queries, and automated tests with fictional data. Windows runtime, live AWS access, additional-account browser sign-in, and the latest OS notification banner remain unverified. Dependency findings are recorded in the dated [security review](docs/security-review.md).

See the [documentation index](docs/README.md) for release, provider, media, and historical review notes.

The [2026-10-04 product QA report](docs/qa-2026-10-04.md) records selection latency, interface checks, independent review, and release blockers. Bulk changes save before background queries complete; clearing selections makes no remote request. A successful local build is not certification for every provider or operating system.

The [2026-10-06 evolution and UX QA report](docs/qa-2026-10-06.md) records query freshness, execution investigation, connection guidance, and menu-bar interaction fixes, with separate automated and native verification limits.

## Development checks

```sh
pnpm check
pnpm test
pnpm exec playwright install chromium
pnpm test:e2e
pnpm build
cd src-tauri
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

These are current repository commands; this README does not claim fixed test counts or a current passing run.

## License

ratelline is available under the [MIT License](LICENSE). Current package and logo notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Ratel icon provenance is in [brand notes](docs/brand.md).

ratelline is an independent project. It is not affiliated with, endorsed by, or sponsored by GitHub, Vercel, Amazon Web Services, Atlassian, Cloudflare, Google, Microsoft, or any other service it can monitor. Service names and logos are trademarks of their respective owners and are used only to identify the corresponding service.
