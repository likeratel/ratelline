# 내부 IPC 계약

JSON 필드와 enum은 camelCase/아래 문자열을 사용한다. frontend 모델은 Zod로 확인한다.

Provider = `src/domain.ts::PROVIDERS` 및 `src-tauri/src/models.rs::Provider` 참조 (GitHub, Vercel, AWS, Bitbucket, 로컬 CLI 및 안내 전용 서비스).
ActivityStatus = queued | running | waiting | success | failed | canceled | skipped | unknown
Ownership = mine | other | unknown

Account { id, provider, label, mode: local|isolated, profile?: string|null, cliUsername?: string|null }
IssueKind = signedOut | missingCli | noAccess | rateLimited | network | timedOut | needsAccount | accountMismatch | unsupportedVersion | other (clients map unknown values to other)
Target { id, connectionId, provider, name, description?: string|null, metadata: Record<string,string> }
Settings { scope: mine|all, historyHours: 24|168, targets: Target[], accounts: Account[] }
Connection { connectionId, provider, status: connected|signed-out|missing-cli|error|unconfigured, account?: string|null, message?: string|null, cliVersion?: string|null, kind?: IssueKind|null }
Activity { id, targetId, connectionId, provider, kind, title, project, branch?: string|null, url, status, rawStatus, actor?: string|null, ownership, startedAt, finishedAt?: string|null, updatedAt }
Issue { connectionId?: string|null, provider, message, targetId?: string|null, kind: IssueKind (default other) }
QueryHealth { provider, connectionId, targetId, status: checking|success|partial|error, lastAttemptAt, lastSuccessfulAt?: string|null }
Snapshot { settings, settingsError?: string|null, connections: Connection[], activities: Activity[], issues: Issue[], refreshing: boolean, pollIntervalSeconds: positive integer (default 120), queryHealth: QueryHealth[], lastUpdatedAt?: string|null }
Discovery { connection: Connection, targets: Target[], awsProfiles: string[], issues: Issue[] }
LoginStatus { connectionId, provider, state: waiting|complete|error, message, url?: string|null, code?: string|null }

Commands:

- get_snapshot() -> Snapshot
- save_settings({settings}) -> Settings
- refresh() -> Snapshot
- request_refresh() -> void
- discover({provider, connectionId: string|null, profile: string|null, region: string|null}) -> Discovery
- create_connection({provider, label, profile: string|null}) -> Account
- remove_connection({connectionId}) -> Settings
- begin_login({provider, connectionId, profile: string|null}) -> LoginStatus
- get_login_status({provider, connectionId}) -> LoginStatus
- open_external({url}) -> void (https provider hosts allowlist)
- hide_panel() -> void
- open_dashboard() -> void

Legacy slots `github:local` and `vercel:local` retain their IDs, modes and monitored targets. GitHub Actions and Deployments use installed GitHub CLI and always read through Account.cliUsername; profile preserves the verified stable provider identity. `github:local` may carry a cliUsername like any other GitHub connection. A GitHub connection without cliUsername still loads, never reads through gh's active account, and reports `status: error, kind: needsAccount` with one connection issue. At startup and on each refresh, an unbound GitHub connection is pinned through `connect_github_cli_account` only when exactly one signed-in gh account can read every selected repository (or `GET /user` when it has none), no check was inconclusive, and that account is not already used by another connection of the same provider (by cliUsername or saved identity); otherwise it stays unbound. A conclusive result is re-checked after 15 minutes or when its targets or saved identity change. An inconclusive attempt, a failed binding, or a failed gh account listing backs off for 1, 2, 4, then 15 minutes (at least 15 minutes when rate limited). A user-requested `refresh` retries early, but only attempts made more than 30 seconds earlier. Automatic binding never reads repositories with an account whose GitHub user differs from the connection's saved identity; only `diagnose_github_connection`, which the user opens, checks such accounts. It runs after the cycle's `refreshing: true` snapshot is published. Pinning changes only ratelline settings, never gh's active account. Vercel uses installed CLI with a per-connection configuration directory for additional accounts. Existing settings, aliases and notification rules are retained. AWS slots bind to one local profile. Bitbucket retains OS-vault API credentials. Target identity is `(provider, connectionId, id)`.

Settings.hiddenActivities stores structured `{provider,targetId,activityId}` keys (default empty; max 10,000). `change_settings({change:{kind:"activityVisibility",activity:{provider,targetId,activityId},hidden:boolean}})` persists hide/undo without deleting remote data, cached activities, or pagination. The frontend filters all execution lists; backend tray counts and notifications also respect hidden keys. Hide validates that the row exists in the current snapshot; undo may remove a persisted key after the row leaves cache.

`discover` without a connectionId is allowed only for local AWS profile-name discovery and performs no remote request. Its connection uses unconfigured sentinel `aws:local`. Legacy saved settings migrate provider defaults and split AWS targets by profile.

Issue kinds come from classified CLI failures: HTTP 401 → signedOut, rate limit (checked before 403) → rateLimited, DNS/TCP/TLS failures → network, HTTP 403/404 and not-found/forbidden wording → noAccess. Raw stderr is never returned. Per-status probe failures (GitHub active runs, Vercel active deployments, GitHub Deployments statuses) are merged into one issue per cause, and a target never carries two issues with the same classified kind.

