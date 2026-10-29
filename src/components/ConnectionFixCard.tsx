import { useEffect, useRef, useState } from "react";
import { AlertCircle, LoaderCircle } from "lucide-react";
import { bridge, errorMessage } from "../bridge";
import {
  accessibleAccounts,
  isGithubProvider,
  type GithubAccountAccess,
} from "../cliAccounts";
import { IdentityChangeConfirm } from "./IdentityChangeConfirm";
import { actionCopy, causeCopy, connectionFixCopy } from "../connectionFixCopy";
import { GITHUB_SIGN_IN_COMMAND, type ConnectionFix } from "../connectionFixes";
import { PROVIDER_NAMES } from "../domain";
import { useI18n } from "../i18n";
import { CommandCopy } from "./CommandCopy";
import { LocalCliHelp } from "./LocalCliHelp";
import { ProviderIcon } from "./ProviderIcon";

type Phase =
  | { kind: "idle" }
  | { kind: "working" }
  | { kind: "signIn" }
  | { kind: "install" }
  | { kind: "switchHelp" }
  | { kind: "choose"; accounts: GithubAccountAccess[] }
  | { kind: "confirm"; login: string }
  | { kind: "noAccount"; reason: NoAccountReason }
  | { kind: "switched"; login: string };

type NoAccountReason = "inconclusive" | "noAccessibleAccount";

export interface FixHandlers {
  onReload: () => Promise<void>;
  onRefresh: () => Promise<void>;
  onSignIn: (fix: ConnectionFix) => void;
  onOpenConnection: (fix: ConnectionFix) => void;
  /** Called once the account is saved; the card may disappear right after. */
  onFixed: (fix: ConnectionFix, login: string) => void;
}

type Props = FixHandlers & {
  fix: ConnectionFix;
  accountName: string;
  busy: boolean;
  focusRequest?: number;
};

