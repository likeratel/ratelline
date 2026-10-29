import { formatRelativeTime, useI18n } from "../i18n";
import { summarizeQueryHealth, type QueryState } from "../queryHealth";
import type { Connection, Provider, Snapshot, Target } from "../domain";
import { connectionProgressCopy } from "../connectionProgressCopy";
import { isGithubProvider } from "../cliAccounts";

export type ConnectionProgressAction =
  | "account"
  | "authentication"
  | "githubAccount"
  | "projects"
  | "query"
  | "verify";
type RowState = "complete" | "waiting" | "warning" | "checking";

type ProgressRow = {
  label: string;
  detail: string;
  state: RowState;
  action?: ConnectionProgressAction;
  actionLabel?: string;
};

const statusSymbol: Record<RowState, string> = {
  complete: "✓",
  waiting: "—",
  warning: "▲",
  checking: "↻",
};

const queryRowState = (state: QueryState): RowState =>
  state === "success"
    ? "complete"
    : state === "checking"
      ? "checking"
      : state === "unknown"
        ? "waiting"
        : "warning";

export function ConnectionProgress({
  snapshot,
  provider,
  accountPresent,
  connection,
  selectedTargets,
  selectionPending,
  requiresCli,
  verificationError,
  onAction,
}: {
  snapshot: Snapshot;
  provider: Provider;
  accountPresent: boolean;
  connection: Connection | undefined;
  selectedTargets: readonly Target[];
  selectionPending: boolean;
  requiresCli: boolean;
  verificationError?: string;
  onAction: (action: ConnectionProgressAction) => void;
}) {
  const { locale } = useI18n();
  const c = (
    key: Parameters<typeof connectionProgressCopy>[1],
    values?: Readonly<Record<string, string | number>>,
  ) => connectionProgressCopy(locale, key, values);
  const health = summarizeQueryHealth(snapshot, selectedTargets);
  const isTokenProvider = [
    "bitbucket",
    "cloudflare-pages",
    "cloudflare-workers",
  ].includes(provider);
  const tool = toolProgress({
    c,
    isTokenProvider,
    accountPresent,
    requiresCli,
    connection,
    provider,
  });
  const account = accountProgress({
    c,
    accountPresent,
    connection,
    verificationError,
    isTokenProvider,
    github: isGithubProvider(provider),
  });
  const project: ProgressRow = selectionPending
    ? {
        label: c("project"),
        detail: c("projectsSaving"),
        state: "checking",
      }
    : selectedTargets.length
      ? {
          label: c("project"),
          detail: c("projectsSaved", { count: selectedTargets.length }),
          state: "complete",
        }
      : {
          label: c("project"),
          detail: c("projectsNeeded"),
          state: "waiting",
          action: "projects",
          actionLabel: c("selectProjects"),
        };
  const query = queryProgress(
    c,
    locale,
    health.state,
    health.lastSuccessfulAt,
    selectedTargets.length,
    selectionPending,
  );
  const rows = [tool, account, project, query];

  return (
    <section className="connection-progress" aria-label={c("title")}>
      <h3>{c("title")}</h3>
      <ol>
        {rows.map((row) => (
          <li
            aria-label={`${row.label}: ${c(row.state)}. ${row.detail}`}
            className={`connection-progress-row ${row.state}`}
            key={row.label}
          >
            <span className="connection-progress-mark" aria-hidden="true">
              {statusSymbol[row.state]}
            </span>
            <div>
              <strong>{row.label}</strong>
              <p>{row.detail}</p>
              <details className="connection-progress-detail">
                <summary aria-label={c("detail", { label: row.label })}>
                  ⓘ
                </summary>
                <p>{row.detail}</p>
              </details>
            </div>
            {row.action && row.actionLabel && (
              <button
                className="text-button"
                type="button"
                onClick={() => onAction(row.action!)}
              >
                {row.actionLabel}
              </button>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}

function toolProgress({
  c,
  isTokenProvider,
  accountPresent,
  requiresCli,
  connection,
  provider,
}: {
  c: (key: Parameters<typeof connectionProgressCopy>[1]) => string;
  isTokenProvider: boolean;
  accountPresent: boolean;
  requiresCli: boolean;
  connection: Connection | undefined;
  provider: Provider;
}): ProgressRow {
  if (isTokenProvider) {
    if (!accountPresent) {
      return {
        label: c("credential"),
        detail: c("credentialNeeded"),
        state: "waiting",
        action: "account",
        actionLabel: c("manageAccount"),
      };
    }
    return connection?.status === "connected"
      ? {
          label: c("credential"),
          detail: c("credentialVerified"),
          state: "complete",
        }
      : {
          label: c("credential"),
          detail: c("credentialRegistered"),
          state: "warning",
          action: "account",
          actionLabel: c("manageAccount"),
        };
  }
  if (provider === "aws") {
    return accountPresent
      ? { label: c("tool"), detail: c("profileReady"), state: "complete" }
      : {
          label: c("tool"),
          detail: c("profileNeeded"),
          state: "waiting",
          action: "account",
          actionLabel: c("manageAccount"),
        };
  }
  if (!requiresCli)
    return { label: c("tool"), detail: c("cliReady"), state: "complete" };
  if (connection?.status === "missing-cli") {
    return {
      label: c("tool"),
      detail: c("cliMissing"),
      state: "warning",
      action: "authentication",
      actionLabel: c("manageAccount"),
    };
  }
  if (
    connection?.status === "connected" ||
    connection?.status === "signed-out" ||
    Boolean(connection?.cliVersion)
  )
    return { label: c("tool"), detail: c("cliReady"), state: "complete" };
  return {
    label: c("tool"),
    detail: c("cliUnknown"),
    state: "waiting",
    action: accountPresent ? "authentication" : "account",
    actionLabel: c("manageAccount"),
  };
}

function accountProgress({
  c,
  accountPresent,
  connection,
  verificationError,
  isTokenProvider,
  github,
}: {
  c: (key: Parameters<typeof connectionProgressCopy>[1]) => string;
  accountPresent: boolean;
  connection: Connection | undefined;
  verificationError?: string;
  isTokenProvider: boolean;
  github: boolean;
}): ProgressRow {
  if (!accountPresent) {
    return {
      label: c("account"),
      detail: c("accountNeeded"),
      state: "waiting",
      action: "account",
      actionLabel: c("manageAccount"),
    };
  }
  if (connection?.status === "connected" && !verificationError)
    return {
      label: c("account"),
      detail: c("accountVerified"),
      state: "complete",
    };
  if (
    github &&
    connection?.status !== "connected" &&
    ["needsAccount", "noAccess", "accountMismatch"].includes(
      connection?.kind ?? "",
    )
  )
    return {
      label: c("account"),
      detail: c("githubAccountNeeded"),
      state: "warning",
      action: "githubAccount",
      actionLabel: c("chooseGithubAccount"),
    };
  const needsAuthentication =
    !isTokenProvider &&
    (connection?.status === "missing-cli" ||
      connection?.status === "signed-out");
  return {
    label: c("account"),
    detail:
      verificationError || connection?.status === "error"
        ? c("accountFailed")
        : c("accountCheck"),
    state:
      verificationError || connection?.status === "error"
        ? "warning"
        : "waiting",
    action: needsAuthentication ? "authentication" : "verify",
    actionLabel: needsAuthentication ? c("manageAccount") : c("checkAccount"),
  };
}

function queryProgress(
  c: (
    key: Parameters<typeof connectionProgressCopy>[1],
    values?: Readonly<Record<string, string | number>>,
  ) => string,
  locale: Parameters<typeof formatRelativeTime>[1],
  state: QueryState,
  lastSuccessfulAt: string | null,
  selectedTargetCount: number,
  selectionPending: boolean,
): ProgressRow {
  if (selectionPending) {
    return {
      label: c("query"),
      detail: c("queryAwaitingProjectSave"),
      state: "checking",
    };
  }
  if (!selectedTargetCount) {
    return {
      label: c("query"),
      detail: c("projectsNeeded"),
      state: "waiting",
      action: "projects",
      actionLabel: c("selectProjects"),
    };
  }
  const details: Record<
    QueryState,
    Parameters<typeof connectionProgressCopy>[1]
  > = {
    success: "queryComplete",
    checking: "queryChecking",
    unknown: "queryUnknown",
    partial: "queryPartial",
    error: "queryFailed",
    stale: "queryStale",
  };
  const detail = lastSuccessfulAt
    ? `${c(details[state])} ${c("lastSuccess", {
        time: formatRelativeTime(lastSuccessfulAt, locale),
      })}`
    : c(details[state]);
  const complete = state === "success";
  return {
    label: c("query"),
    detail,
    state: queryRowState(state),
    ...(complete || state === "checking"
      ? {}
      : { action: "query" as const, actionLabel: c("retryQuery") }),
  };
}
