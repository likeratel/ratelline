use super::*;

mod provider_connections;
mod tray;

fn test_directory(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ratelline-{name}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

fn test_state(directory: &Path) -> AppState {
    AppState {
        tools: Toolchain::unavailable_for_test(directory),
        store: SettingsStore::new(directory.to_owned()),
        snapshot: RwLock::new(Snapshot::initial(Settings::default())),
        refresh_lock: Mutex::new(()),
        refresh_wake: RefreshWake::default(),
        publish_lock: Mutex::new(()),
        settings_revision: AtomicU64::new(0),
        refresh_epoch: AtomicU64::new(0),
        history_lock: Mutex::new(()),
        history_cache: Mutex::new(HistoryCache::default()),
        notification_engine: Mutex::new(NotificationEngine::default()),
        login_statuses: Mutex::new(HashMap::new()),
        github_pin_attempts: Mutex::new(HashMap::new()),
    }
}

fn target(connection_id: &str, project: &str) -> Target {
    Target {
        id: format!("org/{project}"),
        provider: Provider::Github,
        connection_id: connection_id.into(),
        name: project.into(),
        description: None,
        metadata: HashMap::new(),
    }
}

fn activity(connection_id: &str, project: &str) -> Activity {
    Activity {
        id: format!("id-{project}"),
        provider: Provider::Github,
        connection_id: connection_id.into(),
        target_id: format!("org/{project}"),
        workflow_name: None,
        kind: "push".into(),
        title: "Deploy".into(),
        project: project.into(),
        branch: None,
        url: "https://github.com/example/project/actions/runs/1".into(),
        status: ActivityStatus::Success,
        raw_status: "success".into(),
        actor: Some("person".into()),
        ownership: Ownership::Mine,
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn connection(connection_id: &str, account: Option<&str>, status: ConnectionStatus) -> Connection {
    Connection {
        kind: None,
        connection_id: connection_id.into(),
        provider: Provider::Github,
        status,
        account: account.map(str::to_owned),
        message: None,
        cli_version: None,
    }
}

#[test]
fn same_external_target_isolated_by_connection() {
    let targets = [target("github-one", "a")];
    let issues = [Issue {
        kind: crate::models::IssueKind::Other,
        provider: Provider::Github,
        connection_id: Some("github-one".into()),
        message: "failed".into(),
        target_id: Some("org/a".into()),
    }];
    let cached = [activity("github-one", "a"), activity("github-two", "a")];
    let merged = merge_with_cache(
        &connection("github-one", Some("one"), ConnectionStatus::Connected),
        &targets,
        Vec::new(),
        &issues,
        &cached,
        &mut Vec::new(),
    );
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].connection_id, "github-one");
}

#[test]
fn account_change_clears_only_that_accounts_cache() {
    let cached = [activity("github-one", "a"), activity("github-two", "a")];
    let filtered = partition_cache_by_account(
        &cached,
        &[
            connection("github-one", Some("old"), ConnectionStatus::Connected),
            connection("github-two", Some("stable"), ConnectionStatus::Connected),
        ],
        &[
            connection("github-one", Some("new"), ConnectionStatus::Connected),
            connection("github-two", Some("stable"), ConnectionStatus::Connected),
        ],
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].connection_id, "github-two");
}

#[test]
fn failed_account_does_not_slow_healthy_idle_account() {
    let mut snapshot = Snapshot::initial(Settings::default());
    snapshot.connections = vec![
        connection("github:local", Some("person"), ConnectionStatus::Connected),
        connection("github-work", None, ConnectionStatus::SignedOut),
    ];
    snapshot.issues.push(Issue {
        kind: crate::models::IssueKind::Other,
        provider: Provider::Github,
        connection_id: Some("github-work".into()),
        message: "Sign in".into(),
        target_id: None,
    });
    assert_eq!(poll_interval(&snapshot), IDLE_POLL_INTERVAL);
    assert_eq!(
        with_poll_interval(snapshot.clone()).poll_interval_seconds,
        120
    );
    snapshot.activities = vec![Activity {
        status: ActivityStatus::Running,
        ..activity("github:local", "a")
    }];
    assert_eq!(poll_interval(&snapshot), ACTIVE_POLL_INTERVAL);
    assert_eq!(
        with_poll_interval(snapshot.clone()).poll_interval_seconds,
        30
    );
    snapshot.connections[0].status = ConnectionStatus::Error;
    snapshot.activities.clear();
    assert_eq!(poll_interval(&snapshot), ERROR_POLL_INTERVAL);
    assert_eq!(with_poll_interval(snapshot).poll_interval_seconds, 240);
}

#[test]
fn tray_deduplication_key_ignores_connection() {
    let activities = [activity("github-one", "a"), activity("github-two", "a")];
    let unique = activities
        .iter()
        .map(|activity| (activity.provider, activity.id.as_str()))
        .collect::<HashSet<_>>();
    assert_eq!(unique.len(), 1);
}

#[test]
fn tray_uses_latest_status_and_combined_ownership_for_duplicate_runs() {
    let mut snapshot = Snapshot::initial(Settings {
        targets: vec![target("github-one", "a"), target("github-two", "a")],
        ..Settings::default()
    });
    let stale_mine = Activity {
        status: ActivityStatus::Running,
        updated_at: "2026-09-29T10:00:00Z".into(),
        ..activity("github-one", "a")
    };
    let current_other = Activity {
        status: ActivityStatus::Success,
        ownership: Ownership::Other,
        updated_at: "2026-09-29T10:01:00Z".into(),
        ..activity("github-two", "a")
    };
    snapshot.activities = vec![stale_mine, current_other];

    assert_eq!(active_tray_count(&snapshot), 0);

    snapshot.activities[1].status = ActivityStatus::Running;
    assert_eq!(active_tray_count(&snapshot), 1);

    snapshot.activities[1].updated_at = snapshot.activities[0].updated_at.clone();
    snapshot.activities[1].status = ActivityStatus::Unknown;
    assert_eq!(active_tray_count(&snapshot), 1);

    snapshot.activities[1].status = ActivityStatus::Success;
    assert_eq!(active_tray_count(&snapshot), 0);
}

