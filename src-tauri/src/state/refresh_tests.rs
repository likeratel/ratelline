use super::*;
use crate::models::TargetHistory;
use std::sync::{
    atomic::{AtomicUsize, Ordering as AtomicOrdering},
    Mutex as StdMutex,
};

#[cfg(unix)]
static SLOW_GITHUB_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn target(name: &str) -> Target {
    Target {
        id: format!("sample/{name}"),
        provider: Provider::Github,
        connection_id: "github:local".into(),
        name: name.into(),
        description: None,
        metadata: HashMap::new(),
    }
}
fn activity(target: &Target, id: &str, status: ActivityStatus) -> Activity {
    Activity {
        id: id.into(),
        provider: target.provider,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: "workflow".into(),
        title: "Deploy".into(),
        project: target.name.clone(),
        branch: None,
        url: "https://github.com/sample/project/actions/runs/1".into(),
        status,
        raw_status: "fixture".into(),
        actor: None,
        ownership: Ownership::Unknown,
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        updated_at: Utc::now().to_rfc3339(),
    }
}
fn health(
    target: &Target,
    status: QueryHealthStatus,
    last_successful_at: Option<&str>,
) -> QueryHealth {
    QueryHealth {
        provider: target.provider,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        status,
        last_attempt_at: "2026-10-06T00:00:00Z".into(),
        last_successful_at: last_successful_at.map(str::to_owned),
    }
}
fn state() -> AppState {
    let directory = std::env::temp_dir().join(format!("ratelline-stream-{}", uuid::Uuid::new_v4()));
    AppState {
        tools: Toolchain::unavailable_for_test(&directory),
        store: SettingsStore::new(directory),
        snapshot: RwLock::new(Snapshot::initial(Settings::default())),
        refresh_lock: Mutex::new(()),
        refresh_wake: RefreshWake::default(),
        publish_lock: Mutex::new(()),
        settings_revision: AtomicU64::new(0),
        refresh_epoch: AtomicU64::new(1),
        history_lock: Mutex::new(()),
        history_cache: Mutex::new(HistoryCache::default()),
        notification_engine: Mutex::new(NotificationEngine::default()),
        login_statuses: Mutex::new(HashMap::new()),
    }
}
#[derive(Default)]
struct Recorder(StdMutex<Vec<Snapshot>>);
impl SnapshotPublisher for Recorder {
    fn publish_snapshot(&self, snapshot: &Snapshot) {
        self.0.lock().unwrap().push(snapshot.clone());
    }
}

#[derive(Default)]
struct FailOncePublisher {
    attempts: AtomicUsize,
    snapshots: StdMutex<Vec<Snapshot>>,
}

impl SnapshotPublisher for FailOncePublisher {
    fn publish_snapshot(&self, snapshot: &Snapshot) {
        self.snapshots.lock().unwrap().push(snapshot.clone());
    }

    fn publish_notification(&self, _notification: &NativeNotification) -> Result<(), String> {
        if self.attempts.fetch_add(1, AtomicOrdering::SeqCst) == 0 {
            Err("synthetic notification failure".into())
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn warning_rows_notify_and_failed_native_delivery_retries_next_generation() {
    let state = state();
    let selected = target("warning");
    let mut settings = Settings {
        targets: vec![selected.clone()],
        ..Settings::default()
    };
    settings.preferences.notifications.enabled = true;
    settings.preferences.notifications.rules = vec![crate::preferences::NotificationRule {
        target_key: selected.key(),
        scope: crate::preferences::NotificationScope::All,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    }];
    state.snapshot.write().await.settings = settings;
    let publisher = FailOncePublisher::default();
    let running = activity(&selected, "run", ActivityStatus::Running);

    state.refresh_epoch.store(1, Ordering::SeqCst);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![running.clone()],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            0,
            1,
            &publisher,
        )
        .await;

    let provider_warning = Issue {
        provider: selected.provider,
        connection_id: Some(selected.connection_id.clone()),
        target_id: Some(selected.id.clone()),
        message: "Synthetic provider warning".into(),
    };
    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..running
    };
    state.refresh_epoch.store(2, Ordering::SeqCst);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![failed.clone()],
                next_cursor: None,
                issues: vec![provider_warning.clone()],
            }),
            PageKind::Recent,
            0,
            2,
            &publisher,
        )
        .await;
    assert_eq!(publisher.attempts.load(AtomicOrdering::SeqCst), 1);
    assert!(state
        .snapshot()
        .await
        .issues
        .iter()
        .any(|issue| issue.message.contains("synthetic notification failure")));

    state.refresh_epoch.store(3, Ordering::SeqCst);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![failed],
                next_cursor: None,
                issues: vec![provider_warning],
            }),
            PageKind::Recent,
            0,
            3,
            &publisher,
        )
        .await;
    assert_eq!(publisher.attempts.load(AtomicOrdering::SeqCst), 2);
}

#[tokio::test]
async fn hidden_activity_status_updates_do_not_publish_notifications() {
    let state = state();
    let selected = target("hidden-notification");
    let mut settings = Settings {
        targets: vec![selected.clone()],
        ..Settings::default()
    };
    settings.preferences.notifications.enabled = true;
    settings.preferences.notifications.rules = vec![crate::preferences::NotificationRule {
        target_key: selected.key(),
        scope: crate::preferences::NotificationScope::All,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    }];
    state.snapshot.write().await.settings = settings;
    let publisher = FailOncePublisher::default();
    let running = activity(&selected, "hidden-run", ActivityStatus::Running);

    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![running.clone()],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            0,
            1,
            &publisher,
        )
        .await;
    state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: crate::models::ActivityKey::from_activity(&running),
                hidden: true,
            },
            &publisher,
        )
        .await
        .expect("hide activity");

    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..running
    };
    let revision = state.settings_revision.load(Ordering::SeqCst);
    state.refresh_epoch.store(2, Ordering::SeqCst);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![failed],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            revision,
            2,
            &publisher,
        )
        .await;

    assert_eq!(publisher.attempts.load(AtomicOrdering::SeqCst), 0);
    assert_eq!(
        state.snapshot().await.activities[0].status,
        ActivityStatus::Failed
    );
}

