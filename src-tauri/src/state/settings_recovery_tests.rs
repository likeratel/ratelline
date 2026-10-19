use super::*;

#[tokio::test]
async fn unreadable_settings_remain_visible_and_block_changes_after_refresh() {
    let directory = std::env::temp_dir().join(format!(
        "ratelline-settings-recovery-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings.json");
    let original = b"{broken settings file";
    std::fs::write(&path, original).unwrap();
    let store = SettingsStore::new(directory.clone());
    let snapshot = load_initial_snapshot(&store).await;
    assert!(snapshot.settings_error.is_some());
    assert!(snapshot.issues.is_empty(), "not a GitHub connection error");
    let state = AppState {
        tools: Toolchain::unavailable_for_test(&directory),
        store,
        snapshot: RwLock::new(snapshot.clone()),
        refresh_lock: Mutex::new(()),
        refresh_wake: RefreshWake::default(),
        publish_lock: Mutex::new(()),
        settings_revision: AtomicU64::new(0),
        refresh_epoch: AtomicU64::new(0),
        history_lock: Mutex::new(()),
        history_cache: Mutex::new(HistoryCache::default()),
        notification_engine: Mutex::new(NotificationEngine::default()),
        login_statuses: Mutex::new(HashMap::new()),
    };
    let refreshed = state.refresh(&NoopPublisher).await;
    assert_eq!(refreshed.settings_error, snapshot.settings_error);
    let changes = Settings {
        history_hours: 168,
        ..snapshot.settings.clone()
    };
    assert!(state.save_settings(changes, &NoopPublisher).await.is_err());
    assert!(state
        .change_settings(SettingsChange::HistoryHours { value: 168 }, &NoopPublisher)
        .await
        .is_err());
    assert!(state
        .create_connection(Provider::Vercel, "Example".into(), None, &NoopPublisher)
        .await
        .is_err());
    assert!(state
        .remove_connection("bitbucket:example", &NoopPublisher)
        .await
        .unwrap_err()
        .contains("locked"));
    let login = LoginStatus {
        connection_id: LOCAL_VERCEL_ID.into(),
        provider: Provider::Vercel,
        state: crate::models::LoginState::Waiting,
        message: "Example".into(),
        url: None,
        code: None,
    };
    assert!(state
        .register_login_attempt(LOCAL_VERCEL_ID, Provider::Vercel, login)
        .await
        .is_err());
    assert!(state.login_statuses.lock().await.is_empty());
    assert_eq!(state.snapshot().await.settings, snapshot.settings);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn first_run_without_settings_stays_writable() {
    let directory = std::env::temp_dir().join(format!(
        "ratelline-settings-first-run-{}",
        uuid::Uuid::new_v4()
    ));
    let store = SettingsStore::new(directory.clone());
    let initial = load_initial_snapshot(&store).await;
    assert!(initial.settings_error.is_none());
    assert!(store.save(initial.settings).await.is_ok());
    std::fs::remove_dir_all(directory).unwrap();
}
