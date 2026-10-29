import type {
  Connection,
  Issue,
  IssueKind,
  Provider,
  Snapshot,
} from "./domain";
import { isGithubProvider } from "./cliAccounts";

/** The single primary action offered for a cause. */
export type FixAction =
  | "chooseAccount"
  | "githubSignIn"
  | "providerSignIn"
  | "providerSwitchAccount"
  | "installCli"
  | "retry"
  | "openConnection";

export interface ConnectionFix {
  connectionId: string;
  provider: Provider;
  kind: IssueKind;
  action: FixAction;
  affectedTargets: number;
  /** English provider detail, shown only as a secondary explanation. */
  detail: string | null;
}

export interface IssueGroup {
  kind: IssueKind;
  /** Distinct targets with this cause; null target IDs count every target. */
  targetIds: string[];
  messages: string[];
  connectionWide: boolean;
}

/** Most blocking first: an account problem hides the query errors it causes. */
const KIND_PRIORITY: readonly IssueKind[] = [
  "needsAccount",
  "signedOut",
  "missingCli",
  "unsupportedVersion",
  "accountMismatch",
  "noAccess",
  "rateLimited",
  "network",
  "timedOut",
  "other",
];

const APP_SIGN_IN_PROVIDERS: ReadonlySet<Provider> = new Set(["vercel", "aws"]);

export const kindRank = (kind: IssueKind) => KIND_PRIORITY.indexOf(kind);

/** Older snapshots carry only a status; derive the cause from it. */
const kindForStatus = (status: Connection["status"]): IssueKind =>
  status === "signed-out"
    ? "signedOut"
    : status === "missing-cli"
      ? "missingCli"
      : "other";

export function fixAction(provider: Provider, kind: IssueKind): FixAction {
  const github = isGithubProvider(provider);
  switch (kind) {
    case "needsAccount":
    case "noAccess":
      if (github) return "chooseAccount";
      return provider === "vercel" ? "providerSwitchAccount" : "openConnection";
    case "accountMismatch":
      if (github) return "chooseAccount";
      return APP_SIGN_IN_PROVIDERS.has(provider)
        ? "providerSignIn"
        : "openConnection";
    case "signedOut":
      if (github) return "githubSignIn";
      return APP_SIGN_IN_PROVIDERS.has(provider)
        ? "providerSignIn"
        : "openConnection";
    case "missingCli":
    case "unsupportedVersion":
      return "installCli";
    case "rateLimited":
    case "network":
    case "timedOut":
      return "retry";
    case "other":
      return "openConnection";
  }
}

function issuesFor(
  snapshot: Snapshot,
  connectionId: string,
  provider: Provider,
) {
  return snapshot.issues.filter((issue) =>
    issue.connectionId
      ? issue.connectionId === connectionId
      : issue.provider === provider,
  );
}

function failingConnection(
  snapshot: Snapshot,
  connectionId: string,
): Connection | undefined {
  return snapshot.connections.find(
    (connection) =>
      connection.connectionId === connectionId &&
      connection.status !== "connected",
  );
}

/** Problems of connections that monitor at least one target, most blocking first. */
export function connectionFixes(snapshot: Snapshot): ConnectionFix[] {
  const fixes: ConnectionFix[] = [];
  for (const account of snapshot.settings.accounts) {
    const targets = snapshot.settings.targets.filter(
      (target) => target.connectionId === account.id,
    );
    if (!targets.length) continue;
    const failing = failingConnection(snapshot, account.id);
    if (failing) {
      const kind = failing.kind ?? kindForStatus(failing.status);
      fixes.push({
        connectionId: account.id,
        provider: account.provider,
        kind,
        action: fixAction(account.provider, kind),
        affectedTargets: targets.length,
        detail: failing.message ?? null,
      });
      continue;
    }
    const groups = groupIssues(
      issuesFor(snapshot, account.id, account.provider),
    ).filter((group) => group.kind !== "other");
    const [first] = groups;
    if (!first) continue;
    fixes.push({
      connectionId: account.id,
      provider: account.provider,
      kind: first.kind,
      action: fixAction(account.provider, first.kind),
      affectedTargets: first.connectionWide
        ? targets.length
        : Math.min(first.targetIds.length, targets.length),
      detail: first.messages[0] ?? null,
    });
  }
  return fixes.sort((a, b) => kindRank(a.kind) - kindRank(b.kind));
}

/**
 * Collapses issues with the same classified cause into one row. Unclassified
 * issues keep one row per distinct message so different problems stay visible.
 */
export function groupIssues(issues: readonly Issue[]): IssueGroup[] {
  const groups = new Map<string, IssueGroup>();
  for (const issue of issues) {
    const key = issue.kind === "other" ? `other:${issue.message}` : issue.kind;
    const group = groups.get(key) ?? {
      kind: issue.kind,
      targetIds: [],
      messages: [],
      connectionWide: false,
    };
    groups.set(key, {
      ...group,
      targetIds:
        issue.targetId && !group.targetIds.includes(issue.targetId)
          ? [...group.targetIds, issue.targetId]
          : group.targetIds,
      messages: group.messages.includes(issue.message)
        ? group.messages
        : [...group.messages, issue.message],
      connectionWide: group.connectionWide || !issue.targetId,
    });
  }
  return [...groups.values()].sort(
    (a, b) => kindRank(a.kind) - kindRank(b.kind),
  );
}

export const GITHUB_SIGN_IN_COMMAND =
  "gh auth login --hostname github.com --web";
