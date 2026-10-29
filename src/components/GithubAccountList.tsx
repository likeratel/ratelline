import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { bridge, errorMessage, isPreview } from "../bridge";
import { cliCopy } from "../cliCopy";
import type {
  GithubAccountAccess,
  GithubCliAccount,
  GithubProvider,
} from "../cliAccounts";
import { connectionFixCopy } from "../connectionFixCopy";
import { GITHUB_SIGN_IN_COMMAND } from "../connectionFixes";
import { useI18n, type Locale } from "../i18n";
import { CommandCopy } from "./CommandCopy";
import { IdentityChangeConfirm } from "./IdentityChangeConfirm";
import "../styles/connection-fixes.css";

type Props = {
  provider: GithubProvider;
  connectionId: string;
  username?: string | null;
  targetCount: number;
  busy: boolean;
  onBusyChange: (busy: boolean) => void;
  onConnected: (login: string) => Promise<void>;
};

const sameLogin = (left?: string | null, right?: string | null) =>
  Boolean(left && right && left.toLowerCase() === right.toLowerCase());

function accessLabel(
  locale: Locale,
  account: GithubCliAccount,
  access: GithubAccountAccess | undefined,
  diagnosing: boolean,
  targetCount: number,
): string {
  if (account.state === "signedOut" || access?.state === "signedOut")
    return connectionFixCopy(locale, "stateSignedOut");
  if (!access)
    return connectionFixCopy(
      locale,
      diagnosing ? "checkingAccess" : "accessUnknown",
    );
  if (access.usedByConnectionId)
    return connectionFixCopy(locale, "usedElsewhere");
  if (access.identityMismatch)
    return connectionFixCopy(locale, "identityMismatch");
  if (access.checkFailed) return connectionFixCopy(locale, "accessUnknown");
  if (access.inaccessibleTargets.length)
    return connectionFixCopy(locale, "accessMissing", {
      count: access.inaccessibleTargets.length,
    });
  return targetCount
    ? connectionFixCopy(locale, "accessAll", { count: targetCount })
    : connectionFixCopy(locale, "accessVerified");
}

