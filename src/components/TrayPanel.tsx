import { uxCopy } from "../uxCopy";
import { QueryHealthSummary } from "./QueryHealthSummary";
import { summarizeQueryHealth } from "../queryHealth";
import { monitorCopy } from "../monitorCopy";
import { isLongRunning } from "../activityView";
import { useEffect } from "react";
import {
  ArrowUpRight,
  Check,
  ChevronRight,
  Circle,
  LoaderCircle,
  Minus,
  Pause,
  RefreshCw,
  X,
} from "lucide-react";
import { bridge, errorMessage, isPreview } from "../bridge";
import {
  isActive,
  PROVIDER_NAMES,
  panelActivities,
  latestCompleted,
  sortPanelActivities,
  targetKey,
  type Activity,
} from "../domain";
import { ProviderIcon } from "./ProviderIcon";
import { useActivityDismissal } from "../useActivityDismissal";
import { ActivityUndo } from "./ActivityUndo";
import { SettingsRecoveryNotice } from "./SettingsRecoveryNotice";
import { activityCopy } from "../activityCopy";
import { activityIdentity, activityKeyString } from "../domain";
import { useMonitor } from "../useMonitor";
import { useVisibleClock } from "../useVisibleClock";
import {
  formatDuration,
  formatRelativeTime,
  I18nProvider,
  useI18n,
} from "../i18n";
import "../styles/panel.css";

const lockedSettingsChange = async () => false;

function RunRow({
  item,
  onError,
  onHide,
  settingsLocked,
}: {
  item: Activity;
  onError: (message: string) => void;
  onHide: (activity: Activity) => Promise<void>;
  settingsLocked: boolean;
}) {
  const { locale, t } = useI18n();
  const workflow = item.workflowName?.trim();
  const showWorkflow = Boolean(workflow && workflow !== item.title.trim());
  const heading = showWorkflow ? `${workflow} · ${item.title}` : item.title;
  const actor = item.actor?.trim() || t("common.unknownActor");
  const context = [item.project, item.branch].filter(Boolean).join(" · ");
  const Icon =
    item.status === "running"
      ? LoaderCircle
      : item.status === "success"
        ? Check
        : item.status === "failed"
          ? X
          : item.status === "waiting"
            ? Pause
            : item.status === "canceled"
              ? Minus
              : Circle;
  return (
    <div className="panel-run-entry">
      <button
        className="panel-run"
        onClick={() =>
          void bridge
            .open(item.url)
            .catch((cause: unknown) => onError(errorMessage(cause)))
        }
      >
        <span
          className="panel-provider"
          title={PROVIDER_NAMES[item.provider]}
          aria-label={`${PROVIDER_NAMES[item.provider]} · ${t(`status.${item.status}`)}`}
          role="img"
        >
          <ProviderIcon provider={item.provider} size={22} />
        </span>
        <span className="panel-run-status" title={t(`status.${item.status}`)}>
          <Icon
            size={16}
            className={`run-symbol ${item.status} ${item.status === "running" ? "spin" : ""}`}
            aria-hidden="true"
          />
        </span>
        <span className="panel-run-copy">
          <span className="panel-run-heading" title={heading}>
            {showWorkflow && (
              <>
                <span className="panel-run-workflow">{workflow}</span>
                <span aria-hidden="true"> · </span>
              </>
            )}
            <span className="panel-run-title">{item.title}</span>
          </span>
          <span className="panel-run-meta">
            <span className="panel-run-context" title={context}>
              {context}
            </span>
            <span className="panel-run-actor" title={actor}>
              {" "}
              · {actor}
            </span>
          </span>
          {isLongRunning(item) && (
            <span className="panel-run-old">
              {monitorCopy(locale, "longRunning")}
            </span>
          )}
        </span>
        <span className="panel-run-time" title={item.rawStatus}>
          {isActive(item) && item.provider !== "aws"
            ? formatDuration(
                item.startedAt,
                item.finishedAt ?? Date.now(),
                locale,
              )
            : item.finishedAt
              ? formatRelativeTime(item.finishedAt, locale)
              : t("activity.updated", {
                  time: formatRelativeTime(item.updatedAt, locale),
                })}
        </span>
      </button>
      {item.targetId && (
        <button
          className="activity-dismiss"
          aria-label={`${activityCopy(locale, "hide")}: ${item.title}`}
          title={activityCopy(locale, "hide")}
          disabled={settingsLocked}
          onClick={() => void onHide(item)}
        >
          <X size={14} aria-hidden="true" />
        </button>
      )}
    </div>
  );
}