#[tokio::test]
async fn settings_save_rejects_forged_account_definitions() {
    let directory = test_directory("forged-settings");
    let state = test_state(&directory);
    let mut forged = Settings::default();
    forged.accounts.push(Account {
        id: "github-forged".into(),
        provider: Provider::Github,
        label: "Forged".into(),
        mode: AccountMode::Isolated,
        profile: None,
        cli_username: None,
    });

    let result = state.save_settings(forged, &NoopPublisher).await;

    assert_eq!(
        result.expect_err("forged account must be rejected"),
        "Account definitions can only be changed with account commands."
    );
    assert_eq!(state.snapshot().await.settings, Settings::default());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn settings_save_atomically_prunes_removed_target_rules_and_pins() {
    let directory = test_directory("prune-target-preferences");
    let state = test_state(&directory);
    let stale_key = "github:github:local:org/removed".to_owned();
    let mut settings = Settings::default();
    settings.panel.pinned_targets = vec![stale_key.clone()];
    settings.preferences.notifications.rules = vec![crate::preferences::NotificationRule {
        target_key: stale_key,
        scope: crate::preferences::NotificationScope::Mine,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    }];

    let saved = state
        .save_settings(settings, &NoopPublisher)
        .await
        .expect("save normalized settings");

    assert!(saved.panel.pinned_targets.is_empty());
    assert!(saved.preferences.notifications.rules.is_empty());
    assert_eq!(state.store.load().await.unwrap(), saved);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn committed_disable_enable_aba_rebaselines_notification_engine() {
    let directory = test_directory("notification-aba");
    let state = test_state(&directory);
    let selected = target(LOCAL_GITHUB_ID, "aba");
    let rule = crate::preferences::NotificationRule {
        target_key: selected.key(),
        scope: crate::preferences::NotificationScope::All,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    };
    let mut enabled = Settings {
        targets: vec![selected.clone()],
        ..Settings::default()
    };
    enabled.preferences.notifications.enabled = true;
    enabled.preferences.notifications.rules = vec![rule.clone()];
    state.snapshot.write().await.settings = enabled.clone();
    let running = Activity {
        status: ActivityStatus::Running,
        ..activity(LOCAL_GITHUB_ID, "aba")
    };
    state.notification_engine.lock().await.observe(
        &selected,
        std::slice::from_ref(&running),
        &enabled.preferences,
        1,
        Utc::now(),
    );

    let mut disabled = enabled.clone();
    disabled.preferences.notifications.enabled = false;
    state
        .save_settings(disabled, &NoopPublisher)
        .await
        .expect("disable notifications");
    state
        .save_settings(enabled.clone(), &NoopPublisher)
        .await
        .expect("re-enable notifications");

    let terminal = Activity {
        status: ActivityStatus::Failed,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..running
    };
    assert!(state
        .notification_engine
        .lock()
        .await
        .observe(&selected, &[terminal], &enabled.preferences, 2, Utc::now(),)
        .is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn unknown_connection_is_rejected_before_cli_resolution() {
    let directory = test_directory("unknown-connection");
    let state = test_state(&directory);

    let result = state.account("github-unknown", Provider::Github).await;

    assert_eq!(
        result.expect_err("unknown account must be rejected"),
        "The selected account does not exist for this provider."
    );
    assert!(!directory.join("accounts/github-unknown").exists());
}

#[tokio::test]
async fn removed_account_rejects_late_login_publication() {
    let directory = test_directory("late-login");
    let state = test_state(&directory);
    let account = state
        .create_connection(Provider::Vercel, "Work".into(), None, &NoopPublisher)
        .await
        .expect("create account");
    state.login_statuses.lock().await.insert(
        account.id.clone(),
        LoginStatus {
            connection_id: account.id.clone(),
            provider: Provider::Vercel,
            state: crate::models::LoginState::Waiting,
            message: "Waiting".into(),
            url: None,
            code: None,
        },
    );
    state
        .remove_connection(&account.id, &NoopPublisher)
        .await
        .expect("remove account");

    let published = state
        .publish_login_status_if_account_exists(
            &account.id,
            Provider::Vercel,
            LoginStatus {
                connection_id: account.id.clone(),
                provider: Provider::Vercel,
                state: crate::models::LoginState::Complete,
                message: "Complete".into(),
                url: None,
                code: None,
            },
        )
        .await;

    assert!(!published);
    assert!(!state.login_statuses.lock().await.contains_key(&account.id));
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn remove_connection_prunes_its_notification_rule_and_pin() {
    let directory = test_directory("remove-account-preferences");
    let state = test_state(&directory);
    let account = state
        .create_connection(Provider::Vercel, "Work".into(), None, &NoopPublisher)
        .await
        .expect("create account");
    let selected = Target {
        id: "project-id".into(),
        provider: Provider::Vercel,
        connection_id: account.id.clone(),
        name: "Example project".into(),
        description: None,
        metadata: HashMap::new(),
    };
    let key = selected.key();
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![selected];
        snapshot.settings.panel.pinned_targets = vec![key.clone()];
        snapshot.settings.preferences.notifications.rules =
            vec![crate::preferences::NotificationRule {
                target_key: key,
                scope: crate::preferences::NotificationScope::Mine,
                statuses: vec![ActivityStatus::Failed],
                enabled: true,
            }];
    }

    let saved = state
        .remove_connection(&account.id, &NoopPublisher)
        .await
        .expect("remove account and dependent preferences");

    assert!(saved.targets.is_empty());
    assert!(saved.panel.pinned_targets.is_empty());
    assert!(saved.preferences.notifications.rules.is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

fn bitbucket_credential(identity: &str) -> BitbucketCredential {
    BitbucketCredential {
        email: "person@example.invalid".into(),
        token: format!("token-{identity}"),
        uuid: format!("{{{identity}}}"),
        account_id: format!("account-{identity}"),
        display_name: "Bitbucket Person".into(),
    }
}

#[tokio::test]
async fn bitbucket_credential_is_not_serialized_and_failed_renewal_preserves_it() {
    let directory = test_directory("bitbucket-renewal");
    let state = test_state(&directory);
    let original = bitbucket_credential("original");
    let account = state
        .connect_bitbucket(original.clone(), None, &NoopPublisher)
        .await
        .expect("connect bitbucket");
    let persisted = std::fs::read_to_string(directory.join("settings.json")).unwrap();
    assert!(!persisted.contains(&original.email));
    assert!(!persisted.contains(&original.token));

    let result = state
        .connect_bitbucket(
            bitbucket_credential("different"),
            Some(&account.id),
            &NoopPublisher,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        crate::credentials::get_bitbucket(&account.id)
            .await
            .unwrap(),
        original
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn bitbucket_account_can_be_removed_after_credential_was_already_lost() {
    let directory = test_directory("bitbucket-missing-credential-removal");
    let state = test_state(&directory);
    let account = state
        .connect_bitbucket(bitbucket_credential("remove"), None, &NoopPublisher)
        .await
        .expect("connect bitbucket");
    crate::credentials::delete_bitbucket(&account.id)
        .await
        .unwrap();

    let settings = state
        .remove_connection(&account.id, &NoopPublisher)
        .await
        .expect("remove account with missing credential");
    assert!(!settings.accounts.iter().any(|item| item.id == account.id));
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn bitbucket_renewal_recovers_a_missing_credential_for_the_same_identity() {
    let directory = test_directory("bitbucket-missing-credential-renewal");
    let state = test_state(&directory);
    let original = bitbucket_credential("renew");
    let account = state
        .connect_bitbucket(original.clone(), None, &NoopPublisher)
        .await
        .expect("connect bitbucket");
    crate::credentials::delete_bitbucket(&account.id)
        .await
        .unwrap();
    let renewed = BitbucketCredential {
        token: "renewed-token".into(),
        ..original
    };

    state
        .connect_bitbucket(renewed.clone(), Some(&account.id), &NoopPublisher)
        .await
        .expect("renew missing credential");
    assert_eq!(
        crate::credentials::get_bitbucket(&account.id)
            .await
            .unwrap(),
        renewed
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn failed_new_bitbucket_settings_save_cleans_up_credential() {
    let directory = test_directory("bitbucket-create-rollback");
    std::fs::write(&directory, b"blocks directory creation").unwrap();
    let state = test_state(&directory);
    let credential = bitbucket_credential("create-failure");

    let result = state
        .connect_bitbucket(credential.clone(), None, &NoopPublisher)
        .await;

    assert!(result.is_err());
    assert_eq!(state.snapshot().await.settings, Settings::default());
    assert!(!crate::credentials::test_contains_token(&credential.token));
    let _ = std::fs::remove_file(directory);
}

#[tokio::test]
async fn failed_bitbucket_removal_restores_credential_and_account() {
    let directory = test_directory("bitbucket-remove-rollback");
    let state = test_state(&directory);
    let credential = bitbucket_credential("remove-failure");
    let account = state
        .connect_bitbucket(credential.clone(), None, &NoopPublisher)
        .await
        .expect("connect bitbucket");
    std::fs::remove_file(directory.join("settings.json")).unwrap();
    std::fs::remove_dir(&directory).unwrap();
    std::fs::write(&directory, b"blocks directory recreation").unwrap();

    let result = state.remove_connection(&account.id, &NoopPublisher).await;

    assert!(result.is_err());
    assert!(state
        .snapshot()
        .await
        .settings
        .accounts
        .iter()
        .any(|item| item.id == account.id));
    assert_eq!(
        crate::credentials::get_bitbucket(&account.id)
            .await
            .unwrap(),
        credential
    );
    let _ = std::fs::remove_file(directory);
}

#[tokio::test]
async fn simultaneous_account_mutations_do_not_lose_updates() {
    let directory = test_directory("concurrent-mutations");
    let state = test_state(&directory);

    let (first, second) = tokio::join!(
        state.create_connection(Provider::Vercel, "One".into(), None, &NoopPublisher),
        state.create_connection(Provider::Vercel, "Two".into(), None, &NoopPublisher)
    );
    let first = first.expect("create first account");
    let second = second.expect("create second account");
    let snapshot = state.snapshot().await;

    assert!(snapshot
        .settings
        .accounts
        .iter()
        .any(|account| account.id == first.id));
    assert!(snapshot
        .settings
        .accounts
        .iter()
        .any(|account| account.id == second.id));
    assert_eq!(snapshot.settings.accounts.len(), 4);
    assert_eq!(
        state.store.load().await.expect("load persisted settings"),
        snapshot.settings
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn simultaneous_setting_changes_preserve_unrelated_fields() {
    let directory = test_directory("concurrent-setting-changes");
    let state = test_state(&directory);

    let (scope, history) = tokio::join!(
        state.change_settings(SettingsChange::Scope { value: Scope::All }, &NoopPublisher),
        state.change_settings(SettingsChange::HistoryHours { value: 168 }, &NoopPublisher)
    );

    scope.expect("save scope");
    history.expect("save history");
    let saved = state.snapshot().await.settings;
    assert_eq!(saved.scope, Scope::All);
    assert_eq!(saved.history_hours, 168);
    assert_eq!(state.store.load().await.unwrap(), saved);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn hidden_activity_persists_and_can_be_restored_without_losing_raw_data() {
    let directory = test_directory("hidden-activity-round-trip");
    let state = test_state(&directory);
    let run = activity(LOCAL_GITHUB_ID, "persisted");
    let key = crate::models::ActivityKey::from_activity(&run);
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![target(LOCAL_GITHUB_ID, "persisted")];
        snapshot.activities = vec![run.clone()];
    }

    let hidden = state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: key.clone(),
                hidden: true,
            },
            &NoopPublisher,
        )
        .await
        .expect("hide activity");
    assert_eq!(hidden.hidden_activities, vec![key.clone()]);
    assert_eq!(state.snapshot().await.activities, vec![run]);
    assert_eq!(
        state.store.load().await.unwrap().hidden_activities,
        vec![key.clone()]
    );

    let restored = state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: key,
                hidden: false,
            },
            &NoopPublisher,
        )
        .await
        .expect("restore activity");
    assert!(restored.hidden_activities.is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn hiding_requires_an_existing_activity_and_rejects_oversized_keys() {
    let directory = test_directory("hidden-activity-validation");
    let state = test_state(&directory);
    let missing = crate::models::ActivityKey {
        provider: Provider::Github,
        target_id: "org/missing".into(),
        activity_id: "missing".into(),
    };
    assert_eq!(
        state
            .change_settings(
                SettingsChange::ActivityVisibility {
                    activity: missing,
                    hidden: true,
                },
                &NoopPublisher,
            )
            .await
            .expect_err("missing activity"),
        "The selected activity does not exist."
    );
    let mut forged = Settings::default();
    forged.hidden_activities.push(crate::models::ActivityKey {
        provider: Provider::Github,
        target_id: "org/missing".into(),
        activity_id: "missing".into(),
    });
    assert_eq!(
        state
            .save_settings(forged, &NoopPublisher)
            .await
            .expect_err("forged hidden activity"),
        "The selected activity does not exist."
    );

    let run = activity(LOCAL_GITHUB_ID, "oversized");
    state.snapshot.write().await.activities = vec![run.clone()];
    let oversized = crate::models::ActivityKey {
        activity_id: "x".repeat(513),
        ..crate::models::ActivityKey::from_activity(&run)
    };
    assert!(state
        .change_settings(
            SettingsChange::ActivityVisibility {
                activity: oversized,
                hidden: false,
            },
            &NoopPublisher,
        )
        .await
        .is_err());
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn hidden_activity_excludes_newer_status_and_duplicate_accounts_from_tray_count() {
    let first = Activity {
        status: ActivityStatus::Running,
        ..activity("github-one", "shared")
    };
    let second = Activity {
        connection_id: "github-two".into(),
        status: ActivityStatus::Running,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..first.clone()
    };
    let mut snapshot = Snapshot::initial(Settings {
        scope: Scope::All,
        hidden_activities: vec![crate::models::ActivityKey::from_activity(&first)],
        ..Settings::default()
    });
    snapshot.activities = vec![first, second];

    assert_eq!(active_tray_count(&snapshot), 0);
}

#[tokio::test]
async fn bulk_target_change_preserves_other_targets_and_prunes_references() {
    let directory = test_directory("bulk-target-change");
    let state = test_state(&directory);
    let first = target(LOCAL_GITHUB_ID, "first");
    let second = target(LOCAL_GITHUB_ID, "second");
    let third = target(LOCAL_GITHUB_ID, "third");
    let first_key = first.key();
    let second_key = second.key();
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![first.clone(), second.clone(), third.clone()];
        snapshot.settings.panel.hidden_targets = vec![first_key.clone(), second_key.clone()];
        snapshot.settings.panel.pinned_targets = vec![first_key.clone(), second_key.clone()];
        snapshot
            .settings
            .display_scopes
            .targets
            .insert(first_key, Scope::All);
    }

    let saved = state
        .change_settings(
            SettingsChange::Targets {
                targets: vec![first, second],
                selected: false,
            },
            &NoopPublisher,
        )
        .await
        .expect("bulk deselect targets");

    assert_eq!(saved.targets, vec![third]);
    assert!(saved.panel.hidden_targets.is_empty());
    assert!(saved.panel.pinned_targets.is_empty());
    assert!(saved.display_scopes.targets.is_empty());
    assert_eq!(state.store.load().await.unwrap(), saved);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn deselect_preserves_history_and_pending_state_for_unchanged_targets() {
    let directory = test_directory("deselect-preserves-other-history");
    let state = test_state(&directory);
    let removed = target(LOCAL_GITHUB_ID, "removed");
    let retained = Target {
        id: "team:project".into(),
        provider: Provider::Vercel,
        connection_id: LOCAL_VERCEL_ID.into(),
        name: "retained".into(),
        description: None,
        metadata: HashMap::from([
            ("projectId".into(), "project".into()),
            ("scopeSlug".into(), "team".into()),
        ]),
    };
    let generation = 7;
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![removed.clone(), retained.clone()];
        snapshot.history = vec![
            crate::models::TargetHistory {
                provider: removed.provider,
                connection_id: removed.connection_id.clone(),
                target_id: removed.id.clone(),
                next_cursor: Some("2".into()),
                checked_at: Utc::now().to_rfc3339(),
                generation,
            },
            crate::models::TargetHistory {
                provider: retained.provider,
                connection_id: retained.connection_id.clone(),
                target_id: retained.id.clone(),
                next_cursor: Some("1700000000000".into()),
                checked_at: Utc::now().to_rfc3339(),
                generation,
            },
        ];
        snapshot.pending_targets = vec![removed.key(), retained.key()];
        snapshot.query_health = vec![
            QueryHealth {
                provider: removed.provider,
                connection_id: removed.connection_id.clone(),
                target_id: removed.id.clone(),
                status: crate::models::QueryHealthStatus::Success,
                last_attempt_at: Utc::now().to_rfc3339(),
                last_successful_at: Some(Utc::now().to_rfc3339()),
            },
            QueryHealth {
                provider: retained.provider,
                connection_id: retained.connection_id.clone(),
                target_id: retained.id.clone(),
                status: crate::models::QueryHealthStatus::Success,
                last_attempt_at: Utc::now().to_rfc3339(),
                last_successful_at: Some(Utc::now().to_rfc3339()),
            },
        ];
    }
    state.refresh_epoch.store(generation, Ordering::SeqCst);

    state
        .change_settings(
            SettingsChange::Target {
                target: removed,
                selected: false,
            },
            &NoopPublisher,
        )
        .await
        .expect("deselect target");

    let snapshot = state.snapshot().await;
    assert_eq!(state.settings_revision.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot.settings.targets, vec![retained.clone()]);
    assert_eq!(snapshot.pending_targets, vec![retained.key()]);
    assert_eq!(snapshot.history.len(), 1);
    assert_eq!(snapshot.history[0].provider, Provider::Vercel);
    assert_eq!(snapshot.history[0].target_id, retained.id);
    assert_eq!(
        snapshot.history[0].next_cursor.as_deref(),
        Some("1700000000000")
    );
    assert_eq!(snapshot.history[0].generation, generation);
    assert_eq!(snapshot.query_health.len(), 1);
    assert_eq!(snapshot.query_health[0].target_id, retained.id);

    let changed_target = Target {
        metadata: retained
            .metadata
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    if key == "scopeSlug" {
                        "other-team".into()
                    } else {
                        value.clone()
                    },
                )
            })
            .collect(),
        ..retained
    };
    let changed = Settings {
        targets: vec![changed_target],
        ..snapshot.settings
    };
    state
        .save_settings(changed, &NoopPublisher)
        .await
        .expect("save changed target metadata");
    let changed_snapshot = state.snapshot().await;
    assert!(changed_snapshot.history.is_empty());
    assert!(changed_snapshot.pending_targets.is_empty());
    assert!(changed_snapshot.query_health.is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn refresh_wake_is_prompt_when_idle_and_coalesces_duplicate_or_inflight_requests() {
    let wake = RefreshWake::default();
    let refresh_lock = Mutex::new(());
    wake.request();
    wake.request();
    wake.request();
    assert!(wake.wait(Duration::from_secs(1)).await);
    assert!(!wake.wait(Duration::from_millis(10)).await);

    let active = wake.begin(refresh_lock.lock().await);
    wake.request();
    wake.request();
    active.finish(false);
    assert!(!wake.wait(Duration::from_millis(10)).await);

    let active = wake.begin(refresh_lock.lock().await);
    wake.request();
    wake.request();
    active.finish(true);
    assert!(wake.wait(Duration::from_secs(1)).await);
    let retry = wake.begin(refresh_lock.lock().await);
    retry.finish(true);
    assert!(!wake.wait(Duration::from_millis(10)).await);

    wake.request();
    assert!(wake.wait(Duration::from_secs(1)).await);
}

#[tokio::test]
async fn bulk_target_change_over_max_targets_fails_without_partial_update() {
    let directory = test_directory("bulk-target-max");
    let state = test_state(&directory);
    let existing = (0..MAX_TARGETS)
        .map(|index| target(LOCAL_GITHUB_ID, &format!("existing-{index}")))
        .collect::<Vec<_>>();
    state.snapshot.write().await.settings.targets = existing.clone();
    let additional = target(LOCAL_GITHUB_ID, "additional");

    let error = state
        .change_settings(
            SettingsChange::Targets {
                targets: vec![additional],
                selected: true,
            },
            &NoopPublisher,
        )
        .await
        .expect_err("selection beyond MAX_TARGETS must fail");

    assert_eq!(
        error,
        format!("At most {MAX_TARGETS} targets can be selected.")
    );
    assert_eq!(state.snapshot().await.settings.targets, existing);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn setting_changes_reject_unknown_target_and_account_keys() {
    let directory = test_directory("invalid-setting-keys");
    let state = test_state(&directory);

    let target_error = state
        .change_settings(
            SettingsChange::PanelTarget {
                target_key: "github:github:local:org/missing".into(),
                visible: false,
            },
            &NoopPublisher,
        )
        .await
        .expect_err("unknown target must fail");
    let account_error = state
        .change_settings(
            SettingsChange::AccountAlias {
                connection_id: "github:missing".into(),
                alias: Some("Missing".into()),
            },
            &NoopPublisher,
        )
        .await
        .expect_err("unknown account must fail");

    assert_eq!(target_error, "The selected target does not exist.");
    assert_eq!(account_error, "The selected account does not exist.");
    assert_eq!(state.snapshot().await.settings, Settings::default());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn account_aliases_are_trimmed_validated_removed_and_pruned() {
    let directory = test_directory("account-aliases");
    let state = test_state(&directory);
    let account = state
        .create_connection(Provider::Vercel, "Work".into(), None, &NoopPublisher)
        .await
        .expect("create account");

    let aliased = state
        .change_settings(
            SettingsChange::AccountAlias {
                connection_id: account.id.clone(),
                alias: Some("  Production  ".into()),
            },
            &NoopPublisher,
        )
        .await
        .expect("set alias");
    assert_eq!(
        aliased.preferences.account_aliases.get(&account.id),
        Some(&"Production".to_owned())
    );

    for alias in [Some("   ".to_owned()), Some("x".repeat(81))] {
        assert!(state
            .change_settings(
                SettingsChange::AccountAlias {
                    connection_id: account.id.clone(),
                    alias,
                },
                &NoopPublisher,
            )
            .await
            .is_err());
    }
    let removed = state
        .change_settings(
            SettingsChange::AccountAlias {
                connection_id: account.id.clone(),
                alias: None,
            },
            &NoopPublisher,
        )
        .await
        .expect("remove alias");
    assert!(!removed
        .preferences
        .account_aliases
        .contains_key(&account.id));

    state
        .change_settings(
            SettingsChange::AccountAlias {
                connection_id: account.id.clone(),
                alias: Some("Work account".into()),
            },
            &NoopPublisher,
        )
        .await
        .expect("restore alias");
    let pruned = state
        .remove_connection(&account.id, &NoopPublisher)
        .await
        .expect("remove aliased account");
    assert!(!pruned.preferences.account_aliases.contains_key(&account.id));
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn new_notification_rules_use_all_outcome_defaults() {
    let directory = test_directory("notification-rule-defaults");
    let state = test_state(&directory);
    let selected = target(LOCAL_GITHUB_ID, "defaults");
    state.snapshot.write().await.settings.targets = vec![selected.clone()];

    let saved = state
        .change_settings(
            SettingsChange::NotificationRules {
                rules: vec![crate::models::NotificationRuleChange {
                    target_key: selected.key(),
                    enabled: None,
                    scope: None,
                    statuses: None,
                }],
            },
            &NoopPublisher,
        )
        .await
        .expect("create default rule");

    let rule = &saved.preferences.notifications.rules[0];
    assert!(!rule.enabled);
    assert_eq!(rule.scope, crate::preferences::NotificationScope::Mine);
    assert_eq!(
        rule.statuses,
        vec![
            ActivityStatus::Success,
            ActivityStatus::Failed,
            ActivityStatus::Canceled
        ]
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn newly_selected_targets_get_enabled_rules_without_enabling_master_switch() {
    let directory = test_directory("selected-target-notification-defaults");
    let state = test_state(&directory);
    let first = target(LOCAL_GITHUB_ID, "first-default");
    let second = target(LOCAL_GITHUB_ID, "second-default");

    state
        .change_settings(
            SettingsChange::Target {
                target: first.clone(),
                selected: true,
            },
            &NoopPublisher,
        )
        .await
        .expect("select one target");
    let saved = state
        .change_settings(
            SettingsChange::Targets {
                targets: vec![second.clone()],
                selected: true,
            },
            &NoopPublisher,
        )
        .await
        .expect("select targets in bulk");

    assert!(!saved.preferences.notifications.enabled);
    for selected in [first, second] {
        let rule = saved
            .preferences
            .notifications
            .rules
            .iter()
            .find(|rule| rule.target_key == selected.key())
            .expect("new target rule");
        assert!(rule.enabled);
        assert_eq!(rule.scope, crate::preferences::NotificationScope::Mine);
        assert_eq!(
            rule.statuses,
            vec![
                ActivityStatus::Success,
                ActivityStatus::Failed,
                ActivityStatus::Canceled
            ]
        );
    }
    assert_eq!(state.store.load().await.unwrap(), saved);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn newly_selected_target_baselines_existing_terminal_activity_without_backlog() {
    let directory = test_directory("selected-target-notification-baseline");
    let state = test_state(&directory);
    state
        .change_settings(
            SettingsChange::NotificationEnabled { enabled: true },
            &NoopPublisher,
        )
        .await
        .expect("enable master notifications");
    let selected = target(LOCAL_GITHUB_ID, "baseline");
    let saved = state
        .change_settings(
            SettingsChange::Target {
                target: selected.clone(),
                selected: true,
            },
            &NoopPublisher,
        )
        .await
        .expect("select target");
    assert!(saved.preferences.notifications.enabled);
    let existing_terminal = Activity {
        status: ActivityStatus::Failed,
        ..activity(LOCAL_GITHUB_ID, "baseline")
    };

    let deliveries = state.notification_engine.lock().await.observe(
        &selected,
        &[existing_terminal],
        &saved.preferences,
        1,
        Utc::now(),
    );

    assert!(deliveries.is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn selecting_new_targets_preserves_other_target_transitions_and_pending_retries() {
    let directory = test_directory("selected-target-preserves-notification-state");
    let state = test_state(&directory);
    let active_target = target(LOCAL_GITHUB_ID, "active-before-selection");
    let active_rule = crate::preferences::NotificationRule {
        target_key: active_target.key(),
        scope: crate::preferences::NotificationScope::Mine,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    };
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![active_target.clone()];
        snapshot.settings.preferences.notifications.enabled = true;
        snapshot.settings.preferences.notifications.rules = vec![active_rule];
    }
    let initial = state.snapshot().await.settings;
    let running = Activity {
        status: ActivityStatus::Running,
        ..activity(LOCAL_GITHUB_ID, "active-before-selection")
    };
    assert!(state
        .notification_engine
        .lock()
        .await
        .observe(
            &active_target,
            std::slice::from_ref(&running),
            &initial.preferences,
            1,
            Utc::now(),
        )
        .is_empty());

    let second = target(LOCAL_GITHUB_ID, "new-selection");
    let after_second = state
        .change_settings(
            SettingsChange::Target {
                target: second,
                selected: true,
            },
            &NoopPublisher,
        )
        .await
        .expect("select unrelated target");
    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: (Utc::now() + chrono::Duration::seconds(1)).to_rfc3339(),
        ..running
    };
    let pending = state.notification_engine.lock().await.observe(
        &active_target,
        std::slice::from_ref(&failed),
        &after_second.preferences,
        2,
        Utc::now(),
    );
    assert_eq!(pending.len(), 1, "the active transition must survive");
    state
        .notification_engine
        .lock()
        .await
        .mark_failed(&pending[0], 2);

    let third = target(LOCAL_GITHUB_ID, "selection-during-retry");
    let after_third = state
        .change_settings(
            SettingsChange::Target {
                target: third,
                selected: true,
            },
            &NoopPublisher,
        )
        .await
        .expect("select target while delivery is pending");
    let retried = state.notification_engine.lock().await.observe(
        &active_target,
        &[failed],
        &after_third.preferences,
        3,
        Utc::now(),
    );
    assert_eq!(retried.len(), 1, "the failed delivery must remain pending");
    assert_eq!(retried[0].notification, pending[0].notification);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn whole_settings_save_defaults_only_new_targets_and_preserves_disabled_rules() {
    let directory = test_directory("legacy-save-target-defaults");
    let state = test_state(&directory);
    let existing = target(LOCAL_GITHUB_ID, "existing-disabled");
    let added = target(LOCAL_GITHUB_ID, "added-by-save");
    let disabled = crate::preferences::NotificationRule {
        target_key: existing.key(),
        scope: crate::preferences::NotificationScope::All,
        statuses: vec![ActivityStatus::Failed],
        enabled: false,
    };
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![existing];
        snapshot.settings.preferences.notifications.rules = vec![disabled.clone()];
    }
    let mut submitted = state.snapshot().await.settings;
    submitted.targets.push(added.clone());

    let saved = state
        .save_settings(submitted, &NoopPublisher)
        .await
        .expect("save legacy settings payload");

    assert_eq!(saved.preferences.notifications.rules[0], disabled);
    let added_rule = saved
        .preferences
        .notifications
        .rules
        .iter()
        .find(|rule| rule.target_key == added.key())
        .expect("added target rule");
    assert!(added_rule.enabled);
    assert_eq!(saved.preferences.notifications.rules.len(), 2);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn display_scope_changes_persist_and_unknown_target_is_rejected() {
    let directory = test_directory("display-scope-changes");
    let state = test_state(&directory);
    let selected = target(LOCAL_GITHUB_ID, "scope-change");
    state.snapshot.write().await.settings.targets = vec![selected.clone()];

    state
        .change_settings(
            SettingsChange::DisplayProviderScope {
                provider: Provider::Github,
                scope: Some(Scope::All),
            },
            &NoopPublisher,
        )
        .await
        .expect("set provider scope");
    let saved = state
        .change_settings(
            SettingsChange::DisplayTargetScope {
                target_key: selected.key(),
                scope: Some(Scope::Mine),
            },
            &NoopPublisher,
        )
        .await
        .expect("set target scope");
    assert_eq!(
        saved.display_scopes.providers.get(&Provider::Github),
        Some(&Scope::All)
    );
    assert_eq!(
        saved.display_scopes.targets.get(&selected.key()),
        Some(&Scope::Mine)
    );
    assert!(state
        .change_settings(
            SettingsChange::DisplayTargetScope {
                target_key: "github:github:local:org/missing".into(),
                scope: Some(Scope::All),
            },
            &NoopPublisher,
        )
        .await
        .is_err());
    let cleared = state
        .change_settings(
            SettingsChange::DisplayTargetScope {
                target_key: selected.key(),
                scope: None,
            },
            &NoopPublisher,
        )
        .await
        .expect("clear target scope");
    assert!(!cleared.display_scopes.targets.contains_key(&selected.key()));
    assert_eq!(state.store.load().await.unwrap(), cleared);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn notification_rule_patch_preserves_unmentioned_targets() {
    let directory = test_directory("notification-rule-patch");
    let state = test_state(&directory);
    let first = target(LOCAL_GITHUB_ID, "first");
    let second = target(LOCAL_GITHUB_ID, "second");
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![first.clone(), second.clone()];
        snapshot.settings.preferences.notifications.rules = vec![
            crate::preferences::NotificationRule {
                target_key: first.key(),
                scope: crate::preferences::NotificationScope::Mine,
                statuses: vec![ActivityStatus::Failed],
                enabled: false,
            },
            crate::preferences::NotificationRule {
                target_key: second.key(),
                scope: crate::preferences::NotificationScope::All,
                statuses: vec![ActivityStatus::Canceled],
                enabled: true,
            },
        ];
    }

    let saved = state
        .change_settings(
            SettingsChange::NotificationRules {
                rules: vec![crate::models::NotificationRuleChange {
                    target_key: first.key(),
                    enabled: Some(true),
                    scope: None,
                    statuses: None,
                }],
            },
            &NoopPublisher,
        )
        .await
        .expect("patch one notification rule");

    assert_eq!(saved.preferences.notifications.rules.len(), 2);
    assert!(saved.preferences.notifications.rules[0].enabled);
    assert_eq!(
        saved.preferences.notifications.rules[1],
        crate::preferences::NotificationRule {
            target_key: second.key(),
            scope: crate::preferences::NotificationScope::All,
            statuses: vec![ActivityStatus::Canceled],
            enabled: true,
        }
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn concurrent_notification_rule_patches_preserve_both_targets() {
    let directory = test_directory("concurrent-notification-rule-patches");
    let state = test_state(&directory);
    let first = target(LOCAL_GITHUB_ID, "first");
    let second = target(LOCAL_GITHUB_ID, "second");
    {
        let mut snapshot = state.snapshot.write().await;
        snapshot.settings.targets = vec![first.clone(), second.clone()];
        snapshot.settings.preferences.notifications.rules = vec![
            crate::preferences::NotificationRule {
                target_key: first.key(),
                scope: crate::preferences::NotificationScope::Mine,
                statuses: vec![ActivityStatus::Failed],
                enabled: false,
            },
            crate::preferences::NotificationRule {
                target_key: second.key(),
                scope: crate::preferences::NotificationScope::Mine,
                statuses: vec![ActivityStatus::Failed],
                enabled: false,
            },
        ];
    }

    let (first_result, second_result) = tokio::join!(
        state.change_settings(
            SettingsChange::NotificationRules {
                rules: vec![crate::models::NotificationRuleChange {
                    target_key: first.key(),
                    enabled: Some(true),
                    scope: None,
                    statuses: None,
                }],
            },
            &NoopPublisher,
        ),
        state.change_settings(
            SettingsChange::NotificationRules {
                rules: vec![crate::models::NotificationRuleChange {
                    target_key: second.key(),
                    enabled: None,
                    scope: Some(Scope::All),
                    statuses: Some(vec![ActivityStatus::Success]),
                }],
            },
            &NoopPublisher,
        )
    );
    first_result.expect("patch first rule");
    second_result.expect("patch second rule");

    let saved = state.snapshot().await.settings;
    let first_rule = saved
        .preferences
        .notifications
        .rules
        .iter()
        .find(|rule| rule.target_key == first.key())
        .unwrap();
    let second_rule = saved
        .preferences
        .notifications
        .rules
        .iter()
        .find(|rule| rule.target_key == second.key())
        .unwrap();
    assert!(first_rule.enabled);
    assert_eq!(
        second_rule.scope,
        crate::preferences::NotificationScope::All
    );
    assert_eq!(second_rule.statuses, vec![ActivityStatus::Success]);
    assert_eq!(state.store.load().await.unwrap(), saved);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn tray_and_polling_keep_long_running_but_exclude_blocked_runs() {
    let mut snapshot = Snapshot::initial(Settings {
        targets: vec![target("github:local", "a")],
        ..Settings::default()
    });
    snapshot.connections = vec![connection(
        "github:local",
        Some("person"),
        ConnectionStatus::Connected,
    )];
    snapshot.activities = vec![Activity {
        status: ActivityStatus::Running,
        ..activity("github:local", "a")
    }];
    assert_eq!(active_tray_count(&snapshot), 1);
    snapshot.settings.panel.providers = vec![Provider::Vercel];
    assert_eq!(active_tray_count(&snapshot), 0);
    snapshot.settings.panel.providers = vec![Provider::Github];
    snapshot.settings.panel.hidden_targets = vec!["github:github:local:org/a".into()];
    assert_eq!(active_tray_count(&snapshot), 0);
    snapshot.settings.panel.hidden_targets.clear();
    snapshot.settings.panel.show_active = false;
    assert_eq!(active_tray_count(&snapshot), 0);
    snapshot.settings.panel.show_active = true;
    snapshot.activities[0].started_at = (Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
    assert_eq!(active_tray_count(&snapshot), 1);
    assert_eq!(poll_interval(&snapshot), ACTIVE_POLL_INTERVAL);
    snapshot.activities[0].started_at = Utc::now().to_rfc3339();
    snapshot.activities[0].provider = Provider::Vercel;
    snapshot.activities[0].status = ActivityStatus::Waiting;
    snapshot.activities[0].raw_status = "BLOCKED".into();
    snapshot.settings.targets[0].provider = Provider::Vercel;
    snapshot.settings.panel.providers = vec![Provider::Vercel];
    assert_eq!(active_tray_count(&snapshot), 0);
    assert_eq!(poll_interval(&snapshot), IDLE_POLL_INTERVAL);
}

#[test]
fn tray_uses_target_then_provider_then_global_display_scope() {
    let first = target(LOCAL_GITHUB_ID, "scope-first");
    let second = target(LOCAL_GITHUB_ID, "scope-second");
    let mut settings = Settings {
        scope: Scope::Mine,
        targets: vec![first.clone(), second.clone()],
        ..Settings::default()
    };
    settings
        .display_scopes
        .providers
        .insert(Provider::Github, Scope::All);
    settings
        .display_scopes
        .targets
        .insert(second.key(), Scope::Mine);
    let mut first_activity = activity(LOCAL_GITHUB_ID, "scope-first");
    first_activity.status = ActivityStatus::Running;
    first_activity.ownership = Ownership::Other;
    let mut second_activity = activity(LOCAL_GITHUB_ID, "scope-second");
    second_activity.status = ActivityStatus::Running;
    second_activity.ownership = Ownership::Other;
    let mut snapshot = Snapshot::initial(settings);
    snapshot.activities = vec![first_activity, second_activity];

    assert_eq!(active_tray_count(&snapshot), 1);

    snapshot.settings.display_scopes.providers.clear();
    snapshot.settings.display_scopes.targets.clear();
    assert_eq!(active_tray_count(&snapshot), 0);
}

#[test]
fn tray_does_not_share_target_scope_between_equal_provider_activity_ids() {
    let first = target(LOCAL_GITHUB_ID, "isolated-first");
    let second = target(LOCAL_GITHUB_ID, "isolated-second");
    let mut settings = Settings {
        targets: vec![first.clone(), second.clone()],
        ..Settings::default()
    };
    settings
        .display_scopes
        .targets
        .insert(first.key(), Scope::All);
    let mut first_activity = activity(LOCAL_GITHUB_ID, "isolated-first");
    first_activity.id = "provider-local-id".into();
    first_activity.status = ActivityStatus::Running;
    first_activity.ownership = Ownership::Other;
    let mut second_activity = activity(LOCAL_GITHUB_ID, "isolated-second");
    second_activity.id = "provider-local-id".into();
    second_activity.status = ActivityStatus::Running;
    second_activity.ownership = Ownership::Other;
    let mut snapshot = Snapshot::initial(settings);
    snapshot.activities = vec![first_activity, second_activity];

    assert_eq!(active_tray_count(&snapshot), 1);
}

#[test]
fn cached_blocked_wins_a_timestamp_tie_over_running() {
    let running = Activity {
        provider: Provider::Vercel,
        status: ActivityStatus::Running,
        ..activity("vercel:local", "a")
    };
    let blocked = Activity {
        status: ActivityStatus::Waiting,
        raw_status: "BLOCKED".into(),
        ..running.clone()
    };
    let selected = Target {
        provider: Provider::Vercel,
        ..target("vercel:local", "a")
    };
    let snapshot = Snapshot {
        activities: vec![running.clone(), blocked.clone()],
        ..Snapshot::initial(Settings {
            targets: vec![selected],
            ..Settings::default()
        })
    };
    assert_eq!(active_tray_count(&snapshot), 0);
    assert_eq!(
        active_tray_count(&Snapshot {
            activities: vec![blocked, running],
            ..snapshot
        }),
        0
    );
}