export function ConnectionFixCard({
  fix,
  accountName,
  busy,
  focusRequest,
  onReload,
  onRefresh,
  onSignIn,
  onOpenConnection,
  onFixed,
}: Props) {
  const { locale } = useI18n();
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [error, setError] = useState<string | null>(null);
  const primary = useRef<HTMLButtonElement>(null);
  const followUp = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (focusRequest) primary.current?.focus();
  }, [focusRequest]);
  useEffect(() => {
    if (
      phase.kind !== "idle" &&
      phase.kind !== "working" &&
      phase.kind !== "confirm"
    )
      followUp.current?.focus();
  }, [phase]);
  const working = busy || phase.kind === "working";

  const run = async (task: () => Promise<Phase>) => {
    setError(null);
    setPhase({ kind: "working" });
    try {
      setPhase(await task());
    } catch (cause) {
      setError(errorMessage(cause));
      setPhase({ kind: "idle" });
    }
  };
  const bind = async (
    login: string,
    allowIdentityChange = false,
  ): Promise<Phase> => {
    if (!isGithubProvider(fix.provider)) return { kind: "idle" };
    await bridge.connectGithubCliAccount(
      fix.provider,
      fix.connectionId,
      login,
      allowIdentityChange,
    );
    await onReload();
    onFixed(fix, login);
    await onRefresh();
    return { kind: "switched", login };
  };
  // Another GitHub user's account replaces the saved identity only after consent.
  const pick = (account: GithubAccountAccess): Promise<Phase> | Phase =>
    account.identityMismatch
      ? { kind: "confirm", login: account.login }
      : bind(account.login);
  const cancelConfirm = () => {
    setPhase({ kind: "idle" });
    window.requestAnimationFrame(() => primary.current?.focus());
  };
  const chooseAccount = () =>
    run(async () => {
      const results = await bridge.diagnoseGithubConnection(fix.connectionId);
      const accounts = accessibleAccounts(results);
      if (accounts.length === 1) return pick(accounts[0]);
      if (accounts.length > 1) return { kind: "choose", accounts };
      return {
        kind: "noAccount",
        reason: results.some((item) => Boolean(item.checkFailed))
          ? "inconclusive"
          : "noAccessibleAccount",
      };
    });
  const recheck = () =>
    run(async () => {
      await onRefresh();
      return { kind: "idle" };
    });
  const start = () => {
    switch (fix.action) {
      case "chooseAccount":
        return void chooseAccount();
      case "githubSignIn":
        return setPhase({ kind: "signIn" });
      case "installCli":
        return setPhase({ kind: "install" });
      case "retry":
        return void recheck();
      case "providerSignIn":
        return onSignIn(fix);
      case "providerSwitchAccount":
        setPhase({ kind: "switchHelp" });
        return onSignIn(fix);
      case "openConnection":
        return onOpenConnection(fix);
    }
  };

  const titleId = `fix-${fix.connectionId.replace(/[^A-Za-z0-9-]/g, "-")}`;
  return (
    <li className="connection-fix-card" aria-labelledby={titleId}>
      <div className="connection-fix-heading">
        <ProviderIcon provider={fix.provider} />
        <strong id={titleId}>
          {accountName === PROVIDER_NAMES[fix.provider]
            ? accountName
            : `${PROVIDER_NAMES[fix.provider]} · ${accountName}`}
        </strong>
        <span className="connection-fix-count">
          {connectionFixCopy(locale, "affected", {
            count: fix.affectedTargets,
          })}
        </span>
      </div>
      <p className="connection-fix-cause">
        <AlertCircle size={14} aria-hidden="true" />
        {causeCopy(locale, fix.kind)}
      </p>
      {fix.detail && (
        <details className="connection-fix-detail">
          <summary>{connectionFixCopy(locale, "details")}</summary>
          <p>{fix.detail}</p>
        </details>
      )}
      <div className="connection-fix-actions">
        <button
          ref={primary}
          type="button"
          className="button primary"
          disabled={working}
          onClick={start}
        >
          {phase.kind === "working" && (
            <LoaderCircle size={14} className="spin" aria-hidden="true" />
          )}
          {actionCopy(locale, fix.action)}
        </button>
      </div>
      {phase.kind === "working" && fix.action === "chooseAccount" && (
        <p role="status" className="field-help">
          {connectionFixCopy(locale, "checkingAccess")}
        </p>
      )}
      {phase.kind === "choose" && (
        <div className="connection-fix-followup" role="group">
          <p>{connectionFixCopy(locale, "chooseAmong")}</p>
          {phase.accounts.map((account, index) => (
            <button
              key={account.login}
              ref={index === 0 ? followUp : undefined}
              type="button"
              className="button"
              disabled={busy}
              onClick={() => void run(async () => pick(account))}
            >
              {connectionFixCopy(locale, "useNamedAccount", {
                login: account.login,
              })}
            </button>
          ))}
        </div>
      )}
      {(phase.kind === "signIn" || phase.kind === "noAccount") && (
        <div className="connection-fix-followup">
          <p>
            {phase.kind === "noAccount"
              ? connectionFixCopy(locale, phase.reason)
              : connectionFixCopy(locale, "githubSignInHelp")}
          </p>
          {phase.kind === "noAccount" && (
            <p>{connectionFixCopy(locale, "githubSignInHelp")}</p>
          )}
          <CommandCopy ref={followUp} command={GITHUB_SIGN_IN_COMMAND} />
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() =>
              void (fix.action === "chooseAccount"
                ? chooseAccount()
                : recheck())
            }
          >
            {connectionFixCopy(locale, "recheck")}
          </button>
        </div>
      )}
      {phase.kind === "confirm" && (
        <IdentityChangeConfirm
          login={phase.login}
          busy={busy}
          onConfirm={() => void run(() => bind(phase.login, true))}
          onCancel={cancelConfirm}
        />
      )}
      {phase.kind === "install" && (
        <div className="connection-fix-followup">
          <LocalCliHelp provider={fix.provider} open />
          <button
            ref={followUp}
            type="button"
            className="button"
            disabled={busy}
            onClick={() => void recheck()}
          >
            {connectionFixCopy(locale, "recheck")}
          </button>
        </div>
      )}
      {phase.kind === "switchHelp" && (
        <p className="connection-fix-followup">
          {connectionFixCopy(locale, "vercelSwitchHelp")}
        </p>
      )}
      {phase.kind === "switched" && (
        <p role="status" className="connection-fix-followup">
          {connectionFixCopy(locale, "switched", { login: phase.login })}
        </p>
      )}
      {error && (
        <p role="alert" className="connection-fix-error">
          {error}
        </p>
      )}
    </li>
  );
}