function RunSection({
  title,
  items,
  limit,
  onError,
  onHide,
  empty,
  settingsLocked,
}: {
  title: string;
  empty: string;
  items: Activity[];
  limit: number;
  onError: (message: string) => void;
  onHide: (activity: Activity) => Promise<void>;
  settingsLocked: boolean;
}) {
  const { t } = useI18n();
  return (
    <section className="panel-section" aria-label={title}>
      <h2>
        {title}
        <span>{items.length}</span>
      </h2>
      {items.length ? (
        items
          .slice(0, limit)
          .map((item) => (
            <RunRow
              key={activityKeyString(activityIdentity(item))}
              item={item}
              onError={onError}
              onHide={onHide}
              settingsLocked={settingsLocked}
            />
          ))
      ) : (
        <p className="panel-empty-row">{empty}</p>
      )}
      {items.length > limit && (
        <button
          className="panel-more"
          onClick={() =>
            void bridge
              .dashboard()
              .catch((cause: unknown) => onError(errorMessage(cause)))
          }
        >
          {t("tray.moreInDashboard", { count: items.length - limit })}{" "}
          <ChevronRight size={12} />
        </button>
      )}
    </section>
  );
}

export function TrayPanel() {
  const monitor = useMonitor();
  return (
    <I18nProvider language={monitor.snapshot?.settings.preferences.language}>
      <TrayPanelContent monitor={monitor} />
    </I18nProvider>
  );
}