#[tokio::test]
async fn hiding_suppresses_a_previously_pending_notification_retry() {
    let state = state();
    let selected = target("hidden-pending");
    let mut settings = Settings {
        targets: vec![selected.clone()],
        ..Settings::default()
    };
    settings.preferences.notifications.enabled = true;
    settings.preferences.notifications.rules = vec![crate::preferences::NotificationRule {
        target_key: selected.key(),
        scope: crate::preferences::NotificationScope::All,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    }];
    state.snapshot.write().await.settings = settings;
    let publisher = FailOncePublisher::default();
    let running = activity(&selected, "pending-run", ActivityStatus::Running);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![running.clone()],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            0,
            1,
            &publisher,
        )
        .await;
    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..running
    };
    state.refresh_epoch.store(2, Ordering::SeqCst);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![failed.clone()],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            0,
            2,
            &publisher,
        )
        .await;
    assert_eq!(publisher.attempts.load(AtomicOrdering::SeqCst), 1);

    state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: crate::models::ActivityKey::from_activity(&failed),
                hidden: true,
            },
            &publisher,
        )
        .await
        .expect("hide failed activity");
    let revision = state.settings_revision.load(Ordering::SeqCst);
    state.refresh_epoch.store(3, Ordering::SeqCst);
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![failed],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            revision,
            3,
            &publisher,
        )
        .await;

    assert_eq!(publisher.attempts.load(AtomicOrdering::SeqCst), 1);
}

#[tokio::test]
async fn hide_and_undo_preserve_history_and_accept_in_flight_pages() {
    let state = state();
    let selected = target("visibility-generation");
    let running = activity(&selected, "visibility-run", ActivityStatus::Running);
    let generation = 7;
    let pending = selected.key();
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![selected.clone()];
        snapshot.activities = vec![running.clone()];
        snapshot.history = vec![TargetHistory {
            connection_id: selected.connection_id.clone(),
            target_id: selected.id.clone(),
            provider: selected.provider,
            next_cursor: Some("2".into()),
            checked_at: Utc::now().to_rfc3339(),
            generation,
        }];
        snapshot.pending_targets = vec![pending.clone()];
        snapshot.refreshing = true;
    }
    state.refresh_epoch.store(generation, Ordering::SeqCst);
    let recorder = Recorder::default();
    let key = crate::models::ActivityKey::from_activity(&running);

    state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: key.clone(),
                hidden: true,
            },
            &recorder,
        )
        .await
        .expect("hide activity");
    let hidden = state.snapshot().await;
    assert_eq!(state.settings_revision.load(Ordering::SeqCst), 0);
    assert_eq!(hidden.history[0].next_cursor.as_deref(), Some("2"));
    assert_eq!(hidden.pending_targets, vec![pending.clone()]);
    assert!(hidden.refreshing);

    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..running.clone()
    };
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![failed],
                next_cursor: Some("3".into()),
                issues: Vec::new(),
            }),
            PageKind::Recent,
            0,
            generation,
            &recorder,
        )
        .await;
    assert_eq!(
        state.snapshot().await.activities[0].status,
        ActivityStatus::Failed
    );

    state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: key,
                hidden: false,
            },
            &recorder,
        )
        .await
        .expect("restore activity");
    let restored = state.snapshot().await;
    assert_eq!(state.settings_revision.load(Ordering::SeqCst), 0);
    assert_eq!(restored.history[0].next_cursor.as_deref(), Some("3"));
    assert_eq!(restored.pending_targets, vec![pending]);
    assert!(restored.refreshing);

    let success = Activity {
        status: ActivityStatus::Success,
        updated_at: (Utc::now() + chrono::Duration::seconds(2)).to_rfc3339(),
        ..running
    };
    state
        .publish_page(
            &selected,
            Ok(ActivityPage {
                activities: vec![success],
                next_cursor: Some("4".into()),
                issues: Vec::new(),
            }),
            PageKind::Recent,
            0,
            generation,
            &recorder,
        )
        .await;
    assert_eq!(
        state.snapshot().await.activities[0].status,
        ActivityStatus::Success
    );
}