export function GithubAccountList({
  provider,
  connectionId,
  username,
  targetCount,
  busy,
  onBusyChange,
  onConnected,
}: Props) {
  const { locale } = useI18n();
  const [accounts, setAccounts] = useState<GithubCliAccount[] | null>(null);
  const [access, setAccess] = useState<GithubAccountAccess[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [diagnosing, setDiagnosing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    onBusyChange(saving);
    return () => onBusyChange(false);
  }, [saving, onBusyChange]);
  const load = useCallback(
    async (isCurrent: () => boolean = () => true) => {
      setLoading(true);
      setError(null);
      setAccess(null);
      try {
        const listed = await bridge.githubCliAccountStates();
        if (!isCurrent()) return;
        setAccounts(listed);
        setLoading(false);
        if (!listed.length) return;
        setDiagnosing(true);
        const diagnosed = await bridge.diagnoseGithubConnection(connectionId);
        if (isCurrent()) setAccess(diagnosed);
      } catch (cause) {
        if (isCurrent()) setError(errorMessage(cause));
      } finally {
        if (isCurrent()) {
          setLoading(false);
          setDiagnosing(false);
        }
      }
    },
    [connectionId],
  );
  useEffect(() => {
    let current = true;
    void load(() => current);
    return () => {
      current = false;
    };
  }, [load]);
  const [confirming, setConfirming] = useState<string | null>(null);
  const use = async (login: string, allowIdentityChange = false) => {
    setSaving(true);
    setError(null);
    try {
      await bridge.connectGithubCliAccount(
        provider,
        connectionId,
        login,
        allowIdentityChange,
      );
      setConfirming(null);
      await onConnected(login);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  };
  const disabled = busy || saving || loading;
  return (
    <section
      id="github-account-list"
      className="github-account-list"
      aria-label={connectionFixCopy(locale, "accountListTitle")}
      tabIndex={-1}
    >
      <header>
        <strong>{connectionFixCopy(locale, "accountListTitle")}</strong>
        <button
          type="button"
          className="icon-button"
          title={cliCopy(locale, "refresh")}
          aria-label={cliCopy(locale, "refresh")}
          disabled={disabled}
          onClick={() => void load()}
        >
          <RefreshCw
            size={15}
            className={loading || diagnosing ? "spin" : ""}
          />
        </button>
      </header>
      {!username && (
        <p className="github-account-callout">
          {connectionFixCopy(locale, "noAccountSelected")}
        </p>
      )}
      {accounts && accounts.length > 0 && (
        <ul>
          {accounts.map((account) => {
            const result = access?.find((item) =>
              sameLogin(item.login, account.login),
            );
            const current = sameLogin(username, account.login);
            const usable =
              account.state === "ok" &&
              result?.state !== "signedOut" &&
              !result?.usedByConnectionId;
            return (
              <li className="github-account-row" key={account.login}>
                <div>
                  <strong>{account.login}</strong>
                  {account.active && (
                    <span className="account-badge">
                      {connectionFixCopy(locale, "activeAccount")}
                    </span>
                  )}
                  <small
                    className={
                      result?.canAccessAll ? "access ok" : "access warning"
                    }
                  >
                    {accessLabel(
                      locale,
                      account,
                      result,
                      diagnosing,
                      targetCount,
                    )}
                  </small>
                </div>
                {current ? (
                  <span className="account-badge current">
                    {connectionFixCopy(locale, "inUse")}
                  </span>
                ) : (
                  <button
                    type="button"
                    className={`button ${result?.canAccessAll ? "primary" : ""}`}
                    aria-label={connectionFixCopy(locale, "useNamedAccount", {
                      login: account.login,
                    })}
                    disabled={disabled || !usable || Boolean(confirming)}
                    onClick={() =>
                      result?.identityMismatch
                        ? setConfirming(account.login)
                        : void use(account.login)
                    }
                  >
                    {connectionFixCopy(locale, "useAccount")}
                  </button>
                )}
                {confirming === account.login && (
                  <IdentityChangeConfirm
                    login={account.login}
                    busy={disabled}
                    onConfirm={() => void use(account.login, true)}
                    onCancel={() => {
                      setConfirming(null);
                      window.requestAnimationFrame(() =>
                        document
                          .querySelector<HTMLElement>(
                            `#github-account-list button[aria-label="${CSS.escape(
                              connectionFixCopy(locale, "useNamedAccount", {
                                login: account.login,
                              }),
                            )}"]`,
                          )
                          ?.focus(),
                      );
                    }}
                  />
                )}
              </li>
            );
          })}
        </ul>
      )}
      {!loading && !error && accounts?.length === 0 && (
        <p role="status">{cliCopy(locale, "noAccounts")}</p>
      )}
      {error && <p role="alert">{error}</p>}
      <p className="field-help">
        {connectionFixCopy(locale, "accountListHelp")}
      </p>
      <p className="field-help">
        {connectionFixCopy(locale, "addAccountHelp")}
      </p>
      <CommandCopy command={GITHUB_SIGN_IN_COMMAND} />
      {isPreview && <p className="field-help">{cliCopy(locale, "preview")}</p>}
    </section>
  );
}

/** Opens the account list of a bound, working GitHub connection. */
export function GithubAccountChangeButton({
  login,
  disabled,
  onOpen,
}: {
  login: string;
  disabled: boolean;
  onOpen: () => void;
}) {
  const { locale } = useI18n();
  return (
    <button
      id="github-account-change"
      className="button"
      disabled={disabled}
      aria-label={`${connectionFixCopy(locale, "changeAccount")} · ${login}`}
      onClick={() => {
        onOpen();
        window.requestAnimationFrame(() =>
          document.getElementById("github-account-list")?.focus(),
        );
      }}
    >
      {connectionFixCopy(locale, "changeAccount")}
    </button>
  );
}
