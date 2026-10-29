use super::{history::HistoryCache, *};
use crate::{
    cli::CliInvocation,
    models::{ActivityPage, IssueKind, QueryError, QueryHealthStatus},
    providers::catalog,
};
use futures::{stream, StreamExt};

const ACCOUNT_CONCURRENCY: usize = 3;
const TARGET_CONCURRENCY: usize = 2;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_HISTORY_ACTIVITIES: usize = 10_000;

impl AppState {
    pub async fn refresh<P: SnapshotPublisher + Sync>(&self, publisher: &P) -> Snapshot {
        let Ok(refresh_guard) = self.refresh_lock.try_lock() else {
            return self.snapshot().await;
        };
        let refresh_cycle = self.refresh_wake.begin(refresh_guard);
        let mut github_pins_checked = false;
        loop {
            let (settings, revision, epoch) = {
                let _publish = self.publish_lock.lock().await;
                let current = self.snapshot().await;
                let revision = self.settings_revision.load(Ordering::SeqCst);
                let epoch = self.refresh_epoch.fetch_add(1, Ordering::SeqCst) + 1;
                let attempted_at = Utc::now().to_rfc3339();
                let next = Snapshot {
                    refreshing: true,
                    pending_targets: current.settings.targets.iter().map(Target::key).collect(),
                    query_health: checking_query_health(
                        &current.settings.targets,
                        &current.query_health,
                        &attempted_at,
                    ),
                    ..current
                };
                let settings = next.settings.clone();
                self.emit(next, publisher).await;
                (settings, revision, epoch)
            };
            // Bind unbound GitHub connections once per call, after the snapshot
            // shows the cycle as checking. A new binding changes the settings, so
            // the cycle restarts with them.
            if !github_pins_checked {
                github_pins_checked = true;
                self.auto_pin_github_connections(publisher).await;
                if self.settings_revision.load(Ordering::SeqCst) != revision {
                    continue;
                }
            }
            stream::iter(settings.accounts.clone())
                .map(|account| {
                    let settings = &settings;
                    async move {
                        self.refresh_account(&account, settings, revision, epoch, publisher)
                            .await
                    }
                })
                .buffer_unordered(ACCOUNT_CONCURRENCY)
                .collect::<Vec<_>>()
                .await;
            let _publish = self.publish_lock.lock().await;
            if !self.current_generation(revision, epoch) {
                continue;
            }
            let current = self.snapshot().await;
            let done = Snapshot {
                refreshing: false,
                pending_targets: Vec::new(),
                ..current
            };
            self.emit(done.clone(), publisher).await;
            // Keep the cache locked through `finish`: a load failure must either be
            // included here or observe that the refresh cycle has already ended.
            let history_cache = self.history_cache.lock().await;
            let history_needs_followup = history_cache.needs_followup();
            refresh_cycle
                .finish(snapshot_needs_followup(&done, Utc::now()) || history_needs_followup);
            drop(history_cache);
            return done;
        }
    }

    async fn emit<P: SnapshotPublisher>(&self, snapshot: Snapshot, publisher: &P) {
        let snapshot = with_poll_interval(snapshot);
        *self.snapshot.write().await = snapshot.clone();
        publisher.publish_snapshot(&snapshot);
    }

    fn current_generation(&self, revision: u64, epoch: u64) -> bool {
        self.settings_revision.load(Ordering::SeqCst) == revision
            && self.refresh_epoch.load(Ordering::SeqCst) == epoch
    }

