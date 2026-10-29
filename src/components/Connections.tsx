import { useEffect, useMemo, useState } from "react";
import { AlertCircle, ArrowUpRight, LoaderCircle, Search } from "lucide-react";
import { bridge, errorMessage } from "../bridge";
import {
  PROVIDER_NAMES,
  targetKey,
  type Account,
  type Discovery,
  type Connection,
  type Provider,
  type Snapshot,
} from "../domain";
import { EasProjectConnect } from "./EasProjectConnect";
import { expansionCopy } from "../providerExpansionCopy";
import { ServiceSelector } from "./ServiceSelector";
import { AccountPicker } from "./AccountPicker";
import { getPlatformDetails } from "../platformDetails";
import { useI18n } from "../i18n";
import { isGithubProvider } from "../cliAccounts";
import {
  GithubAccountChangeButton,
  GithubAccountList,
} from "./GithubAccountList";
import { ConnectionAuthentication } from "./ConnectionAuthentication";
import { ConnectionStateSummary } from "./ConnectionStateSummary";
import { LoginNotice } from "./LoginNotice";
import { ConnectionFixes } from "./ConnectionFixes";
import type { ConnectionFix } from "../connectionFixes";
import { useCliLogins } from "../useCliLogins";
import { connectionFixCopy } from "../connectionFixCopy";
import { ProjectSelection } from "./ProjectSelection";
import { useSettingsChanges } from "../settingsChangesContext";
import {
  ConnectionProgress,
  type ConnectionProgressAction,
} from "./ConnectionProgress";
import { copy } from "../featureCopy";
import { uxCopy } from "../uxCopy";
import "../styles/connection-progress.css";
interface Props {
  snapshot: Snapshot;
  saving?: boolean;
  onBusyChange?: (busy: boolean) => void;
  onRefresh: () => Promise<void>;
  onReload?: () => Promise<void>;
  requestedProvider?: Provider;
  requestedConnectionId?: string;
  connectionRequest?: number;
  onNotifications?: () => void;
}
export function Connections({
  snapshot,
  saving = false,
  onBusyChange,
  onRefresh,
  onReload,
  requestedProvider,
  requestedConnectionId,
  connectionRequest,
  onNotifications,
}: Props) {
  const { locale, t } = useI18n();
  const { pendingKeys } = useSettingsChanges();
  const [provider, setProvider] = useState<Provider>("github");
  const [connectionId, setConnectionId] = useState(
    snapshot.settings.accounts.find((account) => account.provider === "github")
      ?.id ?? "",
  );
  const [awsProfiles, setAwsProfiles] = useState<string[]>([]);
  const [results, setResults] = useState<
    Partial<Record<string, Discovery & { previousConnection: string }>>
  >({});
  const [knownTargets, setKnownTargets] = useState(snapshot.settings.targets);
  useEffect(() => {
    setKnownTargets((previous) => {
      const accountIds = new Set(
        snapshot.settings.accounts.map((account) => account.id),
      );
      const targetMap = new Map(
        snapshot.settings.targets.map((target) => [targetKey(target), target]),
      );
      const retained = previous.filter(
        (target) =>
          accountIds.has(target.connectionId) &&
          !targetMap.has(targetKey(target)),
      );
      const next = [...retained, ...targetMap.values()];
      return next.length === previous.length &&
        next.every((target, index) => target === previous[index])
        ? previous
        : next;
    });
  }, [snapshot.settings.targets, snapshot.settings.accounts]);
  const [discoveryErrors, setDiscoveryErrors] = useState<
    Record<string, string>
  >({});
  const [region, setRegion] = useState("ap-northeast-2");
  const [busy, setBusy] = useState(false);
  const [cliBusy, setCliBusy] = useState(false);
  const working = busy || saving || cliBusy;
  useEffect(() => {
    onBusyChange?.(busy || cliBusy);
  }, [busy, cliBusy, onBusyChange]);
  const [error, setError] = useState<string | null>(null);
  const [pendingConnection, setPendingConnection] = useState<Account | null>(
    null,
  );
  const { login, begin, forget } = useCliLogins(connectionId, setError);
  const [fixAnnouncement, setFixAnnouncement] = useState("");
  const [githubAccountsOpen, setGithubAccountsOpen] = useState<string | false>(
    false,
  );
  const details = getPlatformDetails(provider, locale);
  const isTokenProvider = [
    "bitbucket",
    "cloudflare-pages",
    "cloudflare-workers",
  ].includes(provider);
  const reload = onReload ?? onRefresh;
  const accounts = snapshot.settings.accounts.filter(
    (account) => account.provider === provider,
  );
  const account = accounts.find((item) => item.id === connectionId);
  const selectAccount = (value: Account) => {
    setConnectionId(value.id);
    setError(null);
  };
  const discoverConnection = async (
    nextProvider: Provider,
    nextConnectionId: string,
    profile: string | null,
  ) => {
    try {
      const found = await bridge.discover(
        nextProvider,
        nextConnectionId,
        nextProvider === "aws" ? profile : null,
        nextProvider === "aws" ? region || null : null,
      );
      setResults((previous) => ({
        ...previous,
        [nextConnectionId]: {
          ...found,
          previousConnection: JSON.stringify(
            snapshot.connections.find(
              (item) => item.connectionId === nextConnectionId,
            ),
          ),
        },
      }));
      setDiscoveryErrors((previous) =>
        Object.fromEntries(
          Object.entries(previous).filter(([id]) => id !== nextConnectionId),
        ),
      );
      if (nextProvider === "aws") setAwsProfiles(found.awsProfiles);
    } catch (cause) {
      setDiscoveryErrors((previous) => ({
        ...previous,
        [nextConnectionId]: errorMessage(cause),
      }));
      throw cause;
    }
  };
  const createAccount = async (label: string, profile: string | null) => {
    setBusy(true);
    setError(null);
    try {
      const created = await bridge.createConnection(provider, label, profile);
      await reload();
      selectAccount(created);
      try {
        await discoverConnection(
          created.provider,
          created.id,
          created.profile ?? null,
        );
      } catch (cause) {
        setError(
          t("connections.addedDiscoveryFailed", { error: errorMessage(cause) }),
        );
      }
      return true;
    } catch (cause) {
      setError(errorMessage(cause));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const connectBitbucket = async (
    email: string,
    token: string,
    id: string | null,
  ) => {
    setBusy(true);
    setError(null);
    try {
      const connected = await bridge.connectBitbucket(email, token, id);
      try {
        await reload();
      } catch {
        setPendingConnection(connected);
        return true;
      }
      selectAccount(connected);
      try {
        await discoverConnection("bitbucket", connected.id, null);
      } catch (cause) {
        setError(
          t("connections.addedDiscoveryFailed", { error: errorMessage(cause) }),
        );
      }
      return true;
    } catch (cause) {
      setError(errorMessage(cause));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const connectCloudflare = async (
    accountId: string,
    token: string,
    id: string | null,
  ) => {
    setBusy(true);
    setError(null);
    try {
      const connected = await bridge.connectCloudflare(
        provider,
        accountId,
        token,
        id,
      );
      try {
        await reload();
      } catch {
        setPendingConnection(connected);
        return true;
      }
      selectAccount(connected);
      try {
        await discoverConnection(connected.provider, connected.id, null);
      } catch (cause) {
        setError(
          t("connections.addedDiscoveryFailed", { error: errorMessage(cause) }),
        );
      }
      return true;
    } catch (cause) {
      setError(errorMessage(cause));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const retryConnectionReload = async () => {
    if (!pendingConnection || busy) return;
    setBusy(true);
    setError(null);
    try {
      await reload();
      selectAccount(pendingConnection);
      setPendingConnection(null);
      try {
        await discoverConnection(
          pendingConnection.provider,
          pendingConnection.id,
          null,
        );
      } catch (cause) {
        setError(
          t("connections.addedDiscoveryFailed", { error: errorMessage(cause) }),
        );
      }
    } catch {
      setError(expansionCopy(locale, "reloadNeeded"));
    } finally {
      setBusy(false);
    }
  };
  const removeAccount = async (): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      const saved = await bridge.removeConnection(connectionId);
      forget(connectionId);
      setResults((previous) =>
        Object.fromEntries(
          Object.entries(previous).filter(([id]) => id !== connectionId),
        ),
      );
      setConnectionId(
        saved.accounts.find((item) => item.provider === provider)?.id ?? "",
      );
      await reload();
      return true;
    } catch (cause) {
      setError(errorMessage(cause));
      return false;
    } finally {
      setBusy(false);
    }
  };
  const loadProfiles = async () => {
    setBusy(true);
    setError(null);
    try {
      setAwsProfiles(
        (await bridge.discover("aws", null, null, null)).awsProfiles,
      );
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const discover = async () => {
    setBusy(true);
    setError(null);
    try {
      if (!connectionId) return;
      await discoverConnection(
        provider,
        connectionId,
        account?.profile ?? null,
      );
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const beginLogin = async (
    loginProvider: Provider = provider,
    loginConnectionId: string = connectionId,
  ) => {
    setBusy(true);
    setError(null);
    try {
      const profile =
        loginProvider === "aws"
          ? (snapshot.settings.accounts.find(
              (item) => item.id === loginConnectionId,
            )?.profile ?? null)
          : null;
      await begin(loginProvider, loginConnectionId, profile);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  const showConnection = (fix: ConnectionFix) => {
    setProvider(fix.provider);
    setConnectionId(fix.connectionId);
    setError(null);
  };
  const focusAccountControls = () =>
    window.requestAnimationFrame(() =>
      document
        .getElementById("connection-account-controls")
        ?.querySelector<HTMLElement>(
          "button:not(:disabled), select:not(:disabled), input:not(:disabled)",
        )
        ?.focus(),
    );
  const result = results[connectionId];
  const snapshotConnection = snapshot.connections.find(
    (item) => item.connectionId === connectionId,
  );
  const newerSnapshot =
    result && JSON.stringify(snapshotConnection) !== result.previousConnection;
  const connection: Connection | undefined = newerSnapshot
    ? snapshotConnection
    : (result?.connection ?? snapshotConnection);
  const discoveryError = discoveryErrors[connectionId];
  const connected = connection?.status === "connected";
  // Bound, working GitHub connections keep the account list one click away.
  const showGithubAccounts =
    !account?.cliUsername ||
    githubAccountsOpen === connectionId ||
    (!connected &&
      ["needsAccount", "noAccess", "accountMismatch"].includes(
        connection?.kind ?? "",
      ));
  const statusLabel = discoveryError
    ? uxCopy(locale, "verificationFailed")
    : copy(
        locale,
        connected
          ? "connected"
          : connection?.status === "signed-out"
            ? "signedOut"
            : connection?.status === "missing-cli"
              ? "missingCli"
              : connection?.status === "error"
                ? "connectionError"
                : "notChecked",
      );
  const uniqueTargets = useMemo(
    () => [
      ...new Map(
        [
          ...knownTargets.filter(
            (target) => target.connectionId === connectionId,
          ),
          ...(result?.targets ?? []),
        ].map((target) => [targetKey(target), target]),
      ).values(),
    ],
    [knownTargets, connectionId, result],
  );
  const selectProvider = (nextProvider: Provider) => {
    setProvider(nextProvider);
    setConnectionId(
      snapshot.settings.accounts.find(
        (account) => account.provider === nextProvider,
      )?.id ?? "",
    );
    setError(null);
  };
  useEffect(() => {
    if (requestedProvider) {
      setProvider(requestedProvider);
      setConnectionId(
        snapshot.settings.accounts.find(
          (item) =>
            item.provider === requestedProvider &&
            (!requestedConnectionId || item.id === requestedConnectionId),
        )?.id ?? "",
      );
      setError(null);
    }
  }, [requestedProvider, requestedConnectionId, connectionRequest]);
  const canConnect = Boolean(account) && !details.unsupportedMessage;
  const selectedTargets = snapshot.settings.targets.filter(
    (target) =>
      target.provider === provider && target.connectionId === connectionId,
  );
  const selectionPrefix = `${provider}:${connectionId}:`;
  const selectionPending = pendingKeys.some(
    (key) =>
      key.startsWith(`target:${selectionPrefix}`) ||
      key.startsWith(`targets:${selectionPrefix}`) ||
      key.includes(`|${selectionPrefix}`),
  );
  const handleProgressAction = (action: ConnectionProgressAction) => {
    if (working) return;
    if (action === "verify") {
      void discover();
      return;
    }
    if (action === "query") {
      void onRefresh().catch((cause: unknown) => setError(errorMessage(cause)));
      return;
    }
    if (action === "githubAccount") {
      const list = document.getElementById("github-account-list");
      (
        list?.querySelector<HTMLElement>(
          ".github-account-row button:not(:disabled)",
        ) ?? list
      )?.focus();
      return;
    }
    const sectionId =
      action === "projects"
        ? "connection-project-controls"
        : action === "authentication"
          ? "connection-authentication"
          : "connection-account-controls";
    const section = document.getElementById(sectionId);
    if (action === "authentication" && section instanceof HTMLDetailsElement)
      section.open = true;
    const control =
      action === "authentication"
        ? (section?.querySelector<HTMLElement>(
            "select:not(:disabled), input:not(:disabled), button:not(:disabled)",
          ) ?? section?.querySelector<HTMLElement>(":scope > summary"))
        : action === "projects"
          ? (section?.querySelector<HTMLElement>(
              ".target-option input:not(:disabled)",
            ) ?? document.getElementById("connection-verify-control"))
          : section?.querySelector<HTMLElement>(
              "button:not(:disabled), select:not(:disabled), input:not(:disabled), summary",
            );
    control?.focus();
  };
  const progress = (
    <ConnectionProgress
      snapshot={snapshot}
      provider={provider}
      accountPresent={Boolean(account)}
      connection={connection}
      selectedTargets={selectedTargets}
      selectionPending={selectionPending}
      requiresCli={provider !== "aws" && Boolean(details.cliCommand)}
      verificationError={discoveryError}
      onAction={handleProgressAction}
    />
  );
  return (
    <section
      className="connections-settings"
      aria-label={t("connections.title")}
    >
      <div
        className="dialog-body"
        tabIndex={0}
        aria-label={t("connections.content")}
      >
        <ConnectionFixes
          snapshot={snapshot}
          busy={working}
          focusConnectionId={requestedConnectionId}
          focusRequest={connectionRequest}
          onReload={reload}
          onRefresh={onRefresh}
          onSignIn={(fix) => {
            showConnection(fix);
            void beginLogin(fix.provider, fix.connectionId);
          }}
          onOpenConnection={(fix) => {
            showConnection(fix);
            focusAccountControls();
          }}
          onFixed={(fix, login) => {
            showConnection(fix);
            setGithubAccountsOpen(false);
            setFixAnnouncement(
              connectionFixCopy(locale, "switched", { login }),
            );
            // The fixed card disappears; continue at the connection's account.
            window.requestAnimationFrame(() =>
              window.requestAnimationFrame(() =>
                (
                  document.getElementById("github-account-change") ??
                  document.getElementById("connection-provider-panel")
                )?.focus(),
              ),
            );
          }}
        />
        <p className="sr-only" role="status" aria-live="polite">
          {fixAnnouncement}
        </p>
        {!connected && (
          <p className="section-intro">{uxCopy(locale, "connectionIntro")}</p>
        )}
        <ServiceSelector
          provider={provider}
          disabled={working}
          onSelect={selectProvider}
        />
        <section
          id="connection-provider-panel"
          className="connection-provider-panel"
          tabIndex={-1}
          role="tabpanel"
          aria-label={t("connections.providerSettings", {
            provider: PROVIDER_NAMES[provider],
          })}
        >
          {!connected && (
            <header className="setup-step-heading">
              <span aria-hidden="true">1</span>
              <div>
                <h2>{uxCopy(locale, "accountStep")}</h2>
                <p>{uxCopy(locale, "accountHelp")}</p>
              </div>
            </header>
          )}
          <div id="connection-account-controls">
            {isTokenProvider && pendingConnection?.provider === provider ? (
              <div className="notice warning" role="status">
                <span>{expansionCopy(locale, "reloadNeeded")}</span>
                <button
                  className="button"
                  disabled={working}
                  onClick={() => void retryConnectionReload()}
                >
                  {t("common.retry")}
                </button>
              </div>
            ) : (
              <AccountPicker
                key={provider}
                provider={provider}
                accounts={accounts}
                settings={snapshot.settings}
                connections={snapshot.connections}
                discoveredConnection={connection}
                selectedId={connectionId}
                busy={working}
                profiles={awsProfiles}
                onSelect={selectAccount}
                onCreate={createAccount}
                onRemove={removeAccount}
                onConnectBitbucket={connectBitbucket}
                onConnectCloudflare={connectCloudflare}
                onLoadProfiles={loadProfiles}
                savedTargetCount={
                  snapshot.settings.targets.filter(
                    (target) => target.connectionId === connectionId,
                  ).length
                }
              />
            )}
            {isGithubProvider(provider) && account && showGithubAccounts && (
              <GithubAccountList
                key={connectionId}
                provider={provider}
                connectionId={connectionId}
                username={account.cliUsername}
                targetCount={selectedTargets.length}
                busy={busy || saving}
                onBusyChange={setCliBusy}
                onConnected={async () => {
                  setGithubAccountsOpen(false);
                  await reload();
                  await discoverConnection(provider, connectionId, null);
                }}
              />
            )}
          </div>
          {details.unsupportedMessage && (
            <div className="connection-actions">
              <button
                className="button"
                onClick={() => selectProvider("github")}
              >
                {expansionCopy(locale, "ciGithub")}
              </button>
              <button
                className="button"
                onClick={() => selectProvider("gitlab")}
              >
                {expansionCopy(locale, "ciGitlab")}
              </button>
            </div>
          )}
          {(error || discoveryError) && (
            <div className="notice warning connection-error" role="alert">
              <AlertCircle size={16} />
              <span>{error || discoveryError}</span>
            </div>
          )}
          {!details.unsupportedMessage &&
            isTokenProvider &&
            !account &&
            progress}
          {!details.unsupportedMessage && (!isTokenProvider || account) && (
            <>
              <div className="connection-overview">
                <ConnectionStateSummary
                  account={account}
                  connection={connection}
                  settings={snapshot.settings}
                  statusLabel={statusLabel}
                  healthy={connected && !discoveryError}
                />
                <div className="connection-actions">
                  <button
                    id="connection-verify-control"
                    className={`button ${connected ? "" : "primary"}`}
                    onClick={() => void discover()}
                    disabled={working || !canConnect}
                  >
                    {busy ? (
                      <LoaderCircle size={15} className="spin" />
                    ) : (
                      <Search size={15} />
                    )}
                    {t("connections.verifyTargets")}
                  </button>
                  {isGithubProvider(provider) &&
                    account?.cliUsername &&
                    !showGithubAccounts && (
                      <GithubAccountChangeButton
                        login={account.cliUsername}
                        disabled={working}
                        onOpen={() => setGithubAccountsOpen(connectionId)}
                      />
                    )}
                  {details.canStartAppLogin && (
                    <button
                      className="button"
                      onClick={() => void beginLogin()}
                      disabled={
                        busy ||
                        !canConnect ||
                        (login?.connectionId === connectionId &&
                          login.state === "waiting")
                      }
                    >
                      {provider === "aws"
                        ? t("connections.ssoLogin")
                        : t("connections.browserLogin")}
                      <ArrowUpRight size={14} />
                    </button>
                  )}
                </div>
              </div>
              {progress}
              <div id="connection-project-controls">
                {provider === "eas" && account && (
                  <EasProjectConnect
                    key={`eas-project-${connectionId}`}
                    connectionId={connectionId}
                    disabled={working}
                    onBusyChange={setCliBusy}
                    onRegistered={async (targets) => {
                      setKnownTargets((previous) => [
                        ...previous.filter(
                          (old) =>
                            !targets.some(
                              (target) => targetKey(target) === targetKey(old),
                            ),
                        ),
                        ...targets,
                      ]);
                      await reload();
                    }}
                  />
                )}
                <ProjectSelection
                  key={connectionId}
                  snapshot={snapshot}
                  provider={provider}
                  connectionId={connectionId}
                  uniqueTargets={uniqueTargets}
                  discovered={Boolean(result)}
                  disabled={working}
                  onRefresh={onRefresh}
                  onError={(cause) => setError(errorMessage(cause))}
                  onNotifications={onNotifications}
                />
              </div>
              {!isTokenProvider && (
                <ConnectionAuthentication
                  key={`auth-${connectionId}`}
                  provider={provider}
                  account={account}
                  connection={connection}
                />
              )}
              {provider === "aws" && account && (
                <div className="aws-fields">
                  <label>
                    {t("connections.profile")}
                    <input value={account.profile ?? ""} readOnly />
                  </label>
                  <label>
                    {t("connections.region")}
                    <input
                      value={region}
                      onChange={(event) => setRegion(event.target.value)}
                      placeholder="ap-northeast-2"
                    />
                  </label>
                </div>
              )}
              {login?.connectionId === connectionId && (
                <LoginNotice
                  login={login}
                  onError={setError}
                  onLoadTargets={() => void discover()}
                />
              )}
              {result?.issues.map((issue, index) => (
                <div
                  className="notice warning"
                  key={`${issue.provider}-${index}`}
                  role="status"
                >
                  {issue.message}
                </div>
              ))}
              {!details.supportsActorAttribution && (
                <p className="field-help">{t("connections.actorHelp")}</p>
              )}
            </>
          )}
        </section>
      </div>
    </section>
  );
}
