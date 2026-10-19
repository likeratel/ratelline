import { useRef, useState } from "react";
import {
  Activity as ActivityIcon,
  AlertCircle,
  ChevronRight,
  LoaderCircle,
  Plus,
  Radio,
  RefreshCw,
  Search,
  Settings2,
  X,
} from "lucide-react";
import { SettingsChangesProvider } from "./settingsChangesContext";
import { SettingFeedback } from "./components/SettingFeedback";
import { isPreview } from "./bridge";
import {
  effectiveDisplayScope,
  PROVIDERS,
  PROVIDER_NAMES,
  visibleActivities,
  targetKey,
  type Provider,
} from "./domain";
import { ActivityStatusFilters } from "./components/ActivityStatusFilters";
import { DashboardRuns } from "./components/DashboardRuns";
import { QueryHealthSummary } from "./components/QueryHealthSummary";
import type { ActivityFilter } from "./activityView";
import { monitorCopy } from "./monitorCopy";
import { summarizeQueryHealth } from "./queryHealth";
import { FilteredRunsEmpty } from "./components/FilteredRunsEmpty";
import { accountDisplayName } from "./accountNames";
import { HistoryBrowser } from "./components/HistoryBrowser";
import { ProviderIcon } from "./components/ProviderIcon";
import { useActivityDismissal } from "./useActivityDismissal";
import { ActivityUndo } from "./components/ActivityUndo";
import { isHiddenActivity } from "./domain";
import { useMonitor } from "./useMonitor";
import { useVisibleClock } from "./useVisibleClock";
import { AccountFilter } from "./components/AccountFilter";
import { SettingsPage } from "./components/SettingsPage";
import { settingsCopy, type SettingsTab } from "./settingsCopy";
import { ServiceHealth } from "./components/ServiceHealth";
import { UpdateNotice } from "./components/UpdateNotice";
import { SettingsRecoveryNotice } from "./components/SettingsRecoveryNotice";
import { I18nProvider, useI18n } from "./i18n";
import { uxCopy } from "./uxCopy";
import { DisplayScopeControl } from "./components/DisplayScopeControl";

const lockedSettingsChange = async () => false;

export default function App() {
  const monitor = useMonitor();
  const settingsChanges = monitor.snapshot?.settingsError
    ? {
        ...monitor.settingsChanges,
        change: lockedSettingsChange,
        retry: async () => false,
      }
    : monitor.settingsChanges;
  return (
    <I18nProvider language={monitor.snapshot?.settings.preferences.language}>
      <SettingsChangesProvider value={settingsChanges}>
        <Dashboard monitor={monitor} />
      </SettingsChangesProvider>
    </I18nProvider>
  );
}

