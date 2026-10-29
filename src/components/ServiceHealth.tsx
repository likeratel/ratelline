import { accountDisplayName } from "../accountNames";
import {
  PROVIDER_NAMES,
  targetKey,
  type Provider,
  type Snapshot,
} from "../domain";
import { formatRelativeTime, useI18n } from "../i18n";
import { queryStateForTarget, summarizeQueryHealth } from "../queryHealth";
import { monitorCopy } from "../monitorCopy";
import { copy, type FeatureKey } from "../featureCopy";
import { ProviderIcon } from "./ProviderIcon";
import { connectionFixes, groupIssues } from "../connectionFixes";
import { actionCopy, causeCopy, connectionFixCopy } from "../connectionFixCopy";
export function ServiceHealth({
  snapshot,
  onConnect,
  accountIds,
  expanded = false,
}: {
  snapshot: Snapshot;
  onConnect: (provider: Provider, connectionId: string) => void;
  accountIds?: ReadonlySet<string>;
  expanded?: boolean;
}) {
  const { locale } = useI18n();
  const accounts = snapshot.settings.accounts.filter(
    (account) =>
      (!accountIds || accountIds.has(account.id)) &&
      snapshot.settings.targets.some(
        (target) => target.connectionId === account.id,
      ),
  );
  const relevantIssues = snapshot.issues.filter((issue) =>
    accounts.some((account) =>
      issue.connectionId
        ? issue.connectionId === account.id
        : issue.provider === account.provider,
    ),
  );
  const needsAttention = (id: string) =>
    ["error", "partial"].includes(
      summarizeQueryHealth(
        snapshot,
        snapshot.settings.targets.filter(
          (target) => target.connectionId === id,
        ),
      ).state,
    ) ||
    snapshot.connections.some(
      (connection) =>
        connection.connectionId === id && connection.status !== "connected",
    ) ||
    relevantIssues.some(
      (issue) =>
        issue.connectionId === id ||
        (!issue.connectionId &&
          accounts.some(
            (account) =>
              account.id === id && account.provider === issue.provider,
          )),
    );
  const fixes = connectionFixes(snapshot);
  const accountIssues = (account: (typeof accounts)[number]) =>
    snapshot.issues.filter(
      (issue) =>
        issue.connectionId === account.id ||
        (!issue.connectionId && issue.provider === account.provider),
    );
  const groupedIssueCount = accounts.reduce(
    (count, account) => count + groupIssues(accountIssues(account)).length,
    0,
  );
  const orderedAccounts = [...accounts].sort(
    (left, right) =>
      Number(needsAttention(right.id)) - Number(needsAttention(left.id)),
  );
  if (!accounts.length) return null;
  return (
    <details className="service-health" open={expanded}>
      <summary>
        {copy(locale, "health")}
        {groupedIssueCount > 0
          ? ` · ${copy(locale, "issuesCount", { count: groupedIssueCount })}`
          : ""}
        {snapshot.refreshing ? ` · ${copy(locale, "checking")}` : ""}
      </summary>
      {orderedAccounts.map((account) => {
        const targets = snapshot.settings.targets.filter(
          (target) => target.connectionId === account.id,
        );
        const pending = targets.filter((target) =>
          snapshot.pendingTargets.includes(targetKey(target)),
        ).length;
        const connection = snapshot.connections.find(
          (item) => item.connectionId === account.id,
        );
        const health = summarizeQueryHealth(snapshot, targets);
        const issues = accountIssues(account);
        const issueGroups = groupIssues(issues);
        const fix = fixes.find((item) => item.connectionId === account.id);
        const key: FeatureKey =
          connection?.status === "signed-out"
            ? "signedOut"
            : connection?.status === "missing-cli"
              ? "missingCli"
              : connection?.status === "error"
                ? "connectionError"
                : pending
                  ? "checking"
                  : issues.length
                    ? "partial"
                    : health.state === "success"
                      ? "checked"
                      : connection?.status === "connected"
                        ? "connected"
                        : "notChecked";
        const help: FeatureKey =
          key === "signedOut"
            ? "loginHelp"
            : key === "missingCli"
              ? "cliHelp"
              : "errorHelp";
        const needsHelp =
          ["error", "partial"].includes(health.state) ||
          issues.length > 0 ||
          ["signedOut", "missingCli", "connectionError"].includes(key);
        return (
          <section className="service-health-row" key={account.id}>
            <div className="service-health-title">
              <ProviderIcon provider={account.provider} />
              <strong>
                {PROVIDER_NAMES[account.provider]} ·{" "}
                {accountDisplayName(
                  account,
                  snapshot.settings,
                  snapshot.connections,
                )}
              </strong>
              <span role="status">
                {["signedOut", "missingCli", "connectionError"].includes(key)
                  ? copy(locale, key)
                  : ["error", "partial", "stale"].includes(health.state)
                    ? monitorCopy(locale, health.state)
                    : copy(locale, key)}
              </span>
            </div>
            {pending > 0 && (
              <p>{copy(locale, "pendingCount", { count: pending })}</p>
            )}
            <div className="query-target-list">
              {targets.map((target) => {
                const result = queryStateForTarget(snapshot, target);
                return (
                  <div className="query-target" key={targetKey(target)}>
                    <strong>{target.name}</strong>
                    <span className={`query-state ${result.state}`}>
                      {monitorCopy(locale, result.state)}
                    </span>
                    <small>
                      {result.lastSuccessfulAt
                        ? `${monitorCopy(locale, "lastSuccess")} · ${formatRelativeTime(result.lastSuccessfulAt, locale)}`
                        : monitorCopy(locale, "noSuccess")}
                    </small>
                  </div>
                );
              })}
            </div>
            {needsHelp && (
              <>
                <p>{fix ? causeCopy(locale, fix.kind) : copy(locale, help)}</p>
                <button
                  className="text-button"
                  onClick={() => onConnect(account.provider, account.id)}
                >
                  {fix
                    ? actionCopy(locale, fix.action)
                    : copy(locale, "reconnect")}
                </button>
                <details>
                  <summary>{copy(locale, "details")}</summary>
                  {connection?.status !== "connected" &&
                    connection?.message && <p>{connection.message}</p>}
                  {issueGroups.map((group) => (
                    <p key={`${group.kind}:${group.messages[0]}`}>
                      {group.kind === "other"
                        ? group.messages[0]
                        : connectionFixCopy(locale, "causeTargets", {
                            cause: causeCopy(locale, group.kind),
                            count: group.connectionWide
                              ? targets.length
                              : group.targetIds.length,
                          })}
                    </p>
                  ))}
                </details>
              </>
            )}
          </section>
        );
      })}
    </details>
  );
}
