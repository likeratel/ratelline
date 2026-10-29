use super::*;
use crate::{
    models::ActivityStatus,
    preferences::{NotificationRule, Preferences},
};

fn target() -> Target {
    Target {
        id: "org/app".into(),
        provider: Provider::Github,
        connection_id: "github:local".into(),
        name: "app".into(),
        description: None,
        metadata: HashMap::new(),
    }
}

fn activity(id: &str, status: ActivityStatus, ownership: Ownership) -> Activity {
    Activity {
        id: id.into(),
        provider: Provider::Github,
        connection_id: "github:local".into(),
        target_id: "org/app".into(),
        workflow_name: None,
        kind: "workflow".into(),
        title: "Deploy production".into(),
        project: "Example App".into(),
        branch: None,
        url: "https://github.com/example/app/actions/runs/1".into(),
        status,
        raw_status: format!("{status:?}"),
        actor: None,
        ownership,
        started_at: "2026-09-30T00:00:00Z".into(),
        finished_at: None,
        updated_at: "2026-09-30T00:01:00Z".into(),
    }
}

fn preferences(scope: NotificationScope) -> Preferences {
    Preferences {
        notifications: NotificationPreferences {
            enabled: true,
            rules: vec![NotificationRule {
                target_key: target().key(),
                scope,
                statuses: vec![
                    ActivityStatus::Success,
                    ActivityStatus::Failed,
                    ActivityStatus::Canceled,
                ],
                enabled: true,
            }],
        },
        ..Preferences::default()
    }
}

fn observed_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-30T00:02:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn observe(
    engine: &mut NotificationEngine,
    target: &Target,
    activities: &[Activity],
    preferences: &Preferences,
    generation: u64,
) -> Vec<NotificationDelivery> {
    engine.observe(target, activities, preferences, generation, observed_at())
}

#[test]
fn initial_terminal_baseline_is_silent_then_transition_notifies_once() {
    let mut engine = NotificationEngine::default();
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    assert!(observe(
        &mut engine,
        &selected,
        &[activity("old", ActivityStatus::Success, Ownership::Mine)],
        &prefs,
        1,
    )
    .is_empty());
    let running = activity("new", ActivityStatus::Running, Ownership::Mine);
    assert!(observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&running),
        &prefs,
        2,
    )
    .is_empty());
    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..running
    };
    let deliveries = observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&failed),
        &prefs,
        3,
    );
    assert_eq!(deliveries.len(), 1);
    engine.acknowledge(&deliveries[0]);
    assert!(observe(&mut engine, &selected, &[failed], &prefs, 3).is_empty());
    let stale_running = activity("stale", ActivityStatus::Running, Ownership::Mine);
    observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&stale_running),
        &prefs,
        4,
    );
    let stale_terminal = Activity {
        status: ActivityStatus::Failed,
        updated_at: "2026-09-30T00:00:00Z".into(),
        ..stale_running
    };
    assert!(observe(&mut engine, &selected, &[stale_terminal], &prefs, 5).is_empty());
}

#[test]
fn mine_excludes_unknown_while_all_includes_it() {
    let selected = target();
    let baseline = activity("run", ActivityStatus::Running, Ownership::Unknown);
    let finished = Activity {
        status: ActivityStatus::Success,
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..baseline.clone()
    };
    let mut mine = NotificationEngine::default();
    let mine_preferences = preferences(NotificationScope::Mine);
    observe(
        &mut mine,
        &selected,
        std::slice::from_ref(&baseline),
        &mine_preferences,
        1,
    );
    assert!(observe(
        &mut mine,
        &selected,
        std::slice::from_ref(&finished),
        &mine_preferences,
        2,
    )
    .is_empty());

    let mut all = NotificationEngine::default();
    let all_preferences = preferences(NotificationScope::All);
    observe(&mut all, &selected, &[baseline], &all_preferences, 1);
    assert_eq!(
        observe(&mut all, &selected, &[finished], &all_preferences, 2).len(),
        1
    );
}

#[test]
fn changing_rules_rebaselines_without_backlog() {
    let selected = target();
    let mut engine = NotificationEngine::default();
    let all = preferences(NotificationScope::All);
    observe(
        &mut engine,
        &selected,
        &[activity("run", ActivityStatus::Running, Ownership::Mine)],
        &all,
        1,
    );
    let mut changed = all.clone();
    changed.notifications.rules[0].statuses = vec![ActivityStatus::Success];
    engine.sync_preferences(&changed.notifications);
    assert!(observe(
        &mut engine,
        &selected,
        &[activity("run", ActivityStatus::Success, Ownership::Mine)],
        &changed,
        2,
    )
    .is_empty());
}

