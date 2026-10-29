import { preferencesSchema, panelSettingsSchema } from "./domain";
import type { Account, Activity, Provider, Snapshot, Target } from "./domain";
import type { GithubAccountAccess, GithubCliAccount } from "./cliAccounts";

/** Fictional gh accounts used by the browser preview. */
export const DEMO_GITHUB_ACCOUNTS: GithubCliAccount[] = [
  { login: "orbit-developer", active: true, state: "ok" },
  { login: "orbit-work", active: false, state: "ok" },
];

export const DEMO_ACCOUNTS: Account[] = [
  {
    id: "github:local",
    provider: "github",
    label: "GitHub CLI",
    mode: "local",
    cliUsername: "orbit-developer",
  },
  {
    id: "vercel:local",
    provider: "vercel",
    label: "Vercel CLI",
    mode: "local",
  },
  {
    id: "aws:development",
    provider: "aws",
    label: "Development",
    mode: "local",
    profile: "development",
  },
];

const ago = (minutes: number) =>
  new Date(Date.now() - minutes * 60_000).toISOString();
export const DEMO_TARGETS: Target[] = [
  {
    id: "github:orbit/web",
    connectionId: "github:local",
    provider: "github",
    name: "orbit/web",
    description: "웹 애플리케이션",
    metadata: { repo: "orbit/web" },
  },
  {
    id: "vercel:orbit-web",
    connectionId: "vercel:local",
    provider: "vercel",
    name: "orbit-web",
    description: "Orbit 팀",
    metadata: { projectId: "prj_demo", teamId: "team_demo" },
  },
  {
    id: "aws:api",
    connectionId: "aws:development",
    provider: "aws",
    name: "orbit-api",
    description: "development · ap-northeast-2",
    metadata: { profile: "development", region: "ap-northeast-2" },
  },
];
function activity(
  id: string,
  provider: Provider,
  title: string,
  status: Activity["status"],
  minutes: number,
  ownership: Activity["ownership"] = "mine",
): Activity {
  return {
    id,
    provider,
    targetId: DEMO_TARGETS.find((target) => target.provider === provider)?.id,
    connectionId: provider === "aws" ? "aws:development" : `${provider}:local`,
    title,
    workflowName:
      provider === "github"
        ? id === "g2"
          ? "CI"
          : "Deploy"
        : provider === "vercel"
          ? "orbit-web"
          : null,
    status,
    ownership,
    kind: provider === "github" ? "workflow" : "deployment",
    project:
      provider === "github"
        ? "orbit/web"
        : provider === "vercel"
          ? "orbit-web"
          : "orbit-api",
    branch: "main",
    actor: ownership === "mine" ? "alex" : ownership === "other" ? "sam" : null,
    rawStatus: status.toUpperCase(),
    url:
      provider === "github"
        ? "https://github.com"
        : provider === "vercel"
          ? "https://vercel.com"
          : "https://console.aws.amazon.com",
    startedAt: ago(minutes),
    updatedAt: ago(Math.max(minutes - 2, 0)),
    finishedAt: ["success", "failed", "canceled"].includes(status)
      ? ago(Math.max(minutes - 2, 0))
      : null,
  };
}
export function demoSnapshot(): Snapshot {
  return {
    settings: {
      hiddenActivities: [],
      scope: "mine",
      displayScopes: { providers: {}, targets: {} },
      historyHours: 24,
      panel: panelSettingsSchema.parse({}),
      preferences: preferencesSchema.parse({}),
      targets: DEMO_TARGETS,
      accounts: DEMO_ACCOUNTS,
    },
    connections: [
      {
        connectionId: "github:local",
        provider: "github",
        status: "connected",
        account: "alex",
      },
      {
        connectionId: "vercel:local",
        provider: "vercel",
        status: "connected",
        account: "alex / Orbit",
      },
      {
        connectionId: "aws:development",
        provider: "aws",
        status: "connected",
        account: "development",
      },
    ],
    activities: [
      activity("g1", "github", "Deploy production", "running", 4),
      activity("v1", "vercel", "Improve workspace navigation", "running", 2),
      activity(
        "a1",
        "aws",
        "orbit-api · 스택 업데이트",
        "running",
        7,
        "unknown",
      ),
      activity("g2", "github", "Test & typecheck", "success", 16),
      activity(
        "v2",
        "vercel",
        "Fix notification preferences",
        "success",
        32,
        "other",
      ),
      activity("g3", "github", "Preview deployment", "failed", 68),
      activity("v3", "vercel", "Update account settings", "success", 110),
    ],
    issues: [],
    refreshing: false,
    history: [],
    pendingTargets: [],
    queryHealth: DEMO_TARGETS.map((target) => ({
      provider: target.provider,
      connectionId: target.connectionId,
      targetId: target.id,
      status: "success",
      lastAttemptAt: ago(0),
      lastSuccessfulAt: ago(0),
    })),
    pollIntervalSeconds: 30,
    lastUpdatedAt: ago(0),
  };
}

/**
 * A preview state with one GitHub connection that has no gh account selected
 * and a signed-out Vercel connection. Open the preview with `?demo=issues`.
 */
export function demoIssueSnapshot(): Snapshot {
  const base = demoSnapshot();
  const broken = new Set(["github:local", "vercel:local"]);
  return {
    ...base,
    settings: {
      ...base.settings,
      accounts: base.settings.accounts.map((account) =>
        account.id === "github:local"
          ? { ...account, cliUsername: null }
          : account,
      ),
    },
    connections: base.connections.map((connection) =>
      connection.connectionId === "github:local"
        ? {
            ...connection,
            status: "error",
            kind: "needsAccount",
            message: "Choose which GitHub CLI account this connection uses.",
          }
        : connection.connectionId === "vercel:local"
          ? {
              ...connection,
              status: "signed-out",
              kind: "signedOut",
              account: null,
              message: "Sign in to Vercel to continue.",
            }
          : connection,
    ),
    issues: [
      {
        provider: "github",
        connectionId: "github:local",
        targetId: null,
        kind: "needsAccount",
        message: "Choose which GitHub CLI account this connection uses.",
      },
      {
        provider: "vercel",
        connectionId: "vercel:local",
        targetId: null,
        kind: "signedOut",
        message: "Sign in to Vercel to continue.",
      },
    ],
    queryHealth: base.queryHealth.map((health) =>
      broken.has(health.connectionId)
        ? { ...health, status: "error", lastSuccessfulAt: null }
        : health,
    ),
  };
}

/** Preview diagnosis: orbit-developer can read every demo target, orbit-work none. */
export function previewGithubAccess(
  snapshot: Snapshot,
  connectionId: string,
): GithubAccountAccess[] {
  const targets = snapshot.settings.targets
    .filter((target) => target.connectionId === connectionId)
    .map((target) => target.id);
  const provider = snapshot.settings.accounts.find(
    (item) => item.id === connectionId,
  )?.provider;
  return DEMO_GITHUB_ACCOUNTS.map((account) => {
    const inaccessibleTargets =
      account.login === "orbit-developer" ? [] : targets;
    const usedBy = snapshot.settings.accounts.find(
      (item) =>
        item.id !== connectionId &&
        item.provider === provider &&
        item.cliUsername === account.login,
    );
    return {
      ...account,
      canAccessAll: inaccessibleTargets.length === 0,
      inaccessibleTargets,
      checkFailed: null,
      identityMismatch: false,
      usedByConnectionId: usedBy?.id ?? null,
    };
  });
}
