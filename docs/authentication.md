# Authentication and provider access

ratelline runs on the user's computer without a ratelline-operated server. It prefers installed provider CLIs and their existing logins, supports multiple accounts where the provider integration can keep them separate, and uses a token only where a suitable CLI integration is unavailable. Monitoring is read-only; hiding a row never deletes or cancels a remote execution.

## GitHub accounts

Install [GitHub CLI](https://cli.github.com/) and run `gh auth login` in a terminal. In **Settings → Connections**, select a GitHub connection, choose a signed-in CLI username, and connect it. Additional connections may select different CLI users; GitHub Actions and GitHub Deployments use the same available user list.

The default local connection can use the CLI's current session. Explicit account selections use [`gh auth token --hostname github.com --user USER`](https://cli.github.com/manual/gh_auth_token) and pass the result only to the matching `gh api` subprocess. Ratelline does not run `gh auth switch`, persist extracted GitHub tokens, return them to the webview, or distribute a patched GitHub CLI. Adding a new login in the terminal is an explicit user action. No Ratelline OAuth app or client ID is required.

A connection is bound to the verified provider identity. Existing API-era connections keep their IDs, selected projects, aliases, and notification rules; select the matching CLI account to reconnect. An unbound extra connection does not silently inherit the CLI's current user.

## Vercel accounts

Install [Vercel CLI](https://vercel.com/docs/cli) and sign in with `vercel login` for the default local connection. Additional accounts use the same installed executable with a private, per-connection [`--global-config`](https://vercel.com/docs/cli/global-options) directory. Use the app's browser sign-in action to authenticate that connection. This keeps additional accounts separate from the terminal's default Vercel session.

Ratelline reads through [`vercel api`](https://vercel.com/changelog/introducing-the-vercel-api-cli-command), explicitly selecting the personal or team scope. Unsupported commands or CLI output report an actionable compatibility error rather than an empty successful result. Previous API connections may need to sign in again with the same account. Ratelline no longer asks for a Vercel API token or its own OAuth registration.

## Provider decision matrix

| Provider | Authentication and execution |
| --- | --- |
| GitHub Actions / GitHub Deployments | Installed `gh`; explicit CLI-user selection supports multiple accounts. |
| Vercel | Installed `vercel`; default session or separate configuration per additional account. |
| AWS | AWS SDK uses existing shared profiles, selected regions, and cached sessions; installed AWS CLI starts SSO login. No extra app token is needed. |
| Bitbucket Cloud | Email and scoped API token in the OS credential store; multiple accounts retained. No suitable official Pipeline CLI path was found in the [Atlassian CLI command reference](https://developer.atlassian.com/cloud/acli/reference/commands/). |
| Railway / Netlify / Render | Installed official CLI and its existing session. |
| Heroku / Fly.io | Installed official CLI; release history, not machine-runtime monitoring. |
| GitLab.com | Installed `glab`; deployment and pipeline monitoring. Self-managed GitLab is outside this integration. |
| DigitalOcean / Google Cloud / Azure App Service | Installed `doctl`, `gcloud`, or `az` and their local sessions. |
| Cloudflare Pages / Workers | User-entered Cloudflare account ID and OS-vault API token; documented direct read-only requests keep Pages, Worker deployments, and Workers Builds separate. Pages needs Cloudflare Pages Read, Worker deployments need Workers Scripts Read, and Builds need Workers CI Read. |
| Firebase Hosting / App Hosting | Installed `gcloud`, with the connection bound to one verified active account. A short-lived per-process token from that account reads Hosting releases and App Hosting rollouts directly; Firebase CLI login is not used. |
| EAS / Expo | User-entered EAS project UUID, owner, and slug with an app-managed context for installed `eas` that does not change project files. `eas build:list --json --non-interactive --limit 1 --offset 0` validates the server project identity before project-scoped Build and published Update reads. |
| Supabase / fastlane | Guidance only. Current Supabase function state is not deployment history; fastlane history belongs to its CI provider. |

No CLI or Node runtime is bundled. Ratelline checks installed executables and versions, uses bounded non-interactive commands, and distinguishes installation, login, compatibility, timeout, and partial-query failures. Installation/update links and login commands are available in connection settings; install or update the official CLI, then restart the app. Ratelline does not automatically install packages.

Bitbucket credentials stay in the OS secure credential store, separately from settings. Cloudflare follows the same vault boundary for its API token. Firebase and EAS reuse their selected installed CLI sessions; any short-lived Firebase access token remains in the backend process and is not persisted, returned over IPC, or obtained by extracting a refresh token. Removing a connection removes only Ratelline's monitoring configuration and app-managed vault material. Old GitHub/Vercel API credentials are not used by the CLI paths; migration does not revoke tokens at the provider. CLI logins and provider-side revocation remain under the user's control.

## Hide individual executions

The X button beside a run hides only that execution from the dashboard, history display, and tray summary. The setting persists across refresh and restart, and applies to duplicate observations through other accounts. Status changes do not reveal it again; a different execution ID remains visible. The immediate Undo action restores the last hidden row.

Hidden executions are also excluded from tray counts and subsequent notifications, including pending notification retries. The original cached row and history cursor remain available for undo and continued pagination. Account/project removal does not silently clear hidden-execution records. The store has an explicit 10,000-entry limit and reports a save error instead of evicting old hidden entries.

## Verification limits

Synthetic fixtures and fake CLI processes validate account selection, failure paths, compatibility handling, hidden-row persistence, and frontend behavior. They do not prove that every installed CLI version, real login, Windows shim, or native credential-store interaction works. Final executed checks are reported with the change; no provider credentials or real API responses are kept in test fixtures.