#[test]
fn adding_an_unrelated_target_rule_preserves_active_transition_tracking() {
    let selected = target();
    let mut engine = NotificationEngine::default();
    let initial = preferences(NotificationScope::All);
    let running = activity("in-flight", ActivityStatus::Running, Ownership::Mine);
    assert!(observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&running),
        &initial,
        1,
    )
    .is_empty());

    let added = Target {
        id: "org/new-app".into(),
        name: "new-app".into(),
        ..selected.clone()
    };
    let mut changed = initial.clone();
    changed.notifications.rules.push(NotificationRule {
        target_key: added.key(),
        scope: NotificationScope::Mine,
        statuses: vec![
            ActivityStatus::Success,
            ActivityStatus::Failed,
            ActivityStatus::Canceled,
        ],
        enabled: true,
    });
    engine.sync_preferences(&changed.notifications);

    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..running
    };
    assert_eq!(
        observe(&mut engine, &selected, &[failed], &changed, 2).len(),
        1
    );
}

#[test]
fn adding_an_unrelated_target_rule_preserves_pending_delivery_retry() {
    let selected = target();
    let mut engine = NotificationEngine::default();
    let initial = preferences(NotificationScope::All);
    let running = activity("pending", ActivityStatus::Running, Ownership::Mine);
    observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&running),
        &initial,
        1,
    );
    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..running
    };
    let pending = observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&failed),
        &initial,
        2,
    );
    assert_eq!(pending.len(), 1);
    engine.mark_failed(&pending[0], 2);

    let added = Target {
        id: "org/new-app".into(),
        name: "new-app".into(),
        ..selected.clone()
    };
    let mut changed = initial.clone();
    changed.notifications.rules.push(NotificationRule {
        target_key: added.key(),
        scope: NotificationScope::Mine,
        statuses: vec![ActivityStatus::Failed],
        enabled: true,
    });
    engine.sync_preferences(&changed.notifications);

    let retried = observe(&mut engine, &selected, &[failed], &changed, 3);
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].notification, pending[0].notification);
}

#[test]
fn bookkeeping_is_bounded() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    observe(&mut engine, &selected, &[], &prefs, 1);
    for index in 0..(MAX_OBSERVATIONS + 20) {
        observe(
            &mut engine,
            &selected,
            &[activity(
                &format!("run-{index}"),
                ActivityStatus::Running,
                Ownership::Mine,
            )],
            &prefs,
            2,
        );
    }
    assert_eq!(
        engine.baselines[&TargetKey::from(&selected)]
            .observations
            .len(),
        MAX_OBSERVATIONS
    );
}

#[test]
fn empty_and_split_initial_baselines_suppress_old_terminal_rows() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    assert!(observe(&mut engine, &selected, &[], &prefs, 1).is_empty());
    let old = activity("old", ActivityStatus::Failed, Ownership::Mine);
    assert!(observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&old),
        &prefs,
        1,
    )
    .is_empty());
    assert!(observe(&mut engine, &selected, &[old], &prefs, 2).is_empty());
    let newly_finished = Activity {
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..activity("new", ActivityStatus::Success, Ownership::Mine)
    };
    assert_eq!(
        observe(&mut engine, &selected, &[newly_finished], &prefs, 3).len(),
        1
    );
}

#[test]
fn recent_and_active_pages_share_the_previous_generation_boundary() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    observe(&mut engine, &selected, &[], &prefs, 1);
    let later_poll = DateTime::parse_from_rfc3339("2026-09-30T00:04:00Z")
        .unwrap()
        .with_timezone(&Utc);
    assert!(engine
        .observe(&selected, &[], &prefs, 2, later_poll)
        .is_empty());
    let finished_between_polls = Activity {
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..activity("active-result", ActivityStatus::Success, Ownership::Mine)
    };
    assert_eq!(
        engine
            .observe(&selected, &[finished_between_polls], &prefs, 2, later_poll,)
            .len(),
        1
    );
}

