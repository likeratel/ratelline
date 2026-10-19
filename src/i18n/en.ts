import type { TranslationKey } from "./ko";

export const en = {
  "bitbucket.reloadFailed":
    "Your account is connected, but the view could not be updated. Retry to reload it.",
  "platform.bitbucket": "Connect with your Atlassian email and an API token.",
  "bitbucket.email": "Atlassian email",
  "bitbucket.token": "API token",
  "bitbucket.connect": "Connect account",
  "bitbucket.renew": "Update token",
  "bitbucket.createToken": "Create API token",
  "bitbucket.help":
    "Create an API token for Bitbucket with the read permissions below.",
  "bitbucket.connecting": "Connecting…",
  "bitbucket.newAccount": "Connect a Bitbucket account",
  "bitbucket.invalid": "Check your email and API token.",

  "common.retry": "Try again",
  "common.refresh": "Refresh",
  "common.cancel": "Cancel",
  "common.updatedSuffix": " updated",
  "common.period24h": "Last 24 hours",
  "common.period7d": "Last 7 days",
  "common.targetCount": "{count} targets",
  "common.selectedCount": "{count} selected",
  "common.totalTargets": "{count} targets total",
  "common.moreCount": "View {count} more",
  "common.unknownActor": "Unknown actor",
  "boot.checking": "Checking connection status…",
  "settingsRecovery.title": "Couldn’t load settings",
  "settingsRecovery.locked": "Saving locked",
  "settingsRecovery.body":
    "The original settings are preserved. Restore read and write access to the settings file and folder, or restore a backup, then restart the app.",
  "settingsRecovery.details": "Error details",
  "nav.monitoring": "Monitoring",
  "nav.serviceFilter": "Service filter",
  "nav.allServices": "All services",
  "nav.panelPreferences": "Menu bar display",
  "nav.connections": "Connections",
  "nav.generalSettings": "Settings",
  "nav.tagline": "Run monitor",
  "dashboard.preview":
    "Preview · Sample data. Live connections are available in the desktop app.",
  "dashboard.connectedProjects": "Connected projects",
  "dashboard.title": "Run overview",
  "dashboard.subtitle": "Track recent runs for the selected account.",
  "dashboard.active": "In progress",
  "dashboard.runs": "runs",
  "dashboard.completed": "Recently completed",
  "dashboard.elapsedPeriod": "{period}",
  "dashboard.connections": "Service connections",
  "dashboard.monitoringTargets": "Monitoring {count} targets",
  "dashboard.actorScope": "Actor scope",
  "dashboard.mineOnly": "My runs",
  "dashboard.allRuns": "All runs",
  "dashboard.searchPlaceholder": "Search runs, projects, or branches",
  "dashboard.search": "Search runs",
  "dashboard.clearSearch": "Clear search",
  "dashboard.period": "Time range",
  "dashboard.mineDescription": "My pushes · PRs · manual runs · reruns",
  "dashboard.allDescription": "Every run available to my accounts",
  "dashboard.unknownActors":
    "View {count} runs with unknown actors in All runs",
  "dashboard.guideTitle": "What appears here",
  "dashboard.guideHistory":
    "In-progress items retain their last observed state beyond 24 hours until completion is confirmed. Recently completed items use the selected range. Load older results per target from ‘View earlier runs’.",
  "dashboard.guideSettings":
    "‘Menu bar display’ controls the tray. ‘Connections’ controls the accounts and targets monitored by the dashboard.",
  "dashboard.pending":
    "Showing recent results first. Checking {count} targets for in-progress runs.",
  "dashboard.dismissError": "Dismiss error",
  "dashboard.onboardingTitle": "Add your first connection",
  "dashboard.ready": "Ready when you are",
  "dashboard.onboardingBody":
    "Use service APIs and local CLI sessions on this device to see active and recent runs together.",
  "dashboard.connectService": "Connect a service",
  "dashboard.onboardingSteps":
    "Connect an account and select projects to start monitoring immediately.",
  "dashboard.activeEmpty": "No in-progress runs match the current filters.",
  "dashboard.completedEmpty": "No recent runs match the current filters.",
  "dashboard.needsAttention": "Needs attention",
  "dashboard.refreshing": "Checking for updates…",
  "dashboard.neverUpdated": "Not checked yet",
  "dashboard.lastChecked": "Checked {time}",
  "dashboard.refreshCadence": "Every 30s while active · every 2 min while idle",
  "account.filter": "Account filter",
  "account.all": "All accounts",
  "activity.updated": "Updated {time}",
  "history.label": "Earlier runs",
  "history.lead": "Recent results are shown first",
  "history.help":
    "Select a target to load older history. The time range and actor filters still apply.",
  "history.target": "History target",
  "history.selectTarget": "Select a target",
  "history.loading": "Loading history…",
  "history.loadMore": "View earlier runs",
  "history.end":
    "There is no more history for this target, or you have reached the platform's available history limit.",
  "accountPicker.defaultAccount": "{provider} account",
  "accountPicker.cliSession": "{provider} CLI session",
  "accountPicker.unavailable": "Connection unavailable",
  "accountPicker.immediate": "Account changes apply immediately.",
  "accountPicker.choose": "Account to connect",
  "accountPicker.add": "Add account",
  "accountPicker.removeAria": "Remove this account connection",
  "accountPicker.removeTitle": "Remove ‘{label}’?",
  "accountPicker.removeBody":
    "This connection and its {count} saved targets will be removed immediately. Unsaved target selections will be discarded.",
  "accountPicker.remove": "Remove connection",
  "accountPicker.localSession": "CLI session on this device",
  "accountPicker.connectCli": "Connect CLI session",
  "accountPicker.awsProfile": "Connect an AWS profile",
  "accountPicker.separateAccount": "Connect another account",
  "accountPicker.name": "Account name",
  "accountPicker.namePlaceholder": "Personal / Work",
  "accountPicker.profile": "AWS profile",
  "accountPicker.loadProfiles": "Load local profiles",
  "accountPicker.addProfile": "Add profile connection",
  "accountPicker.addAccount": "Add account connection",
  "accountPicker.loginHelp":
    "Connect this account using the authentication options below. Tokens stay in this device’s secure credential store.",
  "tray.summary": "Run summary",
  "tray.close": "Close panel",
  "tray.preview": "Preview · Sample data",
  "tray.scope": "{scope} · {visible}/{total} targets",
  "tray.mine": "My runs",
  "tray.all": "All runs",
  "tray.checkScope": "Review in dashboard",
  "tray.moreInDashboard": "View {count} more in dashboard",
  "tray.pending":
    "Showing recent results · checking {count} targets for in-progress runs",
  "tray.issues": "{count} notices · some information may be delayed",
  "tray.checkConnection": "Check your connection status.",
  "tray.connecting": "Checking connections…",
  "tray.onboardingTitle": "Choose projects to monitor.",
  "tray.onboardingBody":
    "GitHub · Vercel · AWS\nUses existing sign-ins on this device.",
  "tray.connectDashboard": "Connect from dashboard",
  "tray.noVisibleTargets":
    "All targets are hidden. Review the menu bar display settings in the dashboard.",
  "tray.noResults":
    "No results match the actor, target, and time filters. Review the display scope and notices in the dashboard.",
  "tray.activeEmpty": "No in-progress runs within this display scope.",
  "tray.completedEmpty":
    "No recently completed runs match the selected range and display scope.",
  "tray.sectionsHidden": "Choose which sections to show from the dashboard.",
  "tray.unresolvedEmpty": "No runs need attention.",
  "tray.checking": "Checking…",
  "tray.checked": "Checked {time}",
  "tray.waiting": "Waiting for connection",
  "tray.openDashboard": "Open dashboard",
  "connections.addedDiscoveryFailed":
    "The connection was added, but its targets could not be loaded. {error}",
  "connections.saveFailed": "Could not save the settings. Please try again.",
  "connections.title": "Service connections",
  "connections.subtitle":
    "Uses existing authentication on this device in read-only mode.",
  "connections.close": "Close settings",
  "connections.content": "Service connection settings",
  "connections.searchPlatform": "Search platforms",
  "connections.services": "Connected services",
  "connections.noPlatforms": "No matching platforms.",
  "connections.providerSettings": "{provider} connection settings",
  "connections.guideTitle": "How to connect",
  "connections.prepare": "Prepare connection",
  "connections.reuseSelected":
    "Ratelline will reuse the existing credentials for ‘{label}’. You do not need to sign in again.",
  "connections.awsSelected":
    "Confirm the ‘{profile}’ profile and region, then use ‘SSO sign in’ in the app. To sign in from a terminal, run {command}.",
  "connections.awsAdd": "Load or add an existing AWS profile.",
  "connections.appLogin":
    "Use ‘Sign in with browser’ to authenticate only this connection. Your default {provider} CLI session will not change.",
  "connections.reuseCli":
    "The selected CLI connection will be reused. If it is missing or expired, run {command} in a terminal.",
  "connections.completeCli":
    "Run {command} in a terminal, then select ‘Connect CLI session’.",
  "connections.verify": "Verify connection",
  "connections.verifyHelp": "Load targets available through this connection.",
  "connections.selectSave": "Select and save targets",
  "connections.selectSaveHelp":
    "Select only the targets you want to monitor, then save.",
  "connections.readOnly":
    "Ratelline only reads run status. It cannot change remote services.",
  "connections.checkConnection": "Verify the connection",
  "connections.tokenNotNeeded":
    "Existing authentication will be used. No token entry is needed.",
  "connections.loginHelp":
    "Use the button below to sign in and load available targets.",
  "connections.profile": "Profile",
  "connections.region": "Region",
  "connections.verifyTargets": "Verify connection and targets",
  "connections.ssoLogin": "SSO sign in",
  "connections.browserLogin": "Sign in with browser",
  "connections.openAuth": "Open authentication page",
  "connections.loadTargets": "Load targets",
  "connections.targets": "Monitoring targets",
  "connections.searchTargets": "Search targets",
  "connections.searchByName": "Search by name",
  "connections.selectAllVisible": "Select all results",
  "connections.deselectAllVisible": "Deselect all results",
  "connections.noTargets":
    "No targets are available. Check the account, permissions, and region.",
  "connections.verifyFirst":
    "Verify the connection to see its available targets here.",
  "connections.actorHelp":
    "Actor information may be unavailable. If an item is missing from ‘My runs’, check All runs.",
  "connections.saveTargets": "Save monitoring targets",
  "status.queued": "Queued",
  "status.running": "Running",
  "status.waiting": "Waiting for approval",
  "status.success": "Succeeded",
  "status.failed": "Failed",
  "status.canceled": "Canceled",
  "status.skipped": "Skipped",
  "status.unknown": "Status unavailable",
  "platform.github": "Choose an account signed in to the installed GitHub CLI.",
  "platform.vercel":
    "Uses the installed Vercel CLI, with separate sign-in for additional accounts.",
  "platform.aws": "Uses the selected AWS profile and region.",
  "platform.cliSession":
    "Uses the signed-in {provider} CLI session on this device.",
  "platform.existingSession": "Uses an existing CLI session on this device.",
  "platform.cloudflarePagesSummary":
    "Wrangler CLI does not reliably provide timestamps and status for Pages deployments.",
  "platform.cloudflarePagesUnsupported":
    "View Cloudflare Pages deployment results in GitHub Actions or GitLab CI.",
  "platform.cloudflareWorkersSummary":
    "Wrangler CLI cannot automatically discover every Workers deployment target in an account.",
  "platform.cloudflareWorkersUnsupported":
    "View Cloudflare Workers deployment results in GitHub Actions or GitLab CI.",
  "platform.easSummary":
    "EAS CLI cannot automatically discover every Expo project in an account.",
  "platform.easUnsupported":
    "Direct EAS Build and Update lookup requires more project connection data. Builds run in CI can be viewed in GitHub Actions or GitLab CI.",
  "platform.firebaseSummary":
    "A Firebase CLI session alone does not provide reliable deployment history.",
  "platform.firebaseUnsupported":
    "View Firebase deployment results in GitHub Actions or GitLab CI.",
  "platform.supabaseSummary":
    "Supabase CLI does not provide reliable deployment history.",
  "platform.supabaseUnsupported":
    "View Supabase Edge Functions deployment status in GitHub Actions or GitLab CI.",
  "platform.fastlaneSummary": "fastlane is not a cloud deployment service.",
  "platform.fastlaneUnsupported":
    "View fastlane run results in GitHub Actions or GitLab CI.",
} satisfies Record<TranslationKey, string>;