    async fn refresh_account<P: SnapshotPublisher + Sync>(
        &self,
        account: &Account,
        settings: &Settings,
        revision: u64,
        epoch: u64,
        publisher: &P,
    ) {
        let targets: Vec<_> = settings
            .targets
            .iter()
            .filter(|t| t.connection_id == account.id)
            .cloned()
            .collect();
        // Unselected providers are checked explicitly during discovery, not on every poll.
        if targets.is_empty() {
            return;
        }
        let cli = self.tools.invocation_for(account);
        let connection = match &cli {
            Ok(cli) => tokio::time::timeout(
                REQUEST_TIMEOUT,
                catalog::connection(cli.as_ref(), account, targets.first()),
            )
            .await
            .unwrap_or_else(|_| {
                Connection::disconnected(
                    &account.id,
                    account.provider,
                    ConnectionStatus::Error,
                    "Connection check timed out. Last known results may be stale.",
                )
                .with_kind(IssueKind::TimedOut)
            }),
            // Invalid saved account data or a local preparation failure; neither is
            // fixed by signing in or retrying, so it stays unclassified.
            Err(message) => Connection::disconnected(
                &account.id,
                account.provider,
                ConnectionStatus::Error,
                message,
            )
            .with_kind(IssueKind::Other),
        };
        if !self
            .publish_connection(connection.clone(), &targets, revision, epoch, publisher)
            .await
        {
            return;
        }
        if connection.status != ConnectionStatus::Connected {
            return;
        }
        let invocation = cli.ok().flatten();
        let identity = connection.account.as_deref();
        // Queue every recent page ahead of active scans. Once recent requests
        // have started, spare slots can scan active runs without a global barrier.
        let mut pending: std::collections::VecDeque<_> = targets
            .into_iter()
            .map(|target| (target, PageKind::Recent))
            .collect();
        let mut inflight = stream::FuturesUnordered::new();
        loop {
            while inflight.len() < TARGET_CONCURRENCY && self.current_generation(revision, epoch) {
                let Some((target, kind)) = pending.pop_front() else {
                    break;
                };
                let invocation = &invocation;
                inflight.push(async move {
                    let result = if kind == PageKind::Recent {
                        bounded_page(
                            invocation.as_ref(),
                            &target,
                            None,
                            identity,
                            settings.history_hours,
                        )
                        .await
                    } else {
                        bounded_active(invocation.as_ref(), &target, identity).await
                    };
                    self.publish_page(&target, result, kind, revision, epoch, publisher)
                        .await;
                    (target, kind)
                });
            }
            let Some((target, kind)) = inflight.next().await else {
                break;
            };
            if kind == PageKind::Recent && catalog::has_active_scan(account.provider) {
                pending.push_back((target, PageKind::Active));
            }
        }
    }

    async fn publish_connection<P: SnapshotPublisher>(
        &self,
        connection: Connection,
        targets: &[Target],
        revision: u64,
        epoch: u64,
        publisher: &P,
    ) -> bool {
        let _publish = self.publish_lock.lock().await;
        if !self.current_generation(revision, epoch) {
            return false;
        }
        let current = self.snapshot().await;
        let previous_connection = current
            .connections
            .iter()
            .find(|item| item.connection_id == connection.connection_id);
        let connection =
            retain_last_confirmed_accounts(&current.connections, vec![connection]).remove(0);
        let identity_changed =
            previous_connection.is_some_and(|previous| previous.account != connection.account);
        let connections: Vec<_> = current
            .connections
            .iter()
            .filter(|c| c.connection_id != connection.connection_id)
            .cloned()
            .chain([connection.clone()])
            .collect();
        let activities =
            partition_cache_by_account(&current.activities, &current.connections, &connections);
        let history = if identity_changed {
            self.history_cache
                .lock()
                .await
                .remove_connection(&connection.connection_id);
            current
                .history
                .iter()
                .filter(|entry| entry.connection_id != connection.connection_id)
                .cloned()
                .collect()
        } else {
            current.history.clone()
        };
        let mut issues: Vec<_> = current
            .issues
            .iter()
            .filter(|issue| issue.connection_id.as_deref() != Some(&connection.connection_id))
            .cloned()
            .collect();
        let pending_targets = if connection.status == ConnectionStatus::Connected {
            current.pending_targets.clone()
        } else {
            issues.push(Issue {
                kind: connection.kind.unwrap_or_default(),
                provider: connection.provider,
                connection_id: Some(connection.connection_id.clone()),
                target_id: None,
                message: connection.message.clone().unwrap_or_else(|| {
                    "Provider connection failed. Last known results may be stale.".into()
                }),
            });
            current
                .pending_targets
                .iter()
                .filter(|key| !targets.iter().any(|t| t.key() == **key))
                .cloned()
                .collect()
        };
        let query_health = if connection.status == ConnectionStatus::Connected {
            current.query_health.clone()
        } else {
            set_account_health(&current.query_health, targets, QueryHealthStatus::Error)
        };
        self.emit(
            Snapshot {
                connections,
                activities,
                history,
                issues,
                pending_targets,
                query_health,
                ..current
            },
            publisher,
        )
        .await;
        true
    }

