# Provider expansion plan — 2026-10-06

Status: implementation, synthetic adapter-fixture/browser checks, and independent review are complete. No authenticated provider checks were performed. See [product review and verification](product-review-2026-10-06.md) for exact command results and native validation limits. User acceptance remains pending.

## Boundaries

Ratelline remains a local, read-only monitor. New adapters use fixed executable arguments or direct HTTPS requests with validated path components; they do not assemble shell strings, deploy, retry, cancel, roll back, or change provider configuration. Tests use fictional accounts, projects, IDs, tokens, and API payloads. No adapter extracts long-lived refresh tokens, evaluates user project configuration, or automatically configures a CI deployment.

## Cloudflare Pages and Workers

Cloudflare uses a separate connection with a user-entered account ID and API token stored only in the OS credential vault. The adapter issues documented `GET` requests after validating the account ID, target name, and pagination data. Pages requires Cloudflare Pages Read; Worker deployments require Workers Scripts Read; Workers Builds require Workers CI Read. [Pages deployments API](https://developers.cloudflare.com/api/resources/pages/subresources/projects/subresources/deployments/methods/list/), [Workers Builds API](https://developers.cloudflare.com/api/resources/workers_builds/subresources/builds/methods/list/)

Discover and monitor Workers as a distinct target kind from Pages. The Workers list endpoint reads scripts for an account, and the Workers deployments endpoint reads deployment history for one script at `GET /accounts/{account_id}/workers/scripts/{script_name}/deployments`; a deployment includes a creation time, source, traffic strategy, versions, and optional annotations. Do not represent a Worker script's current metadata as a deployment event, and do not call the documented POST/DELETE deployment operations. [Workers list API](https://developers.cloudflare.com/api/resources/workers/subresources/beta/subresources/workers/methods/list/), [Workers deployments API](https://developers.cloudflare.com/api/resources/workers/subresources/scripts/subresources/deployments/)

## Firebase Hosting and App Hosting

Firebase requires installed `gcloud` and binds the connection to its selected verified active account. It passes `--account` for token and project operations so another active account cannot silently supply the token. The short-lived access token stays in process memory and is used only as an Authorization header for direct read-only Firebase REST calls; it is never persisted, returned to the webview, derived by reading gcloud credential files, or taken from Firebase CLI login. [gcloud account list](https://docs.cloud.google.com/sdk/gcloud/reference/auth/list), [print access token](https://docs.cloud.google.com/sdk/gcloud/reference/auth/print-access-token)

For Hosting, enumerate accessible projects/sites and read `sites.releases.list`, which returns paginated releases including `releaseTime`, `releaseUser`, and a version. For App Hosting, enumerate backends and read the documented paginated rollout list for each backend. A token lacking the required read scope, a project inaccessible to the selected account, or a partial page is a visible query failure, never an empty successful history. [Hosting releases API](https://firebase.google.com/docs/reference/hosting/rest/v1beta1/sites.releases/list), [App Hosting rollouts API](https://firebase.google.com/docs/reference/apphosting/rest)

## EAS / Expo

EAS has project-scoped, paginated history commands but no approved account-wide project discovery command. The connection therefore requires a user-entered EAS project UUID, owner, and slug as an explicit monitored target. Ratelline supplies those values through a private app-managed context without changing, reading, or evaluating the user's project files, dependencies, or scripts. [EAS CLI reference](https://docs.expo.dev/eas/cli/), [`ProjectIdContextField`](https://raw.githubusercontent.com/expo/eas-cli/main/packages/eas-cli/src/commandUtils/context/ProjectIdContextField.ts), [`getProjectIdAsync`](https://raw.githubusercontent.com/expo/eas-cli/main/packages/eas-cli/src/commandUtils/context/contextUtils/getProjectIdAsync.ts)

The adapter validates the server project ID, owner, and slug with `eas build:list --json --non-interactive --limit 1 --offset 0` in that context, then reads bounded `eas build:list` and `eas update:list --all --json --non-interactive` pages. Build history can expose structured statuses, timestamps, initiating actor, error, and artifact URLs. Update-list JSON is display-oriented and must not be parsed for timestamps or actors; obtain each bounded update group with `eas update:view GROUP_ID --json`, which has structured creation time and URL fields but no actor in its JSON result. Unknown update actors remain unknown. [`build/list.ts`](https://raw.githubusercontent.com/expo/eas-cli/main/packages/eas-cli/src/commands/build/list.ts), [`BuildFragment`](https://raw.githubusercontent.com/expo/eas-cli/main/packages/eas-cli/src/graphql/types/Build.ts), [`update/list.ts`](https://raw.githubusercontent.com/expo/eas-cli/main/packages/eas-cli/src/commands/update/list.ts), [`update/view.ts`](https://raw.githubusercontent.com/expo/eas-cli/main/packages/eas-cli/src/commands/update/view.ts)

## Supabase and fastlane

Supabase Edge Functions remain guidance-only. The current function-list API returns a current record with status, version, creation time, and update time, but not immutable deployment attempts, failed deploys, or actors; this cannot become deployment history. CI-run Supabase deployments should be monitored at their GitHub Actions or GitLab CI source. [Supabase function list API](https://supabase.com/docs/reference/api/v1-list-all-functions), [Supabase deployment guidance](https://supabase.com/docs/guides/deployment)

fastlane also remains guidance-only. It is a local/CI lane runner, and its JUnit output belongs to one CI execution rather than a hosted account-wide deployment history. Direct users to the CI provider that ran the lane. [fastlane CI guide](https://docs.fastlane.tools/best-practices/continuous-integration/)

## Required completion evidence

- [x] Synthetic adapter fixtures and command-contract tests cover valid pages, malformed inputs, sign-in failures, pagination, and partial-query handling without real account output.
- [x] UI tests cover explicit EAS project entry, validation/failure feedback, target selection, credential clearing, and retry after a saved connection fails to reload. Adapter/state tests separately cover identity changes and account isolation.
- [x] The repository's required TypeScript, browser, Rust, and build checks pass; affected reruns are recorded in the verification document.
- [x] An independent reviewer checked read-only boundaries, token handling, account isolation, error paths, and completion criteria.
- [x] Live-account limitation recorded: authenticated checks were not performed and are not implied by fixtures or command contracts.
- [ ] User final review.