#[cfg(unix)]
#[tokio::test]
async fn synthetic_slow_target_does_not_block_first_page_publication() {
    let _serial = SLOW_GITHUB_TEST_LOCK.lock().await;
    let server = FakeGithub::start(false);
    let state = AppState {
        tools: Toolchain::github_for_test(
            &server.directory,
            CliInvocation::for_test(server.script.clone()),
        ),
        ..state()
    };
    let targets = server.configure(&state, &["fast", "slow"]).await;
    let fast = targets[0].clone();
    struct TimedRecorder {
        started: std::time::Instant,
        pages: StdMutex<Vec<(u128, Snapshot)>>,
    }
    impl SnapshotPublisher for TimedRecorder {
        fn publish_snapshot(&self, snapshot: &Snapshot) {
            self.pages
                .lock()
                .unwrap()
                .push((self.started.elapsed().as_millis(), snapshot.clone()));
        }
    }
    let recorder = TimedRecorder {
        started: std::time::Instant::now(),
        pages: StdMutex::new(Vec::new()),
    };
    let done = state.refresh(&recorder).await;
    assert!(!done.refreshing);
    let pages = recorder.pages.lock().unwrap();
    let first = pages
        .iter()
        .find(|(_, s)| s.history.iter().any(|h| h.target_id == fast.id))
        .unwrap();
    assert_eq!(
        first.1.history.len(),
        1,
        "fast result must publish before slow target finishes"
    );
    assert!(first.1.refreshing);
    let fast_complete = pages
        .iter()
        .find(|(_, snapshot)| {
            snapshot.refreshing
                && snapshot.history.iter().any(|h| h.target_id == fast.id)
                && !snapshot.pending_targets.contains(&fast.key())
        })
        .expect("fast target must complete its active scan during refresh");
    assert_eq!(
        fast_complete.1.history.len(),
        1,
        "slow recent lookup must not block the fast target's active scan"
    );
    let calls = std::fs::read_to_string(server.script.with_extension("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.contains("per_page=20"))
            .count(),
        2
    );
    assert!(!calls.contains("&page=2"));
    eprintln!("synthetic streaming: first_page={}ms fast_active={}ms all_targets={}ms recent_calls=2 active_calls=10 (slow request=300ms)",first.0,fast_complete.0,recorder.started.elapsed().as_millis());
}

#[test]
fn recent_page_preserves_other_targets_and_active_until_scan_finishes() {
    let a = target("a");
    let b = target("b");
    let current = Snapshot {
        activities: vec![
            activity(&a, "old", ActivityStatus::Success),
            activity(&a, "active", ActivityStatus::Running),
            activity(&b, "other", ActivityStatus::Success),
        ],
        pending_targets: vec![a.key(), b.key()],
        ..Snapshot::initial(Settings::default())
    };
    let next = apply_page(
        current,
        &a,
        Ok(ActivityPage {
            activities: vec![activity(&a, "recent", ActivityStatus::Success)],
            next_cursor: Some("2".into()),
            issues: vec![],
        }),
        PageKind::Recent,
        1,
    );
    assert_eq!(next.activities.len(), 3);
    assert!(!next.activities.iter().any(|a| a.id == "old"));
    assert_eq!(next.pending_targets.len(), 2);
    assert_eq!(next.history[0].next_cursor.as_deref(), Some("2"));
    let done = apply_page(next, &a, Ok(ActivityPage::default()), PageKind::Active, 1);
    assert!(!done.activities.iter().any(|a| a.id == "active"));
    assert_eq!(done.pending_targets, vec![b.key()]);
}

#[test]
fn loaded_history_survives_poll_and_newer_recent_observation_wins() {
    let selected = target("loaded-history");
    let mut cache = HistoryCache::default();
    let current = Snapshot {
        settings: Settings {
            targets: vec![selected.clone()],
            ..Settings::default()
        },
        ..Snapshot::initial(Settings::default())
    };
    let recent = apply_page_with_history(
        current,
        &selected,
        Ok(ActivityPage {
            activities: vec![activity(&selected, "recent", ActivityStatus::Success)],
            next_cursor: Some("2".into()),
            issues: Vec::new(),
        }),
        PageKind::Recent,
        10,
        4,
        &mut cache,
    );
    let generation = recent.history[0].generation;
    assert!(cache.begin_load(&selected, "2", generation, 4, Utc::now()));
    let mut historical = activity(&selected, "older", ActivityStatus::Success);
    historical.title = "historical value".into();
    historical.updated_at = (Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();
    let loaded = apply_page_with_history(
        recent,
        &selected,
        Ok(ActivityPage {
            activities: vec![historical],
            next_cursor: Some("3".into()),
            issues: Vec::new(),
        }),
        PageKind::History,
        generation,
        4,
        &mut cache,
    );
    let mut fresh = activity(&selected, "older", ActivityStatus::Success);
    fresh.title = "newer recent value".into();
    fresh.updated_at = Utc::now().to_rfc3339();
    let polled = apply_page_with_history(
        loaded,
        &selected,
        Ok(ActivityPage {
            activities: vec![
                activity(&selected, "new-recent", ActivityStatus::Success),
                fresh,
            ],
            next_cursor: Some("provider-restarted-at-2".into()),
            issues: Vec::new(),
        }),
        PageKind::Recent,
        11,
        4,
        &mut cache,
    );

    assert_eq!(polled.history[0].generation, generation);
    assert_eq!(polled.history[0].next_cursor.as_deref(), Some("3"));
    assert_eq!(
        polled
            .activities
            .iter()
            .find(|activity| activity.id == "older")
            .unwrap()
            .title,
        "newer recent value"
    );
}

#[test]
fn historical_active_rows_are_ignored_and_merge_stops_at_global_limit() {
    let selected = target("history-limit");
    let mut cache = HistoryCache::default();
    let session = cache.recent(&selected, 0, Some("2".into()), Utc::now());
    assert!(cache.begin_load(&selected, "2", session.entry.generation, 0, Utc::now()));
    let activities = (0..MAX_HISTORY_ACTIVITIES - 1)
        .map(|index| {
            activity(
                &selected,
                &format!("existing-{index}"),
                ActivityStatus::Success,
            )
        })
        .collect();
    let current = Snapshot {
        settings: Settings {
            targets: vec![selected.clone()],
            ..Settings::default()
        },
        activities,
        history: vec![session.entry.clone()],
        ..Snapshot::initial(Settings::default())
    };
    let next = apply_page_with_history(
        current,
        &selected,
        Ok(ActivityPage {
            activities: vec![
                activity(&selected, "stale-running", ActivityStatus::Running),
                activity(&selected, "last-slot", ActivityStatus::Success),
                activity(&selected, "over-limit", ActivityStatus::Success),
            ],
            next_cursor: Some("3".into()),
            issues: Vec::new(),
        }),
        PageKind::History,
        session.entry.generation,
        0,
        &mut cache,
    );

    assert_eq!(next.activities.len(), MAX_HISTORY_ACTIVITIES);
    assert!(!next
        .activities
        .iter()
        .any(|activity| activity.id == "stale-running"));
    assert!(next
        .activities
        .iter()
        .any(|activity| activity.id == "last-slot"));
    assert!(!next
        .activities
        .iter()
        .any(|activity| activity.id == "over-limit"));
    assert!(next.history[0].next_cursor.is_none());
    assert!(next
        .issues
        .iter()
        .any(|issue| issue.message.contains("History limit reached")));
}

#[tokio::test]
async fn account_identity_change_clears_loaded_history_and_chain() {
    let state = state();
    let selected = target("identity-bound-history");
    let old_connection = Connection {
        connection_id: selected.connection_id.clone(),
        provider: selected.provider,
        status: ConnectionStatus::Connected,
        account: Some("old-user".into()),
        message: None,
        cli_version: Some("fixture".into()),
    };
    let history =
        state
            .history_cache
            .lock()
            .await
            .recent(&selected, 0, Some("2".into()), Utc::now());
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![selected.clone()];
        snapshot.connections = vec![old_connection];
        snapshot.activities = vec![activity(&selected, "older", ActivityStatus::Success)];
        snapshot.history = vec![history.entry.clone()];
    }
    let replacement = Connection {
        connection_id: selected.connection_id.clone(),
        provider: selected.provider,
        status: ConnectionStatus::Connected,
        account: Some("new-user".into()),
        message: None,
        cli_version: Some("fixture".into()),
    };

    assert!(
        state
            .publish_connection(
                replacement,
                std::slice::from_ref(&selected),
                0,
                1,
                &Recorder::default()
            )
            .await
    );
    let snapshot = state.snapshot().await;
    assert!(snapshot.activities.is_empty());
    assert!(snapshot.history.is_empty());
    assert!(!state.history_cache.lock().await.begin_load(
        &selected,
        "2",
        history.entry.generation,
        0,
        Utc::now()
    ));
}

#[test]
fn failed_recent_page_does_not_clear_a_pending_history_reset() {
    let selected = target("failed-recent-history-reset");
    let mut cache = HistoryCache::default();
    let initial = cache.recent(&selected, 7, Some("2".into()), Utc::now());
    assert!(cache.begin_load(&selected, "2", initial.entry.generation, 7, Utc::now()));
    let committed = cache
        .commit(
            &selected,
            initial.entry.generation,
            ["older".into()],
            Some("3".into()),
            Utc::now(),
        )
        .unwrap();
    assert!(cache.begin_load(&selected, "3", committed.generation, 7, Utc::now()));
    cache.mark_failed(&selected, "3", committed.generation);

    let current = Snapshot {
        settings: Settings {
            targets: vec![selected.clone()],
            ..Settings::default()
        },
        activities: vec![activity(&selected, "older", ActivityStatus::Success)],
        history: vec![committed.clone()],
        ..Snapshot::initial(Settings::default())
    };
    let failed = apply_page_with_history(
        current,
        &selected,
        Ok(ActivityPage {
            activities: Vec::new(),
            next_cursor: None,
            issues: vec![Issue {
                provider: selected.provider,
                connection_id: Some(selected.connection_id.clone()),
                target_id: Some(selected.id.clone()),
                message: "provider unavailable".into(),
            }],
        }),
        PageKind::Recent,
        99,
        7,
        &mut cache,
    );

    assert!(cache.needs_followup());
    assert_eq!(failed.history, vec![committed]);
    assert!(!cache.begin_load(&selected, "3", initial.entry.generation, 7, Utc::now()));
}

#[test]
fn blocked_observation_replaces_cached_running_even_if_active_scan_fails() {
    let t = Target {
        provider: Provider::Vercel,
        ..target("a")
    };
    let old = activity(&t, "deployment", ActivityStatus::Running);
    let blocked = Activity {
        status: ActivityStatus::Skipped,
        raw_status: "BLOCKED".into(),
        ..old.clone()
    };
    let current = Snapshot {
        activities: vec![old],
        ..Snapshot::initial(Settings::default())
    };
    let recent = apply_page(
        current,
        &t,
        Ok(ActivityPage {
            activities: vec![blocked],
            ..ActivityPage::default()
        }),
        PageKind::Recent,
        1,
    );
    let next = apply_page(
        recent,
        &t,
        Err("synthetic network failure".into()),
        PageKind::Active,
        1,
    );
    assert_eq!(next.activities[0].raw_status, "BLOCKED");
    assert!(!crate::panel::visible_activity(
        &next.activities[0],
        Utc::now()
    ));
}

#[test]
fn failed_scan_preserves_cache_and_reports_partial_result() {
    let t = target("a");
    let current = Snapshot {
        activities: vec![activity(&t, "active", ActivityStatus::Running)],
        ..Snapshot::initial(Settings::default())
    };
    let next = apply_page(
        current,
        &t,
        Err("synthetic timeout".into()),
        PageKind::Active,
        1,
    );
    assert_eq!(next.activities.len(), 1);
    assert_eq!(next.issues.len(), 1);
    assert!(next.last_updated_at.is_none());
}

#[test]
fn mixed_target_results_keep_health_isolated() {
    let succeeded = target("succeeded");
    let failed = target("failed");
    let current = Snapshot {
        query_health: checking_query_health(
            &[succeeded.clone(), failed.clone()],
            &[],
            "2026-10-06T00:00:00Z",
        ),
        ..Snapshot::initial(Settings::default())
    };
    let current = apply_page(
        current,
        &succeeded,
        Ok(ActivityPage::default()),
        PageKind::Recent,
        1,
    );
    let current = apply_page(
        current,
        &failed,
        Err("synthetic recent failure".into()),
        PageKind::Recent,
        1,
    );
    let current = apply_page(
        current,
        &succeeded,
        Ok(ActivityPage::default()),
        PageKind::Active,
        1,
    );
    let done = apply_page(
        current,
        &failed,
        Err("synthetic active failure".into()),
        PageKind::Active,
        1,
    );

    let succeeded_health = done
        .query_health
        .iter()
        .find(|entry| entry.target_id == succeeded.id)
        .unwrap();
    let failed_health = done
        .query_health
        .iter()
        .find(|entry| entry.target_id == failed.id)
        .unwrap();
    assert_eq!(succeeded_health.status, QueryHealthStatus::Success);
    assert!(succeeded_health.last_successful_at.is_some());
    assert_eq!(failed_health.status, QueryHealthStatus::Error);
    assert!(failed_health.last_successful_at.is_none());
}

#[test]
fn partial_and_active_failures_preserve_last_full_success_until_recovery() {
    let selected = target("health");
    let previous_success = "2026-10-05T00:00:00Z";
    let initial = Snapshot {
        query_health: vec![health(
            &selected,
            QueryHealthStatus::Success,
            Some(previous_success),
        )],
        ..Snapshot::initial(Settings::default())
    };
    let checking = Snapshot {
        query_health: checking_query_health(
            std::slice::from_ref(&selected),
            &initial.query_health,
            "2026-10-06T00:00:00Z",
        ),
        ..initial
    };
    assert_eq!(checking.query_health[0].status, QueryHealthStatus::Checking);
    assert_eq!(
        checking.query_health[0].last_successful_at.as_deref(),
        Some(previous_success)
    );

    let recent = apply_page(
        checking,
        &selected,
        Ok(ActivityPage::default()),
        PageKind::Recent,
        1,
    );
    let active_failed = apply_page(
        recent,
        &selected,
        Err("synthetic active failure".into()),
        PageKind::Active,
        1,
    );
    assert_eq!(
        active_failed.query_health[0].status,
        QueryHealthStatus::Partial
    );
    assert_eq!(
        active_failed.query_health[0].last_successful_at.as_deref(),
        Some(previous_success)
    );

    let checking = Snapshot {
        query_health: checking_query_health(
            std::slice::from_ref(&selected),
            &active_failed.query_health,
            "2026-10-06T00:02:00Z",
        ),
        ..active_failed
    };
    let recovered = apply_page(
        apply_page(
            checking,
            &selected,
            Ok(ActivityPage::default()),
            PageKind::Recent,
            2,
        ),
        &selected,
        Ok(ActivityPage::default()),
        PageKind::Active,
        2,
    );
    assert_eq!(recovered.query_health[0].status, QueryHealthStatus::Success);
    assert_ne!(
        recovered.query_health[0].last_successful_at.as_deref(),
        Some(previous_success)
    );
    chrono::DateTime::parse_from_rfc3339(
        recovered.query_health[0]
            .last_successful_at
            .as_deref()
            .unwrap(),
    )
    .expect("last successful timestamp must be RFC 3339");
}

#[test]
fn partial_recent_rows_remain_partial_after_clean_active_scan() {
    let selected = target("partial");
    let previous_success = "2026-10-05T00:00:00Z";
    let current = Snapshot {
        query_health: vec![health(
            &selected,
            QueryHealthStatus::Checking,
            Some(previous_success),
        )],
        ..Snapshot::initial(Settings::default())
    };
    let recent = apply_page(
        current,
        &selected,
        Ok(ActivityPage {
            activities: vec![activity(&selected, "valid", ActivityStatus::Success)],
            issues: vec![Issue {
                provider: selected.provider,
                connection_id: Some(selected.connection_id.clone()),
                target_id: Some(selected.id.clone()),
                message: "one page failed".into(),
            }],
            ..ActivityPage::default()
        }),
        PageKind::Recent,
        1,
    );
    let done = apply_page(
        recent,
        &selected,
        Ok(ActivityPage::default()),
        PageKind::Active,
        1,
    );
    assert_eq!(done.query_health[0].status, QueryHealthStatus::Partial);
    assert_eq!(
        done.query_health[0].last_successful_at.as_deref(),
        Some(previous_success)
    );
}

#[test]
fn history_pages_do_not_change_current_query_health() {
    let selected = target("history-health");
    let health = health(
        &selected,
        QueryHealthStatus::Partial,
        Some("2026-10-05T00:00:00Z"),
    );
    let current = Snapshot {
        query_health: vec![health.clone()],
        ..Snapshot::initial(Settings::default())
    };
    let next = apply_page(
        current,
        &selected,
        Ok(ActivityPage::default()),
        PageKind::History,
        1,
    );
    assert_eq!(next.query_health, vec![health]);
}

#[test]
fn account_failure_only_marks_its_own_targets() {
    let first = target("first");
    let second = Target {
        connection_id: "github:second".into(),
        ..target("second")
    };
    let current = vec![
        health(&first, QueryHealthStatus::Checking, None),
        health(&second, QueryHealthStatus::Checking, None),
    ];
    let next = set_account_health(
        &current,
        std::slice::from_ref(&first),
        QueryHealthStatus::Error,
    );
    assert_eq!(next[0].status, QueryHealthStatus::Error);
    assert_eq!(next[1].status, QueryHealthStatus::Checking);
}

#[tokio::test]
async fn inflight_request_retries_once_for_unhealthy_targets_but_not_after_full_success() {
    let failed = target("failed-before-online");
    let pending = target("still-pending");
    let unhealthy = Snapshot {
        settings: Settings {
            targets: vec![failed.clone(), pending.clone()],
            ..Settings::default()
        },
        poll_interval_seconds: 120,
        pending_targets: vec![pending.key()],
        query_health: vec![
            health(&failed, QueryHealthStatus::Error, None),
            health(&pending, QueryHealthStatus::Checking, None),
        ],
        ..Snapshot::initial(Settings::default())
    };
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-06T00:02:00Z")
        .unwrap()
        .with_timezone(&Utc);
    assert!(snapshot_needs_followup(&unhealthy, now));

    let wake = RefreshWake::default();
    let refresh_lock = Mutex::new(());
    let active = wake.begin(refresh_lock.lock().await);
    wake.request();
    wake.request();
    active.finish(snapshot_needs_followup(&unhealthy, now));
    assert!(wake.take_pending());
    assert!(!wake.take_pending(), "bursts must promote only one retry");

    let retry = wake.begin(refresh_lock.lock().await);
    retry.finish(snapshot_needs_followup(&unhealthy, now));
    assert!(
        !wake.take_pending(),
        "an unhealthy retry must not loop without a new request"
    );

    let successful_at = now.to_rfc3339();
    let healthy = Snapshot {
        query_health: vec![
            health(&failed, QueryHealthStatus::Success, Some(&successful_at)),
            health(&pending, QueryHealthStatus::Success, Some(&successful_at)),
        ],
        pending_targets: Vec::new(),
        ..unhealthy
    };
    assert!(!snapshot_needs_followup(&healthy, now));
    let active = wake.begin(refresh_lock.lock().await);
    wake.request();
    active.finish(snapshot_needs_followup(&healthy, now));
    assert!(
        !wake.take_pending(),
        "a request covered by full success must not duplicate the poll"
    );
}

#[tokio::test]
async fn history_failure_during_refresh_preserves_the_requested_followup() {
    let selected = target("history-recovery-wake");
    let mut cache = HistoryCache::default();
    let session = cache.recent(&selected, 0, Some("2".into()), Utc::now());
    assert!(cache.begin_load(&selected, "2", session.entry.generation, 0, Utc::now()));

    let wake = RefreshWake::default();
    let refresh_lock = Mutex::new(());
    let active = wake.begin(refresh_lock.lock().await);
    cache.mark_failed(&selected, "2", session.entry.generation);
    wake.request();
    active.finish(cache.needs_followup());

    assert!(wake.take_pending());
    assert!(
        !wake.take_pending(),
        "history recovery queues one follow-up"
    );
}

#[test]
fn successful_health_becomes_stale_after_twice_the_reported_cadence() {
    let selected = target("stale");
    let successful_at = "2026-10-06T00:00:00Z";
    let snapshot = Snapshot {
        settings: Settings {
            targets: vec![selected.clone()],
            ..Settings::default()
        },
        poll_interval_seconds: 120,
        query_health: vec![health(
            &selected,
            QueryHealthStatus::Success,
            Some(successful_at),
        )],
        ..Snapshot::initial(Settings::default())
    };
    let boundary = chrono::DateTime::parse_from_rfc3339("2026-10-06T00:04:00Z")
        .unwrap()
        .with_timezone(&Utc);
    assert!(!snapshot_needs_followup(&snapshot, boundary));
    assert!(snapshot_needs_followup(
        &snapshot,
        boundary + chrono::Duration::milliseconds(1)
    ));
}

#[tokio::test]
async fn results_publish_before_all_targets_finish_and_stale_epochs_are_discarded() {
    let state = state();
    let recorder = Recorder::default();
    let a = target("a");
    let b = target("b");
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.pending_targets = vec![a.key(), b.key()];
        snapshot.query_health =
            checking_query_health(&[a.clone(), b.clone()], &[], "2026-10-06T00:00:00Z");
    }
    state
        .publish_page(
            &a,
            Ok(ActivityPage {
                activities: vec![activity(&a, "first", ActivityStatus::Success)],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            0,
            1,
            &recorder,
        )
        .await;
    let published = recorder.0.lock().unwrap().clone();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].activities[0].id, "first");
    assert!(published[0].pending_targets.contains(&b.key()));
    state.settings_revision.fetch_add(1, Ordering::SeqCst);
    state
        .publish_page(
            &b,
            Ok(ActivityPage {
                activities: vec![activity(&b, "stale", ActivityStatus::Success)],
                ..ActivityPage::default()
            }),
            PageKind::Recent,
            0,
            1,
            &recorder,
        )
        .await;
    assert_eq!(recorder.0.lock().unwrap().len(), 1);
    assert!(!state
        .snapshot()
        .await
        .activities
        .iter()
        .any(|a| a.id == "stale"));
    let stale_health = state
        .snapshot()
        .await
        .query_health
        .into_iter()
        .find(|health| health.target_id == b.id)
        .unwrap();
    assert_eq!(stale_health.status, QueryHealthStatus::Checking);
    assert!(stale_health.last_successful_at.is_none());
}