Events: snapshot-updated carries Snapshot. Backend polls even with window hidden. Keep IPC commands fixed and validate every setting/target.
`QueryHealth` identifies one monitored target by `(provider, connectionId, targetId)`. `lastAttemptAt` records the current polling attempt; `lastSuccessfulAt` changes only after the current monitored query completes successfully. `success` means the current required scans completed, `partial` means at least one required scan did not complete, and `error` means the target query failed. History pagination does not alter this current-query state. A missing health row remains compatible with older snapshots and is treated by the client as unconfirmed.

`pollIntervalSeconds` is the backend's current poll cadence for this snapshot; clients use it to judge freshness per target. `lastUpdatedAt` remains an aggregate compatibility field and is not a substitute for target health or an activity's provider-supplied `updatedAt` timestamp.

`request_refresh` only signals the polling loop. It does not return a snapshot or force a concurrent provider query; repeated signals are coalesced, and a signal received during a running cycle is retained only when a follow-up is needed.
`settingsError` is an app-level persistent recovery error, not a provider issue. A startup read/parse/validation failure locks settings and account/auth mutations for the session to preserve the original file. If only migration persistence fails, the readable settings remain available. Repair the file/permissions and restart before saving. UI suppresses first-use onboarding and shows the lock in every view while permitting read-only monitoring. An initial event subscription failure must be retried before a refresh is treated as recovered.
Dates: RFC3339 UTC strings. Unknown ownership stays unknown; mine filter must not include it.
AWS adapter contract: providers::aws::{discover(profile: Option<String>, region: Option<String>) -> Result<Discovery,String>, collect(targets: &[Target], history_hours: u32) -> (Vec<Activity>,Vec<Issue>)} async. Backend may adapt signatures with explicit coordination.

No CLI or Node runtime is bundled. Auxiliary adapters use installed official CLIs, discovered through bounded executable paths and version probes. Missing installation, unsupported CLI commands/output, and expired login are surfaced separately; no shell command strings or automatic installers are used.

Authentication commands:

- get_github_cli_accounts() -> string[] (public usernames only)
- get_github_cli_account_states() -> {login, active, state: ok|signedOut|error}[] (`active` is informational only)
- diagnose_github_connection({connectionId}) -> {login, active, state, canAccessAll, inaccessibleTargets: string[], checkFailed?: IssueKind|null, identityMismatch, usedByConnectionId?: string|null}[]; probes `GET /user` and `GET /repos/{owner}/{repo}` with each account's own token
- connect_github_cli_account({provider,connectionId,username,allowIdentityChange?: boolean}) -> Account; a saved identity (profile) that differs from the selected user's is rejected unless `allowIdentityChange` is true (sent only after the user confirms in the UI). The flag expresses consent given in the UI; the backend does not and cannot prove that consent itself. On a change the new user ID is saved; the connection's cached activities, history, connection state and notification baselines are dropped, and its targets are published as `checking` and pending until they are queried again. The first query rebuilds the notification baseline silently. Duplicate username/identity checks always apply; automatic binding never sets the flag.
- begin_login({provider,connectionId,profile}) -> LoginStatus (Vercel CLI / AWS SSO)
- get_login_status({provider,connectionId}) -> LoginStatus
- connect_bitbucket({email,token,connectionId}) -> Account
- connect_cloudflare({provider: cloudflare-pages|cloudflare-workers,accountId,token,connectionId: string|null}) -> Account
- register_eas_project({connectionId,projectId: UUID,owner,slug}) -> Target[]

GitHub and Vercel do not require a Ratelline OAuth client ID or API-token form. Read requests use installed CLI commands; GitHub per-user tokens remain ephemeral in the backend subprocess environment. No token is returned over IPC. Bitbucket and Cloudflare are explicit token-authentication exceptions; their tokens stay in OS credential storage and are never returned over IPC. Cloudflare only issues validated direct GET requests. Firebase uses the selected installed `gcloud` account and a short-lived process-only token; EAS receives a UUID, owner, and slug and runs its read commands in an app-created context without reading or changing the user's project. See docs/authentication.md for setup and migration details.

## Display scope and new project defaults

`settings.displayScopes` defaults to `{ providers: {}, targets: {} }`. Values are `mine` or `all`; target keys include provider, connection ID and external target ID. `displayTargetScope` and `displayProviderScope` accept `scope: null` to inherit. Resolution is project > service > legacy `settings.scope`. Dashboard, history, panel and native tray counts share eligibility; duplicate execution observations preserve newest state and a matching account identity. Removed projects prune their overrides. Display filters never change notification scope.

Selecting a new monitored project creates an enabled notification rule with `mine` and success/failed/canceled outcomes. Existing explicit rules and the master notification setting remain unchanged. New targets baseline without replaying history; other targets retain observed runs, delivered deduplication and pending retries. Account aliases are saved explicitly with confirmation; input blur does not persist them.