#[test]
fn response_receipt_time_does_not_suppress_a_known_running_transition() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    let receipt = DateTime::parse_from_rfc3339("2026-09-30T12:00:02Z")
        .unwrap()
        .with_timezone(&Utc);
    let running = Activity {
        updated_at: "2026-09-30T12:00:00Z".into(),
        ..activity("timeline", ActivityStatus::Running, Ownership::Mine)
    };
    assert!(engine
        .observe(
            &selected,
            std::slice::from_ref(&running),
            &prefs,
            1,
            receipt,
        )
        .is_empty());
    let completed = Activity {
        status: ActivityStatus::Success,
        finished_at: Some("2026-09-30T12:00:01Z".into()),
        updated_at: "2026-09-30T12:00:01Z".into(),
        ..running
    };
    assert_eq!(
        engine
            .observe(&selected, &[completed], &prefs, 2, receipt)
            .len(),
        1
    );
}

#[test]
fn equal_timestamp_active_to_terminal_is_valid_but_older_transition_is_not() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    let running = Activity {
        updated_at: "2026-09-30T12:00:00Z".into(),
        ..activity("equal-time", ActivityStatus::Running, Ownership::Mine)
    };
    engine.observe(
        &selected,
        std::slice::from_ref(&running),
        &prefs,
        1,
        observed_at(),
    );
    engine
        .baselines
        .get_mut(&TargetKey::from(&selected))
        .unwrap()
        .replay_floor = Some(
        DateTime::parse_from_rfc3339("2026-09-30T12:05:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let terminal = Activity {
        status: ActivityStatus::Failed,
        finished_at: Some("2026-09-30T12:00:00Z".into()),
        ..running
    };
    assert_eq!(
        engine
            .observe(&selected, &[terminal], &prefs, 2, observed_at())
            .len(),
        1
    );
}

#[test]
fn delayed_unseen_terminal_within_overlap_is_delivered_but_older_replay_is_not() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    let startup = DateTime::parse_from_rfc3339("2026-09-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    engine.observe(&selected, &[], &prefs, 1, startup);
    let high_water = Activity {
        updated_at: "2026-09-30T12:20:00Z".into(),
        ..activity("newer-running", ActivityStatus::Running, Ownership::Mine)
    };
    engine.observe(
        &selected,
        &[high_water],
        &prefs,
        2,
        startup + Duration::minutes(21),
    );
    let delayed = Activity {
        finished_at: Some("2026-09-30T12:15:00Z".into()),
        updated_at: "2026-09-30T12:15:00Z".into(),
        ..activity("delayed", ActivityStatus::Failed, Ownership::Mine)
    };
    let delayed_delivery = engine.observe(
        &selected,
        &[delayed],
        &prefs,
        3,
        startup + Duration::minutes(22),
    );
    assert_eq!(delayed_delivery.len(), 1);
    engine.acknowledge(&delayed_delivery[0]);
    let old_replay = Activity {
        finished_at: Some("2026-09-30T12:05:00Z".into()),
        updated_at: "2026-09-30T12:05:00Z".into(),
        ..activity("evicted-old", ActivityStatus::Failed, Ownership::Mine)
    };
    assert!(engine
        .observe(
            &selected,
            &[old_replay],
            &prefs,
            4,
            startup + Duration::minutes(23),
        )
        .is_empty());
}

#[test]
fn freshness_boundary_blocks_history_expansion_and_eviction_replay() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    observe(&mut engine, &selected, &[], &prefs, 1);
    for index in 0..(MAX_OBSERVATIONS + 1) {
        observe(
            &mut engine,
            &selected,
            &[activity(
                &format!("active-{index}"),
                ActivityStatus::Running,
                Ownership::Mine,
            )],
            &prefs,
            2,
        );
    }
    let replay_key = DeliveryKey {
        target: TargetKey::from(&selected),
        activity_id: "active-0".into(),
        status: ActivityStatus::Success,
    };
    engine.delivered.insert(replay_key.clone());
    engine.delivered_order.push_back(replay_key.clone());
    for index in 0..MAX_DELIVERED {
        let key = DeliveryKey {
            target: TargetKey::from(&selected),
            activity_id: format!("delivered-{index}"),
            status: ActivityStatus::Failed,
        };
        engine.delivered.insert(key.clone());
        engine.delivered_order.push_back(key);
    }
    engine.prune_delivered();
    assert!(!engine.delivered.contains(&replay_key));
    let replayed_old = activity("active-0", ActivityStatus::Success, Ownership::Mine);
    assert!(observe(&mut engine, &selected, &[replayed_old], &prefs, 3,).is_empty());
}