    async fn publish_page<P: SnapshotPublisher>(
        &self,
        target: &Target,
        result: Result<ActivityPage, QueryError>,
        kind: PageKind,
        revision: u64,
        epoch: u64,
        publisher: &P,
    ) {
        let _publish = self.publish_lock.lock().await;
        if !self.current_generation(revision, epoch) {
            return;
        }
        let current = self.snapshot().await;
        let observed_at = Utc::now();
        let notifications = if kind != PageKind::History {
            match &result {
                Ok(page) if page.issues.is_empty() || !page.activities.is_empty() => {
                    let visible_activities = page
                        .activities
                        .iter()
                        .filter(|activity| {
                            !current
                                .settings
                                .hidden_activities
                                .iter()
                                .any(|hidden| hidden.matches(activity))
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    self.notification_engine.lock().await.observe(
                        target,
                        &visible_activities,
                        &current.settings.preferences,
                        epoch,
                        observed_at,
                    )
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let hidden_activities = current.settings.hidden_activities.clone();
        let mut history_cache = self.history_cache.lock().await;
        let mut updated = apply_page_with_history(
            current,
            target,
            result,
            kind,
            epoch,
            revision,
            &mut history_cache,
        );
        for delivery in notifications.into_iter().filter(|delivery| {
            !hidden_activities
                .iter()
                .any(|hidden| delivery.matches(hidden))
        }) {
            if let Err(message) = publisher.publish_notification(&delivery.notification) {
                self.notification_engine
                    .lock()
                    .await
                    .mark_failed(&delivery, epoch);
                updated.issues.retain(|issue| {
                    issue.connection_id.as_deref() != Some(&target.connection_id)
                        || issue.target_id.as_deref() != Some(&target.id)
                        || !issue.message.starts_with("Notification:")
                });
                updated.issues.push(Issue {
                    kind: IssueKind::Other,
                    provider: target.provider,
                    connection_id: Some(target.connection_id.clone()),
                    target_id: Some(target.id.clone()),
                    message: format!("Notification: {message}"),
                });
            } else {
                self.notification_engine.lock().await.acknowledge(&delivery);
            }
        }
        self.emit(updated, publisher).await;
    }

    pub async fn load_more<P: SnapshotPublisher>(
        &self,
        connection_id: &str,
        target_id: &str,
        cursor: &str,
        generation: u64,
        publisher: &P,
    ) -> Result<Snapshot, String> {
        let _history_guard = self.history_lock.lock().await;
        let (current, revision) = {
            let _publish = self.publish_lock.lock().await;
            (
                self.snapshot().await,
                self.settings_revision.load(Ordering::SeqCst),
            )
        };
        let history = current
            .history
            .iter()
            .find(|h| {
                h.connection_id == connection_id
                    && h.target_id == target_id
                    && h.next_cursor.as_deref() == Some(cursor)
                    && h.generation == generation
            })
            .ok_or_else(|| "The history page has changed. Refresh and try again.".to_owned())?;
        if current.activities.len() >= MAX_HISTORY_ACTIVITIES {
            return Err("History limit reached. Narrow the selected targets.".into());
        }
        let target = current
            .settings
            .targets
            .iter()
            .find(|t| {
                t.connection_id == connection_id
                    && t.id == target_id
                    && t.provider == history.provider
            })
            .cloned()
            .ok_or_else(|| "The selected target no longer exists.".to_owned())?;
        if !self.history_cache.lock().await.begin_load(
            &target,
            cursor,
            generation,
            revision,
            Utc::now(),
        ) {
            return Err("The history page has changed. Refresh and try again.".into());
        }
        let account = match self.account(connection_id, target.provider).await {
            Ok(account) => account,
            Err(message) => {
                self.fail_history_load(&target, cursor, generation).await;
                return Err(message);
            }
        };
        let cli = match self.tools.invocation_for(&account) {
            Ok(cli) => cli,
            Err(message) => {
                self.fail_history_load(&target, cursor, generation).await;
                return Err(message);
            }
        };
        let identity = current
            .connections
            .iter()
            .find(|c| c.connection_id == connection_id)
            .and_then(|c| c.account.clone());
        let authenticated = match tokio::time::timeout(
            REQUEST_TIMEOUT,
            catalog::connection(cli.as_ref(), &account, Some(&target)),
        )
        .await
        {
            Ok(authenticated) => authenticated,
            Err(_) => {
                self.fail_history_load(&target, cursor, generation).await;
                return Err(
                    "Connection check timed out. Refresh before loading more history.".into(),
                );
            }
        };
        if authenticated.status != ConnectionStatus::Connected || authenticated.account != identity
        {
            self.fail_history_load(&target, cursor, generation).await;
            return Err(
                "The CLI account changed or needs sign-in. Refresh before loading more history."
                    .into(),
            );
        }
        let page = match bounded_page(
            cli.as_ref(),
            &target,
            Some(cursor),
            identity.as_deref(),
            current.settings.history_hours,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => {
                self.fail_history_load(&target, cursor, generation).await;
                return Err(error.message);
            }
        };
        let _publish = self.publish_lock.lock().await;
        if self.settings_revision.load(Ordering::SeqCst) != revision {
            self.fail_history_load(&target, cursor, generation).await;
            return Err("Settings changed while loading history. Try again.".into());
        }
        let current = self.snapshot().await;
        let target_is_current = current
            .settings
            .targets
            .iter()
            .any(|candidate| candidate == &target);
        let identity_is_current = current
            .connections
            .iter()
            .find(|connection| connection.connection_id == target.connection_id)
            .and_then(|connection| connection.account.as_ref())
            == identity.as_ref();
        let mut history_cache = self.history_cache.lock().await;
        if !target_is_current
            || !identity_is_current
            || !history_cache.accepts_inflight(&target, cursor, generation, revision, Utc::now())
        {
            history_cache.mark_failed(&target, cursor, generation);
            drop(history_cache);
            self.request_refresh();
            return Err("The history page changed while loading. Try again.".into());
        }
        let next = apply_page_with_history(
            current,
            &target,
            Ok(page),
            PageKind::History,
            generation,
            revision,
            &mut history_cache,
        );
        self.emit(next.clone(), publisher).await;
        Ok(next)
    }

    async fn fail_history_load(&self, target: &Target, cursor: &str, generation: u64) {
        self.history_cache
            .lock()
            .await
            .mark_failed(target, cursor, generation);
        self.request_refresh();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PageKind {
    Recent,
    Active,
    History,
}

async fn bounded_page(
    cli: Option<&CliInvocation>,
    target: &Target,
    cursor: Option<&str>,
    identity: Option<&str>,
    hours: u32,
) -> Result<ActivityPage, QueryError> {
    tokio::time::timeout(
        REQUEST_TIMEOUT,
        catalog::page(cli, target, cursor, identity, hours),
    )
    .await
    .map_err(|_| {
        QueryError::new(
            IssueKind::TimedOut,
            "Run history lookup timed out. Last known results may be stale.",
        )
    })?
}

fn same_target(activity: &Activity, target: &Target) -> bool {
    activity.provider == target.provider
        && activity.connection_id == target.connection_id
        && activity.target_id == target.id
}

#[cfg(test)]
fn apply_page(
    current: Snapshot,
    target: &Target,
    result: Result<ActivityPage, QueryError>,
    kind: PageKind,
    epoch: u64,
) -> Snapshot {
    let mut history_cache = HistoryCache::default();
    apply_page_with_history(current, target, result, kind, epoch, 0, &mut history_cache)
}

#[allow(clippy::too_many_arguments)]
fn apply_page_with_history(
    current: Snapshot,
    target: &Target,
    result: Result<ActivityPage, QueryError>,
    kind: PageKind,
    epoch: u64,
    settings_revision: u64,
    history_cache: &mut HistoryCache,
) -> Snapshot {
    let mut issues = current.issues.clone();
    let mut history = current.history.clone();
    let mut activities = current.activities.clone();
    let mut last_updated_at = current.last_updated_at.clone();
    let mut query_health = current.query_health.clone();
    let outcome = QueryOutcome::from_result(&result);
    match result {
        Ok(mut page) => {
            let succeeded = page.issues.is_empty() || !page.activities.is_empty();
            let now = Utc::now();
            let cutoff = now - chrono::Duration::hours(i64::from(current.settings.history_hours));
            let mut history_entry = None;
            let mut loaded_ids = HashSet::new();
            let updates_recent_history =
                kind == PageKind::Recent && outcome != QueryOutcome::Failed;
            if updates_recent_history {
                let recent =
                    history_cache.recent(target, settings_revision, page.next_cursor.clone(), now);
                loaded_ids = recent.loaded_ids;
                history_entry = Some(recent.entry);
            }
            if kind == PageKind::Recent {
                issues.retain(|i| {
                    i.connection_id.as_deref() != Some(&target.connection_id)
                        || i.target_id.as_deref() != Some(&target.id)
                });
                if page.issues.is_empty() {
                    // Keep old active observations until the dedicated scan completes.
                    activities.retain(|a| {
                        !same_target(a, target)
                            || catalog::has_active_scan(target.provider)
                                && is_active_status(a.status)
                            || loaded_ids.contains(&a.id)
                                && is_history_status(a)
                                && in_history_range(a, cutoff)
                    });
                }
            }
            if kind == PageKind::Active && page.issues.is_empty() {
                let fresh_ids: HashSet<_> = page.activities.iter().map(|a| a.id.as_str()).collect();
                // A disappeared active record is not a fabricated success; remove the stale observation.
                activities.retain(|a| {
                    !same_target(a, target)
                        || !is_active_status(a.status)
                        || fresh_ids.contains(a.id.as_str())
                });
            }
            let mut committed_history_ids = Vec::new();
            let mut history_limit_reached = false;
            for activity in page.activities {
                if kind == PageKind::History
                    && (!is_history_status(&activity) || !in_history_range(&activity, cutoff))
                {
                    continue;
                }
                let existing = activities.iter().position(|a| {
                    a.connection_id == activity.connection_id
                        && a.provider == activity.provider
                        && a.target_id == activity.target_id
                        && a.id == activity.id
                });
                if let Some(index) = existing {
                    if kind != PageKind::History {
                        activities[index] =
                            newer_observation(&activities[index], &activity).clone();
                    }
                    if kind == PageKind::History {
                        committed_history_ids.push(activity.id);
                    }
                } else if kind == PageKind::History && activities.len() >= MAX_HISTORY_ACTIVITIES {
                    history_limit_reached = true;
                } else {
                    if kind == PageKind::History {
                        committed_history_ids.push(activity.id.clone());
                    }
                    activities.push(activity);
                }
            }
            if updates_recent_history {
                let retained_loaded_ids = activities
                    .iter()
                    .filter(|activity| {
                        same_target(activity, target)
                            && loaded_ids.contains(&activity.id)
                            && is_history_status(activity)
                            && in_history_range(activity, cutoff)
                    })
                    .map(|activity| activity.id.clone())
                    .collect();
                history_cache.replace_loaded_ids(target, retained_loaded_ids);
            }
            if kind == PageKind::History {
                if history_limit_reached {
                    page.next_cursor = None;
                    page.issues.push(Issue {
                        kind: IssueKind::Other,
                        provider: target.provider,
                        connection_id: Some(target.connection_id.clone()),
                        target_id: Some(target.id.clone()),
                        message: "History limit reached. Narrow the selected targets.".into(),
                    });
                }
                history_entry = history_cache.commit(
                    target,
                    epoch,
                    committed_history_ids,
                    page.next_cursor.clone(),
                    now,
                );
            }
            extend_issues(&mut issues, page.issues);
            if let Some(history_entry) = history_entry {
                history.retain(|h| {
                    h.connection_id != target.connection_id
                        || h.target_id != target.id
                        || h.provider != target.provider
                });
                history.push(history_entry);
            }
            if succeeded {
                last_updated_at = Some(Utc::now().to_rfc3339());
            }
        }
        Err(error) => extend_issues(
            &mut issues,
            vec![Issue {
                kind: error.kind,
                provider: target.provider,
                connection_id: Some(target.connection_id.clone()),
                target_id: Some(target.id.clone()),
                message: error.message,
            }],
        ),
    }
    activities.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let pending_targets = if kind == PageKind::Active
        || kind == PageKind::Recent && !catalog::has_active_scan(target.provider)
    {
        current
            .pending_targets
            .iter()
            .filter(|key| **key != target.key())
            .cloned()
            .collect()
    } else {
        current.pending_targets.clone()
    };
    if kind != PageKind::History {
        query_health = apply_query_outcome(&query_health, target, kind, outcome);
    }
    Snapshot {
        activities,
        history,
        issues,
        pending_targets,
        last_updated_at,
        query_health,
        ..current
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueryOutcome {
    Full,
    Partial,
    Failed,
}

impl QueryOutcome {
    fn from_result(result: &Result<ActivityPage, QueryError>) -> Self {
        match result {
            Ok(page) if page.issues.is_empty() => Self::Full,
            Ok(page) if !page.activities.is_empty() => Self::Partial,
            Ok(_) | Err(_) => Self::Failed,
        }
    }
}

fn checking_query_health(
    targets: &[Target],
    previous: &[QueryHealth],
    attempted_at: &str,
) -> Vec<QueryHealth> {
    targets
        .iter()
        .map(|target| QueryHealth {
            provider: target.provider,
            connection_id: target.connection_id.clone(),
            target_id: target.id.clone(),
            status: QueryHealthStatus::Checking,
            last_attempt_at: attempted_at.to_owned(),
            last_successful_at: previous
                .iter()
                .find(|health| same_health_target(health, target))
                .and_then(|health| health.last_successful_at.clone()),
        })
        .collect()
}

fn set_account_health(
    current: &[QueryHealth],
    targets: &[Target],
    status: QueryHealthStatus,
) -> Vec<QueryHealth> {
    current
        .iter()
        .map(|health| {
            if targets
                .iter()
                .any(|target| same_health_target(health, target))
            {
                QueryHealth {
                    status,
                    ..health.clone()
                }
            } else {
                health.clone()
            }
        })
        .collect()
}

fn apply_query_outcome(
    current: &[QueryHealth],
    target: &Target,
    kind: PageKind,
    outcome: QueryOutcome,
) -> Vec<QueryHealth> {
    let completed_at = Utc::now().to_rfc3339();
    current
        .iter()
        .map(|health| {
            if !same_health_target(health, target) {
                return health.clone();
            }
            let status = query_status_after(health.status, target.provider, kind, outcome);
            QueryHealth {
                status,
                last_successful_at: (status == QueryHealthStatus::Success)
                    .then(|| completed_at.clone())
                    .or_else(|| health.last_successful_at.clone()),
                ..health.clone()
            }
        })
        .collect()
}

fn query_status_after(
    current: QueryHealthStatus,
    provider: Provider,
    kind: PageKind,
    outcome: QueryOutcome,
) -> QueryHealthStatus {
    match kind {
        PageKind::History => current,
        PageKind::Recent if catalog::has_active_scan(provider) => match outcome {
            QueryOutcome::Full => QueryHealthStatus::Checking,
            QueryOutcome::Partial => QueryHealthStatus::Partial,
            QueryOutcome::Failed => QueryHealthStatus::Error,
        },
        PageKind::Recent => outcome_status(outcome),
        PageKind::Active => match (current, outcome) {
            (QueryHealthStatus::Checking, QueryOutcome::Full) => QueryHealthStatus::Success,
            (QueryHealthStatus::Error, QueryOutcome::Failed) => QueryHealthStatus::Error,
            _ => QueryHealthStatus::Partial,
        },
    }
}

fn outcome_status(outcome: QueryOutcome) -> QueryHealthStatus {
    match outcome {
        QueryOutcome::Full => QueryHealthStatus::Success,
        QueryOutcome::Partial => QueryHealthStatus::Partial,
        QueryOutcome::Failed => QueryHealthStatus::Error,
    }
}

fn same_health_target(health: &QueryHealth, target: &Target) -> bool {
    health.provider == target.provider
        && health.connection_id == target.connection_id
        && health.target_id == target.id
}

fn snapshot_needs_followup(snapshot: &Snapshot, now: chrono::DateTime<Utc>) -> bool {
    let stale_after_seconds = snapshot.poll_interval_seconds.saturating_mul(2).max(60);
    let stale_after =
        chrono::Duration::seconds(i64::try_from(stale_after_seconds).unwrap_or(i64::MAX));
    snapshot.settings.targets.iter().any(|target| {
        let Some(health) = snapshot
            .query_health
            .iter()
            .find(|health| same_health_target(health, target))
        else {
            return true;
        };
        if health.status != QueryHealthStatus::Success {
            return true;
        }
        health
            .last_successful_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .is_none_or(|successful_at| now.signed_duration_since(successful_at) > stale_after)
    })
}

async fn bounded_active(
    cli: Option<&CliInvocation>,
    target: &Target,
    identity: Option<&str>,
) -> Result<ActivityPage, QueryError> {
    tokio::time::timeout(REQUEST_TIMEOUT, catalog::active(cli, target, identity))
        .await
        .map_err(|_| {
            QueryError::new(
                IssueKind::TimedOut,
                "Active run lookup timed out; the recent list is partial.",
            )
        })?
}

/// Adds issues while keeping one issue per target and classified cause, so a
/// failed recent page and its active scan do not report the same cause twice.
fn extend_issues(issues: &mut Vec<Issue>, added: Vec<Issue>) {
    for issue in added {
        if !issues.iter().any(|known| known.same_cause(&issue)) {
            issues.push(issue);
        }
    }
}

fn in_history_range(activity: &Activity, cutoff: chrono::DateTime<Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(&activity.updated_at)
        .map(|updated| updated.with_timezone(&Utc) >= cutoff)
        .unwrap_or(false)
}

fn is_history_status(activity: &Activity) -> bool {
    is_terminal_status(activity.status)
        || crate::panel::is_blocked(activity)
        || !is_active_status(activity.status) && activity.finished_at.is_some()
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
