# Authentication and provider access

ratelline runs on the user's computer without a ratelline-operated server. It prefers installed provider CLIs and their existing logins, supports multiple accounts where the provider integration can keep them separate, and uses a token only where a suitable CLI integration is unavailable. Monitoring is read-only; hiding a row never deletes or cancels a remote execution.

## GitHub accounts

Install [GitHub CLI](https://cli.github.com/) and run `gh auth login --hostname github.com --web` in a terminal. In **Settings → Connections**, choose GitHub; the **GitHub account** list appears immediately with each signed-in account's state (signed in, sign-in needed, check failed) and whether it can read every repository selected for the connection. Press **Use this account**, then pick projects. Additional connections may select different CLI users; GitHub Actions and GitHub Deployments use the same available user list. gh versions without `gh auth status --json` are read from the plain `gh auth status` report.

Every GitHub connection, including the built-in `github:local`, is pinned to one gh account and never follows gh's active account, so `gh auth switch` in a terminal does not change what ratelline reads. A connection without a selected account reports **No CLI account is selected** instead of reading through the active account. When the app starts and on each refresh, such a connection is pinned automatically only when exactly one signed-in account can read all of its repositories (or, with no repositories, return `GET /user`), every check finished, and no other connection of the same service already uses that account; otherwise it waits for your choice. If the only matching account is already used by another connection, the account list marks it **Used by another connection**; choose a different account or remove the other connection.

Account selections use [`gh auth token --hostname github.com --user USER`](https://cli.github.com/manual/gh_auth_token) and pass the result only to the matching `gh api` subprocess. Ratelline does not run `gh auth switch`, persist extracted GitHub tokens, return them to the webview, or distribute a patched GitHub CLI. Adding a new login in the terminal is an explicit user action. No Ratelline OAuth app or client ID is required.

A connection is bound to the verified provider identity. Existing API-era connections keep their IDs, selected projects, aliases, and notification rules; select the matching CLI account to reconnect. No unbound connection silently inherits the CLI's current user.

### When a connection fails

**Settings → Connections** lists failing connections first under **Needs fixing**, each with its cause, the number of affected targets and one action:

| Cause | Action |
| --- | --- |
| No account selected, or the selected GitHub account cannot read a repository (HTTP 403/404) | **Switch to an account with access** checks every signed-in account. One match is used immediately, several are offered as a short choice, and none leads to the sign-in command. |
| Signed out (GitHub) | Shows `gh auth login --hostname github.com --web` with a copy button and **Check again**. ratelline never runs gh's login or switch commands. |
| CLI missing or unsupported version | Installation help and **Check again**. |
| Network, rate limit, timeout | **Try again** starts a new query. |
| Vercel or AWS signed out | **Sign in with browser** runs the existing in-app sign-in. Vercel access denied asks you to sign in with an account that has access. |

The same cause reported by several queries for one target is shown once, and service health groups causes per connection. A connection stays bound to the GitHub user it first verified. Automatic binding never changes that user. When you choose an account of a different GitHub user yourself, in the fix card or the account list, ratelline first asks you to confirm that the connection's My runs and notifications will follow the new user; after you confirm, the connection keeps its ID and selected projects, its cached runs are discarded and queried again with the new user. An account already used by another connection still cannot be chosen. Repository access of an account belonging to a different GitHub user is checked only when you open the diagnosis (the fix card or the account list), never during automatic binding.

## Vercel accounts

Install [Vercel CLI](https://vercel.com/docs/cli) and sign in with `vercel login` for the default local connection. Additional accounts use the same installed executable with a private, per-connection [`--global-config`](https://vercel.com/docs/cli/global-options) directory. Use the app's browser sign-in action to authenticate that connection. This keeps additional accounts separate from the terminal's default Vercel session.

The default `vercel:local` connection follows the terminal's current Vercel login. Once its identity is saved, a different login is reported as an account mismatch instead of being read.

Ratelline reads through [`vercel api`](https://vercel.com/changelog/introducing-the-vercel-api-cli-command), explicitly selecting the personal or team scope. Unsupported commands or CLI output report an actionable compatibility error rather than an empty successful result. Previous API connections may need to sign in again with the same account. Ratelline no longer asks for a Vercel API token or its own OAuth registration.

## Provider decision matrix

| Provider | Authentication and execution |
| --- | --- |
| GitHub Actions / GitHub Deployments | Installed `gh`; explicit CLI-user selection supports multiple accounts. |
| Vercel | Installed `vercel`; default session or separate configuration per additional account. |
| AWS | AWS SDK uses existing shared profiles, selected regions, and cached sessions; installed AWS CLI starts SSO login. No extra app token is needed. |
| Bitbucket Cloud | Email and scoped API token saved in the app data folder (`credentials.json`); multiple accounts retained. No suitable official Pipeline CLI path was found in the [Atlassian CLI command reference](https://developer.atlassian.com/cloud/acli/reference/commands/). |
| Railway / Netlify / Render | Installed official CLI and its existing session. |
| Heroku / Fly.io | Installed official CLI; release history, not machine-runtime monitoring. |
| GitLab.com | Installed `glab`; deployment and pipeline monitoring. Self-managed GitLab is outside this integration. |
| DigitalOcean / Google Cloud / Azure App Service | Installed `doctl`, `gcloud`, or `az` and their local sessions. |
| Cloudflare Pages / Workers | User-entered Cloudflare account ID and an API token saved in `credentials.json`; documented direct read-only requests keep Pages, Worker deployments, and Workers Builds separate. Pages needs Cloudflare Pages Read, Worker deployments need Workers Scripts Read, and Builds need Workers CI Read. |
| Firebase Hosting / App Hosting | Installed `gcloud`, with the connection bound to one verified active account. A short-lived per-process token from that account reads Hosting releases and App Hosting rollouts directly; Firebase CLI login is not used. |
| EAS / Expo | User-entered EAS project UUID, owner, and slug with an app-managed context for installed `eas` that does not change project files. `eas build:list --json --non-interactive --limit 1 --offset 0` validates the server project identity before project-scoped Build and published Update reads. |
| Supabase / fastlane | Guidance only. Current Supabase function state is not deployment history; fastlane history belongs to its CI provider. |

No CLI or Node runtime is bundled. Ratelline checks installed executables and versions, uses bounded non-interactive commands, and distinguishes installation, login, compatibility, timeout, and partial-query failures. Installation/update links and login commands are available in connection settings; install or update the official CLI, then restart the app. Ratelline does not automatically install packages.

Bitbucket credentials and Cloudflare API tokens are saved in `credentials.json` in the app data folder, separately from settings. The file is unencrypted JSON with owner-only permissions (0600) on macOS and Linux (on Windows it relies on the user profile folder's permissions), so processes running as the same user can read it and folder backups include it. Earlier versions used the OS credential store; on start the app looks up each missing token there once, moves it into the file, and deletes the Keychain items only after the file is saved. Each lookup is recorded in the file so the Keychain is not read again for that connection. Firebase and EAS reuse their selected installed CLI sessions; any short-lived Firebase access token remains in the backend process and is not persisted, returned over IPC, or obtained by extracting a refresh token. Removing a connection removes only Ratelline's monitoring configuration and its saved token; it does not access the Keychain. Old GitHub/Vercel API credentials are not used by the CLI paths; migration does not revoke tokens at the provider. CLI logins and provider-side revocation remain under the user's control.

## Hide individual executions

The X button beside a run hides only that execution from the dashboard, history display, and tray summary. The setting persists across refresh and restart, and applies to duplicate observations through other accounts. Status changes do not reveal it again; a different execution ID remains visible. The immediate Undo action restores the last hidden row.

Hidden executions are also excluded from tray counts and subsequent notifications, including pending notification retries. The original cached row and history cursor remain available for undo and continued pagination. Account/project removal does not silently clear hidden-execution records. The store has an explicit 10,000-entry limit and reports a save error instead of evicting old hidden entries.

## Verification limits

Synthetic fixtures and fake CLI processes validate account selection, failure paths, compatibility handling, hidden-row persistence, and frontend behavior. They do not prove that every installed CLI version, real login, Windows shim, or native credential-store interaction works. Final executed checks are reported with the change; no provider credentials or real API responses are kept in test fixtures.