#[test]
fn acknowledged_record_does_not_replay_after_global_other_target_eviction() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    let startup = DateTime::parse_from_rfc3339("2026-09-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let running = Activity {
        updated_at: "2026-09-30T12:01:00Z".into(),
        ..activity("acknowledged-a", ActivityStatus::Running, Ownership::Mine)
    };
    engine.observe(
        &selected,
        std::slice::from_ref(&running),
        &prefs,
        1,
        startup,
    );
    let terminal = Activity {
        status: ActivityStatus::Failed,
        finished_at: Some("2026-09-30T12:02:00Z".into()),
        updated_at: "2026-09-30T12:02:00Z".into(),
        ..running
    };
    let delivered = engine.observe(
        &selected,
        std::slice::from_ref(&terminal),
        &prefs,
        2,
        startup + Duration::minutes(3),
    );
    assert_eq!(delivered.len(), 1);
    let acknowledged_key = delivered[0].key.clone();
    engine.acknowledge(&delivered[0]);

    for index in 0..MAX_DELIVERED {
        let key = DeliveryKey {
            target: TargetKey {
                provider: Provider::Github,
                connection_id: "github:local".into(),
                target_id: format!("org/delivered-{index}"),
            },
            activity_id: format!("other-delivery-{index}"),
            status: ActivityStatus::Failed,
        };
        engine.delivered.insert(key.clone());
        engine.delivered_order.push_back(key);
    }
    engine.prune_delivered();
    assert!(!engine.delivered.contains(&acknowledged_key));

    for target_index in 0..(MAX_BASELINES - 1) {
        let other_target = Target {
            id: format!("org/other-{target_index}"),
            ..selected.clone()
        };
        for activity_index in 0..22 {
            let other_activity = Activity {
                id: format!("other-{target_index}-{activity_index}"),
                target_id: other_target.id.clone(),
                updated_at: "2026-09-30T12:02:00Z".into(),
                ..activity("unused", ActivityStatus::Running, Ownership::Mine)
            };
            engine.observe(
                &other_target,
                &[other_activity],
                &prefs,
                3,
                startup + Duration::minutes(3),
            );
        }
    }
    let selected_key = TargetKey::from(&selected);
    assert!(!engine.baselines[&selected_key]
        .observations
        .contains_key("acknowledged-a"));
    assert_eq!(
        engine.baselines[&selected_key].replay_floor,
        terminal_event_time(&terminal)
    );
    let stale_running = Activity {
        status: ActivityStatus::Running,
        finished_at: None,
        updated_at: "2026-09-30T12:01:00Z".into(),
        ..terminal.clone()
    };
    assert!(engine
        .observe(
            &selected,
            &[stale_running],
            &prefs,
            4,
            startup + Duration::minutes(4),
        )
        .is_empty());
    assert!(!engine.baselines[&selected_key]
        .observations
        .contains_key("acknowledged-a"));
    assert!(engine
        .observe(
            &selected,
            &[terminal],
            &prefs,
            5,
            startup + Duration::minutes(5),
        )
        .is_empty());
}

#[test]
fn disable_enable_aba_rebaselines_before_the_next_observation() {
    let selected = target();
    let enabled = preferences(NotificationScope::All);
    let mut disabled = enabled.clone();
    disabled.notifications.enabled = false;
    let mut engine = NotificationEngine::default();
    observe(
        &mut engine,
        &selected,
        &[activity("run", ActivityStatus::Running, Ownership::Mine)],
        &enabled,
        1,
    );
    engine.sync_preferences(&disabled.notifications);
    engine.sync_preferences(&enabled.notifications);
    assert!(observe(
        &mut engine,
        &selected,
        &[activity("run", ActivityStatus::Failed, Ownership::Mine)],
        &enabled,
        2,
    )
    .is_empty());
}

#[test]
fn failed_delivery_remains_pending_and_successful_ack_deduplicates() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    let running = activity("run", ActivityStatus::Running, Ownership::Mine);
    observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&running),
        &prefs,
        1,
    );
    let failed = Activity {
        status: ActivityStatus::Failed,
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..running
    };
    let first = observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&failed),
        &prefs,
        2,
    );
    assert_eq!(first.len(), 1);
    engine.mark_failed(&first[0], 2);
    assert!(observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&failed),
        &prefs,
        2,
    )
    .is_empty());
    let retry = observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&failed),
        &prefs,
        3,
    );
    assert_eq!(retry.len(), 1);
    engine.acknowledge(&retry[0]);
    assert!(observe(&mut engine, &selected, &[failed], &prefs, 4).is_empty());
}