#[tokio::test]
async fn forged_or_expired_history_cursor_is_rejected_before_any_cli_call() {
    let state = state();
    let recorder = Recorder::default();
    let error = state
        .load_more(
            "github:local",
            "sample/a",
            "https://invalid.example",
            1,
            &recorder,
        )
        .await
        .unwrap_err();
    assert!(error.contains("history page has changed"));
    assert!(recorder.0.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn queued_recent_pages_start_before_slow_active_scans() {
    let _serial = SLOW_GITHUB_TEST_LOCK.lock().await;
    let server = FakeGithub::start(true);
    let state = AppState {
        tools: Toolchain::github_for_test(
            &server.directory,
            CliInvocation::for_test(server.script.clone()),
        ),
        ..state()
    };
    server
        .configure(&state, &["first", "second", "third"])
        .await;
    let recorder = Recorder::default();
    let done = state.refresh(&recorder).await;
    assert!(!done.refreshing);
    let pages = recorder.0.lock().unwrap();
    let all_recent = pages.iter().find(|s| s.history.len() == 3).unwrap();
    assert_eq!(
        all_recent.pending_targets.len(),
        3,
        "queued recent pages must publish before slow active scans finish"
    );
    let calls = std::fs::read_to_string(server.script.with_extension("calls")).unwrap();
    let third_recent = calls
        .find("repos/sample/third/actions/runs?per_page=20")
        .unwrap();
    let first_active_done = calls.find("active-finished").unwrap();
    assert!(
        third_recent < first_active_done,
        "queued recent lookup must start before slow active probes complete"
    );
    assert!(done.issues.is_empty());
}

#[cfg(unix)]
struct FakeGithub {
    directory: std::path::PathBuf,
    script: std::path::PathBuf,
}

#[cfg(unix)]
impl FakeGithub {
    fn start(slow_active: bool) -> Self {
        Self::start_with_identity_delay(slow_active, 0)
    }

    fn start_with_identity_delay(slow_active: bool, identity_delay_ms: u64) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("ratelline-stream-cli-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let script = directory.join("fake-cli");
        let source = r#"#!/bin/sh
printf '%s\n' "$*" >> "$0.calls"
case "$*" in
  *--version*) printf 'gh version 2.90.0\n';;
  *'api user '*|*'api /user '*) sleep IDENTITY_DELAY; printf '{"id":42,"login":"synthetic-user"}';;
  *'status='*) if [ "SLOW_ACTIVE" = true ]; then attempts=0; while [ ! -e "$0.third-recent-started" ] && [ "$attempts" -lt 500 ]; do sleep 0.02; attempts=$((attempts + 1)); done; fi; printf 'active-finished\n' >> "$0.calls"; printf '{"total_count":0,"workflow_runs":[]}';;
  *'repos/sample/failing/actions/runs?per_page=20'*) printf '{';;
  *'repos/sample/third/actions/runs?per_page=20'*) touch "$0.third-recent-started"; printf '{"total_count":0,"workflow_runs":[]}';;
  *'repos/sample/slow/actions/runs?per_page=20'*) if [ "SLOW_ACTIVE" = false ]; then sleep 0.3; fi; printf '{"total_count":0,"workflow_runs":[]}';;
  *) printf '{"total_count":0,"workflow_runs":[]}';;
