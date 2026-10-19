use super::*;
use crate::models::QueryHealthStatus;
use crate::preferences::Language;
use crate::state::tray::presentation;

#[test]
fn tray_marks_failed_queries_without_hiding_last_observed_active_count() {
    let mut snapshot = Snapshot::initial(Settings {
        targets: vec![target("github:local", "a")],
        ..Settings::default()
    });
    snapshot.settings.preferences.language = Language::Korean;
    snapshot.activities = vec![Activity {
        status: ActivityStatus::Running,
        ..activity("github:local", "a")
    }];
    snapshot.connections = vec![connection(
        "github:local",
        None,
        ConnectionStatus::SignedOut,
    )];
    let (title, tooltip) = presentation(&snapshot, Utc::now());
    assert_eq!(title.as_deref(), Some("1 !"));
    assert!(tooltip.contains("진행 중 1개"));
    assert!(tooltip.contains("1개 프로젝트 조회 확인 필요"));
    snapshot.settings.panel.hidden_targets = vec![snapshot.settings.targets[0].key()];
    assert_eq!(
        presentation(&snapshot, Utc::now()),
        (None, "ratelline".into())
    );
}

#[test]
fn tray_health_warning_is_scoped_and_recovers_after_success() {
    let now = Utc::now();
    let mut snapshot = Snapshot::initial(Settings {
        targets: vec![target("github:local", "a")],
        ..Settings::default()
    });
    snapshot.settings.preferences.language = Language::English;
    snapshot.connections = vec![connection(
        "github:local",
        Some("person"),
        ConnectionStatus::Connected,
    )];
    snapshot.query_health = vec![QueryHealth {
        provider: Provider::Github,
        connection_id: "github:local".into(),
        target_id: "org/a".into(),
        status: QueryHealthStatus::Partial,
        last_attempt_at: now.to_rfc3339(),
        last_successful_at: Some(now.to_rfc3339()),
    }];
    assert_eq!(presentation(&snapshot, now).0.as_deref(), Some("!"));
    snapshot.query_health[0].status = QueryHealthStatus::Success;
    assert_eq!(presentation(&snapshot, now), (None, "ratelline".into()));
    assert_eq!(
        presentation(&snapshot, now + chrono::Duration::minutes(10))
            .0
            .as_deref(),
        Some("!")
    );
    snapshot.settings.panel.providers.clear();
    assert_eq!(
        presentation(&snapshot, now + chrono::Duration::minutes(10)).0,
        None
    );
}
