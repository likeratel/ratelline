use super::*;
use crate::models::QueryHealthStatus;
use crate::preferences::{resolved_language, Language};

pub(super) fn update_tray<R: Runtime>(app: &AppHandle<R>, snapshot: &Snapshot) {
    let (title, tooltip) = presentation(snapshot, Utc::now());
    if let Some(tray) = app.tray_by_id(crate::TRAY_ID) {
        if let Err(error) = tray.set_title(title) {
            eprintln!("Could not update tray title: {error}");
        }
        if let Err(error) = tray.set_tooltip(Some(tooltip)) {
            eprintln!("Could not update tray tooltip: {error}");
        }
    }
    #[cfg(target_os = "macos")]
    crate::tray_animation::sync(
        app,
        active_tray_count(snapshot),
        snapshot.settings.preferences.animate_menu_bar_icon,
    );
}

pub(super) fn presentation(
    snapshot: &Snapshot,
    now: chrono::DateTime<Utc>,
) -> (Option<String>, String) {
    let active = active_tray_count(snapshot);
    let affected = snapshot
        .settings
        .targets
        .iter()
        .filter(|target| {
            snapshot.settings.panel.providers.contains(&target.provider)
                && !snapshot
                    .settings
                    .panel
                    .hidden_targets
                    .contains(&target.key())
                && needs_attention(snapshot, target, now)
        })
        .count();
    let title = match (active, affected) {
        (0, 0) => None,
        (0, _) => Some("!".into()),
        (_, 0) => Some(active.to_string()),
        _ => Some(format!("{active} !")),
    };
    let language = resolved_language(&snapshot.settings.preferences.language);
    let status = match language {
        Language::Korean => format!("진행 중 {active}개"),
        Language::Japanese => format!("実行中 {active}件"),
        Language::SimplifiedChinese => format!("{active} 个运行中"),
        _ => format!("{active} active runs"),
    };
    let attention = match language {
        Language::Korean => format!("{affected}개 프로젝트 조회 확인 필요"),
        Language::Japanese => format!("{affected}件のプロジェクトの取得状況を確認してください"),
        Language::SimplifiedChinese => format!("{affected} 个项目需要检查查询状态"),
        _ => format!("Check query status for {affected} projects"),
    };
    let tooltip = if affected > 0 {
        format!("ratelline — {status} · {attention}")
    } else if active > 0 {
        format!("ratelline — {status}")
    } else {
        "ratelline".into()
    };
    (title, tooltip)
}

fn needs_attention(snapshot: &Snapshot, target: &Target, now: chrono::DateTime<Utc>) -> bool {
    if snapshot.connections.iter().any(|connection| {
        connection.connection_id == target.connection_id
            && connection.provider == target.provider
            && connection.status != ConnectionStatus::Connected
    }) {
        return true;
    }
    snapshot.query_health.iter().any(|health| {
        health.connection_id == target.connection_id
            && health.target_id == target.id
            && health.provider == target.provider
            && (matches!(
                health.status,
                QueryHealthStatus::Error | QueryHealthStatus::Partial
            ) || health
                .last_successful_at
                .as_deref()
                .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
                .is_some_and(|time| {
                    now.signed_duration_since(time).num_seconds()
                        > snapshot.poll_interval_seconds.saturating_mul(2).max(60) as i64
                }))
    })
}

pub(super) fn active_tray_count(snapshot: &Snapshot) -> usize {
    let mut runs = HashMap::<(Provider, &str, &str), (&Activity, bool)>::new();
    if !snapshot.settings.panel.show_active {
        return 0;
    }
    let now = Utc::now();
    for activity in snapshot.activities.iter().filter(|activity| {
        !snapshot
            .settings
            .hidden_activities
            .iter()
            .any(|hidden| hidden.matches(activity))
            && snapshot
                .settings
                .panel
                .includes(activity, &snapshot.settings)
    }) {
        let key = (
            activity.provider,
            activity.target_id.as_str(),
            activity.id.as_str(),
        );
        let eligible = snapshot.settings.effective_display_scope(
            activity.provider,
            Some(&activity.connection_id),
            Some(&activity.target_id),
        ) == Scope::All
            || activity.ownership == Ownership::Mine;
        let eligible_seen = eligible || runs.get(&key).is_some_and(|(_, seen)| *seen);
        let latest = runs
            .get(&key)
            .map(|(current, _)| newer_observation(current, activity))
            .unwrap_or(activity);
        runs.insert(key, (latest, eligible_seen));
    }
    runs.values()
        .filter(|(activity, eligible)| {
            *eligible
                && is_active_status(activity.status)
                && crate::panel::visible_activity(activity, now)
        })
        .count()
}