function TrayPanelContent({
  monitor,
}: {
  monitor: ReturnType<typeof useMonitor>;
}) {
  const { locale, t } = useI18n();
  const { snapshot, refreshing: busy, error, setError, refresh } = monitor;
  const settingsLocked = Boolean(snapshot?.settingsError);
  const dismissal = useActivityDismissal(
    settingsLocked ? lockedSettingsChange : monitor.change,
    setError,
  );
  useVisibleClock();
  useEffect(() => {
    document.documentElement.classList.add("panel-mode");
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape")
        void bridge
          .hide()
          .catch((cause: unknown) => setError(errorMessage(cause)));
    };
    window.addEventListener("keydown", onKey);
    return () => {
      document.documentElement.classList.remove("panel-mode");
      window.removeEventListener("keydown", onKey);
    };
  }, [setError]);
  const items = snapshot ? panelActivities(snapshot) : [];
  const panelTargets =
    snapshot?.settings.targets.filter(
      (target) =>
        snapshot.settings.panel.providers.includes(target.provider) &&
        !snapshot.settings.panel.hiddenTargets.includes(targetKey(target)),
    ) ?? [];
  const visibleTargets = panelTargets.length;
  const queryHealth = snapshot
    ? summarizeQueryHealth(snapshot, panelTargets)
    : null;
  const pendingCount = snapshot
    ? panelTargets.filter((target) =>
        snapshot.pendingTargets.includes(targetKey(target)),
      ).length
    : 0;
  const panelIssues =
    snapshot?.issues.filter(
      (issue) =>
        !snapshot.settings.targets.length ||
        panelTargets.some(
          (target) =>
            target.provider === issue.provider &&
            (!issue.connectionId ||
              issue.connectionId === target.connectionId) &&
            (!issue.targetId || issue.targetId === target.id),
        ),
    ) ?? [];
  const active = items.filter(isActive);
  const complete = snapshot
    ? sortPanelActivities(latestCompleted(items), snapshot.settings.panel)
    : [];
  const unresolved = items.filter((item) => item.status === "unknown");
  const openDashboard = () =>
    void bridge
      .dashboard()
      .catch((cause: unknown) => setError(errorMessage(cause)));
  return (
    <main className="tray-panel" aria-label={t("tray.summary")}>
      <header className="panel-header">
        <span className="panel-brand">
          <img src="/ratelline-mark-light.png" alt="" />
          ratelline
        </span>
        <div className="panel-actions">
          <button
            className="panel-icon"
            aria-label={t("common.refresh")}
            disabled={busy}
            onClick={() => void refresh()}
          >
            <RefreshCw size={14} className={busy ? "spin" : ""} />
          </button>
          <button
            className="panel-icon"
            aria-label={t("tray.close")}
            onClick={() =>
              void bridge
                .hide()
                .catch((cause: unknown) => setError(errorMessage(cause)))
            }
          >
            <X size={15} />
          </button>
        </div>
      </header>
      {isPreview && <div className="panel-preview">{t("tray.preview")}</div>}
      {snapshot && snapshot.settings.targets.length > 0 && (
        <div className="panel-scope">
          <span>
            {t("tray.scope", {
              scope:
                Object.keys(snapshot.settings.displayScopes.providers).length >
                  0 ||
                Object.keys(snapshot.settings.displayScopes.targets).length > 0
                  ? uxCopy(locale, "mixedScope")
                  : snapshot.settings.scope === "mine"
                    ? t("tray.mine")
                    : t("tray.all"),
              visible: visibleTargets,
              total: snapshot.settings.targets.length,
            })}
          </span>
          <button onClick={openDashboard}>
            {t("tray.checkScope")} <ChevronRight size={12} />
          </button>
        </div>
      )}
      <div className="panel-scroll">
        {snapshot && panelTargets.length > 0 && (
          <div className="panel-query-summary">
            <QueryHealthSummary
              snapshot={snapshot}
              targets={panelTargets}
              compact
            />
          </div>
        )}
        <SettingsRecoveryNotice error={snapshot?.settingsError} />
        {dismissal.lastHidden && !settingsLocked && (
          <ActivityUndo onUndo={dismissal.undo} busy={dismissal.undoing} />
        )}
        {snapshot && pendingCount > 0 && (
          <p className="panel-empty-row" role="status">
            {t("tray.pending", { count: pendingCount })}
          </p>
        )}
        {error && (
          <p className="panel-error" role="alert">
            {error}
          </p>
        )}
        {panelIssues.length > 0 && (
          <details className="panel-issues">
            <summary>{t("tray.issues", { count: panelIssues.length })}</summary>
            {panelIssues.map((issue, index) => (
              <p key={`${issue.provider}:${index}`}>
                {PROVIDER_NAMES[issue.provider]} · {issue.message}
              </p>
            ))}
          </details>
        )}
        {!snapshot ? (
          <p className="panel-empty-row">
            {error ? t("tray.checkConnection") : t("tray.connecting")}
          </p>
        ) : snapshot.settings.targets.length === 0 && !settingsLocked ? (
          <div className="panel-onboarding">
            <h1>{t("tray.onboardingTitle")}</h1>
            <p>{t("tray.onboardingBody")}</p>
            <button onClick={openDashboard}>
              {t("tray.connectDashboard")} <ChevronRight size={13} />
            </button>
          </div>
        ) : (
          <>
            {items.length === 0 && (
              <p className="panel-empty-row">
                {visibleTargets === 0
                  ? t("tray.noVisibleTargets")
                  : t("tray.noResults")}
              </p>
            )}
            {snapshot.settings.panel.showActive && (
              <RunSection
                title={t("dashboard.active")}
                empty={t("tray.activeEmpty")}
                items={active}
                limit={snapshot.settings.panel.rowLimit}
                onError={setError}
                onHide={dismissal.hide}
                settingsLocked={settingsLocked}
              />
            )}
            {snapshot.settings.panel.showCompleted && (
              <RunSection
                title={
                  snapshot.settings.historyHours === 24
                    ? t("common.period24h")
                    : t("common.period7d")
                }
                empty={t("tray.completedEmpty")}
                items={complete}
                limit={snapshot.settings.panel.rowLimit}
                onError={setError}
                onHide={dismissal.hide}
                settingsLocked={settingsLocked}
              />
            )}
            {!snapshot.settings.panel.showActive &&
              !snapshot.settings.panel.showCompleted && (
                <p className="panel-empty-row">{t("tray.sectionsHidden")}</p>
              )}
            {unresolved.length > 0 && (
              <RunSection
                title={t("dashboard.needsAttention")}
                empty={t("tray.unresolvedEmpty")}
                items={unresolved}
                limit={snapshot.settings.panel.rowLimit}
                onError={setError}
                onHide={dismissal.hide}
                settingsLocked={settingsLocked}
              />
            )}
          </>
        )}
      </div>
      <footer className="panel-footer">
        <span>
          {busy
            ? t("tray.checking")
            : queryHealth
              ? monitorCopy(locale, queryHealth.state)
              : t("tray.waiting")}
        </span>
        <button onClick={openDashboard}>
          {t("tray.openDashboard")} <ArrowUpRight size={13} />
        </button>
      </footer>
    </main>
  );
}