function Dashboard({ monitor }: { monitor: ReturnType<typeof useMonitor> }) {
  const { locale, t } = useI18n();
  const settingsLocked = Boolean(monitor.snapshot?.settingsError);
  const settingsChange = settingsLocked ? lockedSettingsChange : monitor.change;
  const dismissal = useActivityDismissal(settingsChange, monitor.setError);
  const [provider, setProvider] = useState<Provider | "all">("all");
  const [accountId, setAccountId] = useState("all");
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<ActivityFilter>("all");
  const searchInput = useRef<HTMLInputElement>(null);
  const [settingsConnectionId, setSettingsConnectionId] = useState<string>();
  const [connectionRequest, setConnectionRequest] = useState(0);
  const [settingsProvider, setSettingsProvider] = useState<
    Provider | undefined
  >();
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("connections");
  const [settingsVisited, setSettingsVisited] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const openSettings = (
    tab?: SettingsTab,
    target?: { provider: Provider; connectionId: string },
  ) => {
    if (tab) setSettingsTab(tab);
    if (tab === "connections") {
      const account = monitor.snapshot?.settings.accounts.find(
        (item) =>
          item.id === accountId &&
          (provider === "all" || item.provider === provider),
      );
      setSettingsProvider(
        target?.provider ??
          account?.provider ??
          (provider === "all" ? undefined : provider),
      );
      setSettingsConnectionId(target?.connectionId ?? account?.id);
      setConnectionRequest((value) => value + 1);
    }
    setSettingsVisited(true);
    setSettingsOpen(true);
  };
  useVisibleClock();
  const {
    snapshot,
    busy,
    refreshing,
    error,
    setError,
    refresh,
    reload,
    loadMore,
    loadingMore,
  } = monitor;
  if (!snapshot)
    return (
      <main className="boot">
        <img className="boot-logo" src="/ratelline-mark.png" alt="" />
        <h1>ratelline</h1>
        <p>{error ?? t("boot.checking")}</p>
        {error ? (
          <button className="button" onClick={() => void refresh()}>
            {t("common.retry")}
          </button>
        ) : (
          <LoaderCircle className="spin" size={22} />
        )}
      </main>
    );
  const selectedAccount = snapshot.settings.accounts.some(
    (account) =>
      account.id === accountId &&
      (provider === "all" || provider === account.provider),
  )
    ? accountId
    : "all";
  const activities = visibleActivities(
    snapshot,
    provider,
    search,
    undefined,
    selectedAccount,
  );
  const activityListKey = JSON.stringify([
    provider,
    selectedAccount,
    search,
    snapshot.settings.historyHours,
  ]);
  const unknown = snapshot.activities.filter(
    (activity) =>
      !isHiddenActivity(activity, snapshot.settings) &&
      activity.ownership === "unknown" &&
      effectiveDisplayScope(snapshot.settings, activity) === "mine" &&
      (selectedAccount === "all" ||
        activity.connectionId === selectedAccount) &&
      (provider === "all" || activity.provider === provider),
  ).length;
  const hasScopeOverrides =
    Object.keys(snapshot.settings.displayScopes.providers).length > 0 ||
    Object.keys(snapshot.settings.displayScopes.targets).length > 0;
  const providerTargets = snapshot.settings.targets.filter(
    (target) => provider === "all" || target.provider === provider,
  );
  const scopedTargets = providerTargets.filter(
    (target) =>
      selectedAccount === "all" || target.connectionId === selectedAccount,
  );
  const monitoredAccountIds = new Set(
    scopedTargets.map((target) => target.connectionId),
  );
  const hasConnectionIssues =
    ["error", "partial"].includes(
      summarizeQueryHealth(snapshot, scopedTargets).state,
    ) ||
    snapshot.issues.some((issue) =>
      issue.connectionId
        ? monitoredAccountIds.has(issue.connectionId)
        : scopedTargets.some((target) => target.provider === issue.provider),
    ) ||
    snapshot.connections.some(
      (connection) =>
        connection.status !== "connected" &&
        monitoredAccountIds.has(connection.connectionId),
    );
  const hasTargets = providerTargets.length > 0;
  const pendingCount = scopedTargets.filter((target) =>
    snapshot.pendingTargets.includes(targetKey(target)),
  ).length;
  const noFilteredRuns =
    activities.length === 0 &&
    pendingCount === 0 &&
    (Boolean(search.trim()) || selectedAccount !== "all");
  const filteredAccount = snapshot.settings.accounts.find(
    (account) => account.id === selectedAccount,
  );
  const connected = snapshot.connections.filter(
    (connection) =>
      connection.status === "connected" &&
      monitoredAccountIds.has(connection.connectionId),
  ).length;

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          onClick={(event) => {
            event.preventDefault();
            setProvider("all");
            setSettingsOpen(false);
          }}
        >
          <span className="brand-mark">
            <img src="/ratelline-mark.png" alt="" />
          </span>
          <span>ratelline</span>
        </a>
        <nav aria-label={t("nav.serviceFilter")}>
          <button
            className={`nav-item ${!settingsOpen && provider === "all" ? "selected" : ""}`}
            title={t("nav.allServices")}
            onClick={() => {
              setProvider("all");
              setSettingsOpen(false);
            }}
          >
            <ActivityIcon size={17} />
            {t("nav.allServices")}
            <span className="nav-count">
              {snapshot.settings.targets.length}
            </span>
          </button>
          {PROVIDERS.filter((item) =>
            snapshot.settings.accounts.some(
              (account) => account.provider === item,
            ),
          ).map((item) => (
            <button
              key={item}
              title={PROVIDER_NAMES[item]}
              className={`nav-item ${!settingsOpen && provider === item ? "selected" : ""}`}
              onClick={() => {
                setProvider(item);
                setSettingsOpen(false);
              }}
            >
              <ProviderIcon provider={item} />
              {PROVIDER_NAMES[item]}
              <span className="nav-count">
                {
                  snapshot.settings.targets.filter(
                    (target) => target.provider === item,
                  ).length
                }
              </span>
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <button
            className={`nav-item ${settingsOpen ? "selected" : ""}`}
            title={settingsCopy(locale, "title")}
            onClick={() => openSettings()}
          >
            <Settings2 size={17} />
            {settingsCopy(locale, "title")}
          </button>
        </div>
      </aside>
      <main
        className="main-content"
        data-settings-locked={settingsLocked || undefined}
      >
        {settingsVisited && (
          <div hidden={!settingsOpen}>
            <SettingsPage
              tab={settingsTab}
              requestedProvider={settingsProvider}
              requestedConnectionId={settingsConnectionId}
              connectionRequest={connectionRequest}
              onTabChange={setSettingsTab}
              snapshot={snapshot}
              busy={busy}
              error={error}
              onRefresh={refresh}
              onReload={reload}
            />
          </div>
        )}
        {!settingsOpen && (
          <>
            {isPreview && (
              <div className="preview-banner">
                <Radio size={14} />
                <span>{t("dashboard.preview")}</span>
              </div>
            )}
            <header className="main-header">
              <div>
                <h1>
                  {provider === "all"
                    ? t("dashboard.title")
                    : PROVIDER_NAMES[provider]}
                </h1>
                {hasTargets ? (
                  <QueryHealthSummary
                    snapshot={snapshot}
                    targets={scopedTargets}
                  />
                ) : (
                  <p className="page-description">
                    {uxCopy(locale, "dashboardHelp")}
                  </p>
                )}
              </div>
              <div className="dashboard-header-actions">
                <button
                  className="button refresh-button"
                  disabled={refreshing}
                  onClick={() => void refresh()}
                >
                  <RefreshCw size={15} className={refreshing ? "spin" : ""} />
                  {t("common.refresh")}
                </button>
                {hasTargets && (
                  <div
                    className="monitor-summary"
                    aria-label={t("dashboard.title")}
                  >
                    <button onClick={() => openSettings("connections")}>
                      <Radio size={14} />
                      {t("dashboard.monitoringTargets", {
                        count: scopedTargets.length,
                      })}
                      <ChevronRight size={12} />
                    </button>
                    <span className="connection-health-summary">
                      {uxCopy(locale, "connectedAccounts")} {connected}/
                      {monitoredAccountIds.size}
                    </span>
                  </div>
                )}
              </div>
            </header>
            <UpdateNotice settings={snapshot.settings} />
            <SettingsRecoveryNotice error={snapshot.settingsError} />
            {error && (
              <div className="notice warning" role="alert">
                <AlertCircle size={16} />
                <span>{error}</span>
                <button
                  className="icon-button"
                  aria-label={t("dashboard.dismissError")}
                  onClick={() => setError(null)}
                >
                  <X size={14} />
                </button>
              </div>
            )}
            {snapshot.issues
              .filter(
                (issue) =>
                  !snapshot.settings.accounts.some(
                    (account) =>
                      (issue.connectionId
                        ? issue.connectionId === account.id
                        : issue.provider === account.provider) &&
                      snapshot.settings.targets.some(
                        (target) => target.connectionId === account.id,
                      ),
                  ),
              )
              .map((issue, index) => (
                <div
                  className="notice warning"
                  role="status"
                  key={`${issue.provider}-${index}`}
                >
                  <AlertCircle size={16} />
                  <span>
                    <strong>{PROVIDER_NAMES[issue.provider]}</strong> ·{" "}
                    {issue.message}
                  </span>
                </div>
              ))}
            {!hasTargets ? (
              settingsLocked ? null : (
                <div className="onboarding-empty">
                  <span className="empty-orbit">
                    <Radio size={37} />
                  </span>
                  <h2>
                    {provider === "all"
                      ? t("dashboard.onboardingTitle")
                      : uxCopy(locale, "noServiceProjects")}
                  </h2>
                  <p>
                    {provider === "all"
                      ? t("dashboard.onboardingBody")
                      : uxCopy(locale, "noServiceProjectsHelp")}
                  </p>
                  <button
                    className="button primary"
                    onClick={() => openSettings("connections")}
                  >
                    <Plus size={16} />
                    {provider === "all"
                      ? t("dashboard.connectService")
                      : uxCopy(locale, "chooseProjects")}
                  </button>
                  {provider === "all" && (
                    <small>{t("dashboard.onboardingSteps")}</small>
                  )}
                </div>
              )
            ) : (
              <>
                <ActivityStatusFilters
                  activities={activities}
                  value={statusFilter}
                  onChange={setStatusFilter}
                />
                <div className="display-scope-bar">
                  {provider !== "all" ? (
                    <DisplayScopeControl
                      settings={snapshot.settings}
                      provider={provider}
                      disabled={busy || settingsLocked}
                    />
                  ) : (
                    <div className="scope-default-choice">
                      <span>{uxCopy(locale, "globalDefault")}</span>
                      <div
                        className="segmented"
                        role="group"
                        aria-label={t("dashboard.actorScope")}
                      >
                        <button
                          aria-pressed={snapshot.settings.scope === "mine"}
                          disabled={busy || settingsLocked}
                          onClick={() =>
                            void settingsChange({
                              kind: "scope",
                              value: "mine",
                            })
                          }
                        >
                          {t("dashboard.mineOnly")}
                        </button>
                        <button
                          aria-pressed={snapshot.settings.scope === "all"}
                          disabled={busy || settingsLocked}
                          onClick={() =>
                            void settingsChange({
                              kind: "scope",
                              value: "all",
                            })
                          }
                        >
                          {t("dashboard.allRuns")}
                        </button>
                      </div>
                    </div>
                  )}
                  <div className="scope-management">
                    {hasScopeOverrides && (
                      <span>{uxCopy(locale, "customScopes")}</span>
                    )}
                    <button
                      className="text-button"
                      onClick={() => openSettings("connections")}
                    >
                      {uxCopy(locale, "manageScopes")}
                      <ChevronRight size={14} />
                    </button>
                  </div>
                </div>
                <div className="toolbar">
                  <label className="search-box">
                    <Search size={15} />
                    <input
                      ref={searchInput}
                      value={search}
                      onChange={(event) => setSearch(event.target.value)}
                      placeholder={t("dashboard.searchPlaceholder")}
                      aria-label={t("dashboard.search")}
                    />
                    {search && (
                      <button
                        className="clear-search"
                        aria-label={t("dashboard.clearSearch")}
                        onClick={() => setSearch("")}
                      >
                        <X size={13} />
                      </button>
                    )}
                  </label>
                  <AccountFilter
                    snapshot={snapshot}
                    provider={provider}
                    value={selectedAccount}
                    onChange={setAccountId}
                    className="period-select"
                  />
                  <select
                    className="period-select"
                    aria-label={t("dashboard.period")}
                    value={snapshot.settings.historyHours}
                    disabled={busy || settingsLocked}
                    onChange={(event) =>
                      void settingsChange({
                        kind: "historyHours",
                        value: event.target.value === "168" ? 168 : 24,
                      }).then((saved) => {
                        if (saved) void refresh();
                      })
                    }
                  >
                    <option value={24}>{t("common.period24h")}</option>
                    <option value={168}>{t("common.period7d")}</option>
                  </select>
                </div>
                <SettingFeedback changeKey="scope" />
                <SettingFeedback changeKey="historyHours" />
                {unknown > 0 && (
                  <div className="scope-description">
                    <button
                      disabled={busy}
                      onClick={() => openSettings("connections")}
                    >
                      {uxCopy(locale, "unknownScope")}{" "}
                      <ChevronRight size={12} />
                    </button>
                  </div>
                )}
                {hasConnectionIssues && (
                  <ServiceHealth
                    snapshot={snapshot}
                    accountIds={monitoredAccountIds}
                    expanded
                    onConnect={(provider, connectionId) =>
                      openSettings("connections", { provider, connectionId })
                    }
                  />
                )}
                {pendingCount > 0 && (
                  <div className="notice" role="status">
                    {t("dashboard.pending", {
                      count: pendingCount,
                    })}
                  </div>
                )}
                {dismissal.lastHidden && !settingsLocked && (
                  <ActivityUndo
                    onUndo={dismissal.undo}
                    busy={dismissal.undoing}
                  />
                )}

                {noFilteredRuns ? (
                  <FilteredRunsEmpty
                    search={search}
                    accountName={
                      filteredAccount
                        ? accountDisplayName(
                            filteredAccount,
                            snapshot.settings,
                            snapshot.connections,
                          )
                        : undefined
                    }
                    onReset={() => {
                      setSearch("");
                      setAccountId("all");
                      searchInput.current?.focus();
                    }}
                  />
                ) : (
                  <DashboardRuns
                    snapshot={snapshot}
                    activities={activities}
                    filter={statusFilter}
                    listKey={activityListKey}
                    onResetFilter={() => setStatusFilter("all")}
                    onError={setError}
                    onHide={dismissal.hide}
                  />
                )}
                <HistoryBrowser
                  snapshot={snapshot}
                  provider={provider}
                  accountId={selectedAccount}
                  loading={loadingMore}
                  onLoad={loadMore}
                />
                {!hasConnectionIssues && (
                  <ServiceHealth
                    snapshot={snapshot}
                    accountIds={monitoredAccountIds}
                    onConnect={(provider, connectionId) =>
                      openSettings("connections", { provider, connectionId })
                    }
                  />
                )}
                <details className="dashboard-guide">
                  <summary>{t("dashboard.guideTitle")}</summary>
                  <div>
                    <p>{uxCopy(locale, "displayHelp")}</p>
                    <p>{t("dashboard.guideHistory")}</p>
                    <p>{t("dashboard.guideSettings")}</p>
                  </div>
                </details>
              </>
            )}
            <footer className="main-footer">
              <span>{monitorCopy(locale, "healthHelp")}</span>
              <span>
                {monitorCopy(locale, "cadence", {
                  seconds: snapshot.pollIntervalSeconds,
                })}
              </span>
            </footer>
          </>
        )}
      </main>
    </div>
  );
}