#[test]
fn notification_includes_service_execution_and_actor_context() {
    let run = Activity {
        provider: Provider::Vercel,
        workflow_name: Some("orbit-web".into()),
        title: "Ship checkout".into(),
        project: "Orbit".into(),
        branch: Some("main".into()),
        actor: Some("alex".into()),
        ..activity("run", ActivityStatus::Success, Ownership::Mine)
    };
    let notification = localize(&run, Language::Korean);
    assert_eq!(notification.title, "Vercel · 실행 성공");
    assert_eq!(
        notification.body,
        "orbit-web · Ship checkout\nOrbit · main · alex"
    );
}

#[test]
fn notification_omits_duplicate_and_missing_execution_names() {
    for name in [None, Some("  "), Some(" Deploy production ")] {
        let run = Activity {
            workflow_name: name.map(str::to_owned),
            branch: Some(" ".into()),
            actor: Some(" ".into()),
            ..activity("run", ActivityStatus::Failed, Ownership::Unknown)
        };
        let notification = localize(&run, Language::English);
        assert_eq!(notification.title, "GitHub Actions · Run failed");
        assert_eq!(
            notification.body,
            "Deploy production\nExample App · Unknown actor"
        );
    }
}

#[test]
fn notification_unknown_actor_follows_language_and_outcome() {
    for (language, status, label, actor) in [
        (
            Language::Korean,
            ActivityStatus::Success,
            "실행 성공",
            "실행자 미확인",
        ),
        (
            Language::English,
            ActivityStatus::Failed,
            "Run failed",
            "Unknown actor",
        ),
        (
            Language::Japanese,
            ActivityStatus::Canceled,
            "実行キャンセル",
            "実行者不明",
        ),
        (
            Language::SimplifiedChinese,
            ActivityStatus::Success,
            "运行成功",
            "执行者未知",
        ),
    ] {
        let notification = localize(&activity("run", status, Ownership::Unknown), language);
        assert_eq!(notification.title, format!("GitHub Actions · {label}"));
        assert_eq!(
            notification.body,
            format!("Deploy production\nExample App · {actor}")
        );
    }
}

#[test]
fn notification_keeps_external_multiline_text_within_two_body_lines() {
    let run = Activity {
        workflow_name: Some("  Deploy\nproduction ".into()),
        title: "Deploy production".into(),
        branch: Some("release\nnext".into()),
        actor: Some(" alex\tdev ".into()),
        ..activity("run", ActivityStatus::Canceled, Ownership::Mine)
    };
    let notification = localize(&run, Language::Korean);
    assert_eq!(
        notification.body,
        "Deploy production\nExample App · release next · alex dev"
    );
}

#[test]
fn forgotten_target_rebaselines_silently_after_an_identity_change() {
    let selected = target();
    let prefs = preferences(NotificationScope::All);
    let mut engine = NotificationEngine::default();
    let running = activity("run", ActivityStatus::Running, Ownership::Mine);
    observe(
        &mut engine,
        &selected,
        std::slice::from_ref(&running),
        &prefs,
        1,
    );
    engine.forget_targets(&[selected.key()]);
    let finished = Activity {
        status: ActivityStatus::Success,
        updated_at: "2026-09-30T00:03:00Z".into(),
        ..running
    };
    let backlog = [
        finished,
        activity("older", ActivityStatus::Failed, Ownership::Mine),
    ];
    assert!(
        observe(&mut engine, &selected, &backlog, &prefs, 2).is_empty(),
        "the first query after forgetting must only rebuild the baseline"
    );
    let next = Activity {
        status: ActivityStatus::Failed,
        updated_at: "2026-09-30T00:05:00Z".into(),
        ..activity("next", ActivityStatus::Running, Ownership::Mine)
    };
    observe(
        &mut engine,
        &selected,
        &[activity("next", ActivityStatus::Running, Ownership::Mine)],
        &prefs,
        3,
    );
    assert_eq!(observe(&mut engine, &selected, &[next], &prefs, 4).len(), 1);
}