esac
"#
        .replace("SLOW_ACTIVE", if slow_active { "true" } else { "false" })
        .replace(
            "IDENTITY_DELAY",
            &format!("{}.{:03}", identity_delay_ms / 1_000, identity_delay_ms % 1_000),
        );
        std::fs::write(&script, source).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { directory, script }
    }

    async fn configure(&self, state: &AppState, names: &[&str]) -> Vec<Target> {
        let targets: Vec<_> = names.iter().map(|name| target(name)).collect();
        state.snapshot.write().await.settings.targets = targets.clone();
        targets
    }

    async fn configure_bound(&self, state: &AppState, names: &[&str]) -> Vec<Target> {
        let targets = self.configure(state, names).await;
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.accounts = snapshot
            .settings
            .accounts
            .iter()
            .map(|account| {
                if account.id == LOCAL_GITHUB_ID {
                    Account {
                        profile: Some("42".into()),
                        cli_username: Some("synthetic-user".into()),
                        ..account.clone()
                    }
                } else {
                    account.clone()
                }
            })
            .collect();
        targets
    }
}

#[cfg(unix)]
#[tokio::test]
async fn inflight_recovery_request_waits_for_lock_then_runs_exactly_one_followup() {
    let _serial = SLOW_GITHUB_TEST_LOCK.lock().await;
    let server = FakeGithub::start(false);
    let state = std::sync::Arc::new(AppState {
        tools: Toolchain::github_for_test(
            &server.directory,
            CliInvocation::for_test(server.script.clone()),
        ),
        ..state()
    });
    let targets = server.configure(&state, &["failing", "slow"]).await;
    let failing = targets[0].clone();
    let slow = targets[1].clone();
    let publisher = std::sync::Arc::new(Recorder::default());

    let polling = tokio::spawn({
        let state = state.clone();
        let publisher = publisher.clone();
        async move {
            assert!(state.refresh_wake.wait(Duration::from_secs(5)).await);
            state.refresh(publisher.as_ref()).await
        }
    });
    let manual = tokio::spawn({
        let state = state.clone();
        let publisher = publisher.clone();
        async move { state.refresh(publisher.as_ref()).await }
    });

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = state.snapshot().await;
            let failed = snapshot.query_health.iter().any(|health| {
                health.target_id == failing.id && health.status == QueryHealthStatus::Error
            });
            if failed && snapshot.pending_targets.contains(&slow.key()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("one target must fail while the slow target is still pending");
    state.request_refresh();

    manual.await.expect("manual refresh task");
    let retry = polling.await.expect("polling follow-up task");
    assert_eq!(
        retry
            .query_health
            .iter()
            .find(|health| health.target_id == failing.id)
            .unwrap()
            .status,
        QueryHealthStatus::Partial
    );
    assert!(
        !state.refresh_wake.wait(Duration::from_millis(20)).await,
        "a failed retry must not enqueue itself"
    );

    let calls = std::fs::read_to_string(server.script.with_extension("calls")).unwrap();
    for target in [failing, slow] {
        let needle = format!("repos/{}/actions/runs?per_page=20", target.id);
        assert_eq!(
            calls.lines().filter(|line| line.contains(&needle)).count(),
            2,
            "each target must run once in the original cycle and once in the follow-up"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn refresh_resolves_account_identity_once_for_many_targets() {
    let _serial = SLOW_GITHUB_TEST_LOCK.lock().await;
    let server = FakeGithub::start_with_identity_delay(false, 80);
    let state = AppState {
        tools: Toolchain::github_for_test(
            &server.directory,
            CliInvocation::for_test(server.script.clone()),
        ),
        ..state()
    };
    let names = (0..8)
        .map(|index| format!("bulk-{index}"))
        .collect::<Vec<_>>();
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    server.configure_bound(&state, &names).await;

    let started = std::time::Instant::now();
    let done = state.refresh(&Recorder::default()).await;
    let elapsed = started.elapsed();
    let calls = std::fs::read_to_string(server.script.with_extension("calls")).unwrap();
    let identity_calls = calls
        .lines()
        .filter(|line| line.contains("api user ") || line.contains("api /user "))
        .count();

    eprintln!(
        "synthetic identity cache: targets=8 identity_calls={identity_calls} elapsed={}ms identity_delay=80ms",
        elapsed.as_millis()
    );
    assert_eq!(
        identity_calls, 1,
        "identity must be resolved once per cycle"
    );
    assert_eq!(done.history.len(), 8);
    assert!(done.issues.is_empty());

    state.refresh(&Recorder::default()).await;
    let calls = std::fs::read_to_string(server.script.with_extension("calls")).unwrap();
    let identity_calls = calls
        .lines()
        .filter(|line| line.contains("api user ") || line.contains("api /user "))
        .count();
    assert_eq!(
        identity_calls, 2,
        "a later refresh cycle must revalidate identity"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn in_flight_refresh_restarts_once_and_includes_a_new_target() {
    let _serial = SLOW_GITHUB_TEST_LOCK.lock().await;
    let server = FakeGithub::start_with_identity_delay(false, 300);
    let state = std::sync::Arc::new(AppState {
        tools: Toolchain::github_for_test(
            &server.directory,
            CliInvocation::for_test(server.script.clone()),
        ),
        store: SettingsStore::new(server.directory.clone()),
        ..state()
    });
    server.configure_bound(&state, &["first"]).await;
    let publisher = std::sync::Arc::new(Recorder::default());
    let refresh = tokio::spawn({
        let state = state.clone();
        let publisher = publisher.clone();
        async move { state.refresh(publisher.as_ref()).await }
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let calls =
                std::fs::read_to_string(server.script.with_extension("calls")).unwrap_or_default();
            if calls.contains("api user ") || calls.contains("api /user ") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first refresh cycle must start");

    let added = target("second");
    state
        .change_settings(
            SettingsChange::Target {
                target: added.clone(),
                selected: true,
            },
            publisher.as_ref(),
        )
        .await
        .expect("select target during refresh");
    state
        .change_settings(
            SettingsChange::Language {
                value: crate::preferences::Language::English,
            },
            publisher.as_ref(),
        )
        .await
        .expect("change presentation setting during refresh");

    let done = refresh.await.expect("refresh task");
    assert_eq!(
        done.settings.preferences.language,
        crate::preferences::Language::English
    );
    assert_eq!(done.history.len(), 2);
    assert!(done
        .history
        .iter()
        .any(|history| history.target_id == added.id));
    let calls = std::fs::read_to_string(server.script.with_extension("calls")).unwrap();
    assert!(calls.contains("repos/sample/second/actions/runs?per_page=20"));
}

#[cfg(unix)]
#[tokio::test]
async fn local_setting_save_does_not_wait_for_provider_network() {
    let _serial = SLOW_GITHUB_TEST_LOCK.lock().await;
    let server = FakeGithub::start_with_identity_delay(false, 1_000);
    let state = std::sync::Arc::new(AppState {
        tools: Toolchain::github_for_test(
            &server.directory,
            CliInvocation::for_test(server.script.clone()),
        ),
        store: SettingsStore::new(server.directory.clone()),
        ..state()
    });
    server.configure(&state, &["network-delay"]).await;
    let publisher = std::sync::Arc::new(Recorder::default());
    let refresh_started = std::time::Instant::now();
    let refresh = tokio::spawn({
        let state = state.clone();
        let publisher = publisher.clone();
        async move { state.refresh(publisher.as_ref()).await }
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let calls =
                std::fs::read_to_string(server.script.with_extension("calls")).unwrap_or_default();
            if calls.contains("api user ") || calls.contains("api /user ") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("delayed provider request must start");

    let save_started = std::time::Instant::now();
    let saved = tokio::time::timeout(
        Duration::from_millis(750),
        state.change_settings(
            SettingsChange::Language {
                value: crate::preferences::Language::English,
            },
            publisher.as_ref(),
        ),
    )
    .await
    .expect("local settings must not wait for provider network")
    .expect("save local setting");
    let save_elapsed = save_started.elapsed();
    assert_eq!(
        saved.preferences.language,
        crate::preferences::Language::English
    );
    assert_eq!(state.settings_revision.load(Ordering::SeqCst), 0);
    assert!(state.snapshot().await.refreshing);

    let done = refresh.await.expect("refresh task");
    eprintln!(
        "synthetic independent save: save={}ms refresh={}ms provider_delay=1000ms",
        save_elapsed.as_millis(),
        refresh_started.elapsed().as_millis()
    );
    assert!(!done.refreshing);
}

#[cfg(unix)]
impl Drop for FakeGithub {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
