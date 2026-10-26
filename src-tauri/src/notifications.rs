use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Duration, Utc};
use tauri::{plugin::PermissionState, AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::{
    models::{Activity, ActivityKey, ActivityStatus, Ownership, Provider, Target},
    preferences::{resolved_language, Language, NotificationPreferences, NotificationScope},
};

const MAX_BASELINES: usize = 500;
const MAX_OBSERVATIONS: usize = 10_000;
const MAX_DELIVERED: usize = 4096;
const MAX_PENDING: usize = 512;
const PENDING_TTL_HOURS: i64 = 24;
const DELAYED_EVENT_OVERLAP_MINUTES: i64 = 10;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TargetKey {
    provider: Provider,
    connection_id: String,
    target_id: String,
}

impl From<&Target> for TargetKey {
    fn from(target: &Target) -> Self {
        Self {
            provider: target.provider,
            connection_id: target.connection_id.clone(),
            target_id: target.id.clone(),
        }
    }
}

impl TargetKey {
    fn preference_key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.provider.as_str(),
            self.connection_id,
            self.target_id
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DeliveryKey {
    target: TargetKey,
    activity_id: String,
    status: ActivityStatus,
}

#[derive(Clone, Copy, Debug)]
struct Observation {
    status: ActivityStatus,
    event_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
struct Baseline {
    observations: HashMap<String, Observation>,
    startup_cutoff: DateTime<Utc>,
    event_high_water: Option<DateTime<Utc>>,
    replay_floor: Option<DateTime<Utc>>,
    initialized_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeNotification {
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct NotificationDelivery {
    key: DeliveryKey,
    pub notification: NativeNotification,
}

impl NotificationDelivery {
    pub fn matches(&self, activity: &ActivityKey) -> bool {
        self.key.target.provider == activity.provider
            && self.key.target.target_id == activity.target_id
            && self.key.activity_id == activity.activity_id
    }
}

#[derive(Clone, Debug)]
struct PendingDelivery {
    notification: NativeNotification,
    expires_at: DateTime<Utc>,
    last_attempt_generation: Option<u64>,
}

#[derive(Default)]
pub(crate) struct NotificationEngine {
    preferences: Option<NotificationPreferences>,
    baselines: HashMap<TargetKey, Baseline>,
    baseline_order: VecDeque<TargetKey>,
    observation_order: VecDeque<(TargetKey, String)>,
    delivered: HashSet<DeliveryKey>,
    delivered_order: VecDeque<DeliveryKey>,
    pending: HashMap<DeliveryKey, PendingDelivery>,
    pending_order: VecDeque<DeliveryKey>,
}

impl NotificationEngine {
    pub fn sync_preferences(&mut self, preferences: &NotificationPreferences) {
        let Some(previous) = self.preferences.clone() else {
            self.preferences = Some(preferences.clone());
            return;
        };
        if previous == *preferences {
            return;
        }
        if previous.enabled != preferences.enabled {
            self.reset(preferences.clone());
            return;
        }

        let previous_rules: HashMap<_, _> = previous
            .rules
            .iter()
            .map(|rule| (rule.target_key.as_str(), rule))
            .collect();
        let next_rules: HashMap<_, _> = preferences
            .rules
            .iter()
            .map(|rule| (rule.target_key.as_str(), rule))
            .collect();
        let target_keys: HashSet<_> = previous_rules
            .keys()
            .chain(next_rules.keys())
            .copied()
            .collect();
        for target_key in target_keys {
            if previous_rules.get(target_key) != next_rules.get(target_key) {
                self.reset_target(target_key);
            }
        }
        self.preferences = Some(preferences.clone());
    }

    pub fn observe(
        &mut self,
        target: &Target,
        activities: &[Activity],
        preferences: &crate::preferences::Preferences,
        generation: u64,
        observed_at: DateTime<Utc>,
    ) -> Vec<NotificationDelivery> {
        self.sync_preferences(&preferences.notifications);
        if !preferences.notifications.enabled {
            return Vec::new();
        }

        let target_key = TargetKey::from(target);
        let is_initial = !self.baselines.contains_key(&target_key);
        if is_initial {
            self.insert_baseline(target_key.clone(), generation, observed_at);
        }

        let startup_cutoff = self.baselines[&target_key].startup_cutoff;
        let event_high_water = self.baselines[&target_key].event_high_water;
        let replay_floor = self.baselines[&target_key].replay_floor;
        let silent_collection = self.baselines[&target_key].initialized_generation == generation;
        for activity in activities.iter().filter(|activity| {
            activity.provider == target.provider
                && activity.connection_id == target.connection_id
                && activity.target_id == target.id
        }) {
            let candidate = Observation {
                status: activity.status,
                event_time: activity_event_time(activity),
            };
            let previous = self.baselines[&target_key]
                .observations
                .get(&activity.id)
                .copied();
            let blocked_by_replay_floor = previous.is_none()
                && replay_floor.is_some_and(|floor| {
                    candidate
                        .event_time
                        .is_none_or(|event_time| event_time <= floor)
                });
            let non_monotonic =
                previous.is_some_and(|previous| !newer_observation(previous, candidate));
            if blocked_by_replay_floor || non_monotonic {
                continue;
            }
            self.baselines
                .get_mut(&target_key)
                .expect("baseline inserted above")
                .observations
                .insert(activity.id.clone(), candidate);
            if previous.is_none() {
                self.observation_order
                    .push_back((target_key.clone(), activity.id.clone()));
            }
            let event_time = terminal_event_time(activity);
            let is_newer_transition = previous.is_some_and(|previous| {
                previous.status != activity.status
                    && event_time.is_some_and(|time| {
                        previous.event_time.is_none_or(|old| {
                            time > old || is_active_status(previous.status) && time == old
                        })
                    })
            });
            let is_delayed_unseen = previous.is_none()
                && event_time.is_some_and(|time| {
                    time > startup_cutoff
                        && replay_floor.is_none_or(|floor| time > floor)
                        && event_high_water.is_none_or(|high_water| {
                            time >= high_water - Duration::minutes(DELAYED_EVENT_OVERLAP_MINUTES)
                        })
                });
            if silent_collection
                || previous.is_some_and(|previous| previous.status == activity.status)
                || !is_notifiable(activity.status)
                || !matches_rule(target, activity, &preferences.notifications)
                || !(is_newer_transition || is_delayed_unseen)
            {
                continue;
            }
            let delivery_key = DeliveryKey {
                target: target_key.clone(),
                activity_id: activity.id.clone(),
                status: activity.status,
            };
            if !self.delivered.contains(&delivery_key) && !self.pending.contains_key(&delivery_key)
            {
                self.pending_order.push_back(delivery_key.clone());
                self.pending.insert(
                    delivery_key,
                    PendingDelivery {
                        notification: localize(activity, preferences.language),
                        expires_at: observed_at + Duration::hours(PENDING_TTL_HOURS),
                        last_attempt_generation: None,
                    },
                );
            }
        }
        if let Some(baseline) = self.baselines.get_mut(&target_key) {
            baseline.event_high_water = activities
                .iter()
                .filter_map(activity_event_time)
                .chain(baseline.event_high_water)
                .max();
        }
        self.prune_delivered();
        self.prune_observations();
        self.prune_pending(observed_at);
        self.pending_for_target(&target_key, generation)
    }

    pub fn acknowledge(&mut self, delivery: &NotificationDelivery) {
        self.pending.remove(&delivery.key);
        if self.delivered.insert(delivery.key.clone()) {
            self.delivered_order.push_back(delivery.key.clone());
        }
        self.prune_delivered();
    }

    pub fn mark_failed(&mut self, delivery: &NotificationDelivery, generation: u64) {
        if let Some(pending) = self.pending.get_mut(&delivery.key) {
            pending.last_attempt_generation = Some(generation);
        }
    }

    fn reset(&mut self, preferences: NotificationPreferences) {
        self.preferences = Some(preferences);
        self.baselines.clear();
        self.baseline_order.clear();
        self.observation_order.clear();
        self.delivered.clear();
        self.delivered_order.clear();
        self.pending.clear();
        self.pending_order.clear();
    }

    fn reset_target(&mut self, preference_key: &str) {
        self.baselines
            .retain(|target, _| target.preference_key() != preference_key);
        self.baseline_order
            .retain(|target| target.preference_key() != preference_key);
        self.observation_order
            .retain(|(target, _)| target.preference_key() != preference_key);
        self.delivered
            .retain(|delivery| delivery.target.preference_key() != preference_key);
        self.delivered_order
            .retain(|delivery| delivery.target.preference_key() != preference_key);
        self.pending
            .retain(|delivery, _| delivery.target.preference_key() != preference_key);
        self.pending_order
            .retain(|delivery| delivery.target.preference_key() != preference_key);
    }

    fn insert_baseline(&mut self, key: TargetKey, generation: u64, observed_at: DateTime<Utc>) {
        self.baselines.insert(
            key.clone(),
            Baseline {
                observations: HashMap::new(),
                startup_cutoff: observed_at,
                event_high_water: None,
                replay_floor: None,
                initialized_generation: generation,
            },
        );
        self.baseline_order.push_back(key);
        while self.baseline_order.len() > MAX_BASELINES {
            if let Some(oldest) = self.baseline_order.pop_front() {
                self.baselines.remove(&oldest);
                self.observation_order
                    .retain(|(target, _)| target != &oldest);
            }
        }
    }

    fn prune_delivered(&mut self) {
        while self.delivered_order.len() > MAX_DELIVERED {
            if let Some(oldest) = self.delivered_order.pop_front() {
                self.delivered.remove(&oldest);
            }
        }
    }

    fn prune_observations(&mut self) {
        while self.observation_order.len() > MAX_OBSERVATIONS {
            if let Some((target, activity_id)) = self.observation_order.pop_front() {
                if let Some(baseline) = self.baselines.get_mut(&target) {
                    if let Some(evicted) = baseline.observations.remove(&activity_id) {
                        if let Some(event_time) = evicted.event_time {
                            baseline.replay_floor = Some(
                                baseline
                                    .replay_floor
                                    .map_or(event_time, |floor| floor.max(event_time)),
                            );
                        }
                    }
                }
            }
        }
    }

    fn prune_pending(&mut self, now: DateTime<Utc>) {
        self.pending.retain(|_, pending| pending.expires_at > now);
        self.pending_order
            .retain(|key| self.pending.contains_key(key));
        while self.pending_order.len() > MAX_PENDING {
            if let Some(oldest) = self.pending_order.pop_front() {
                self.pending.remove(&oldest);
            }
        }
    }

    fn pending_for_target(&self, target: &TargetKey, generation: u64) -> Vec<NotificationDelivery> {
        self.pending_order
            .iter()
            .filter(|key| &key.target == target)
            .filter_map(|key| {
                let pending = self.pending.get(key)?;
                (pending.last_attempt_generation != Some(generation)).then(|| {
                    NotificationDelivery {
                        key: key.clone(),
                        notification: pending.notification.clone(),
                    }
                })
            })
            .collect()
    }
}

fn terminal_event_time(activity: &Activity) -> Option<DateTime<Utc>> {
    is_notifiable(activity.status)
        .then(|| activity_event_time(activity))
        .flatten()
}

fn activity_event_time(activity: &Activity) -> Option<DateTime<Utc>> {
    activity
        .finished_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .or_else(|| DateTime::parse_from_rfc3339(&activity.updated_at).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn is_notifiable(status: ActivityStatus) -> bool {
    matches!(
        status,
        ActivityStatus::Success | ActivityStatus::Failed | ActivityStatus::Canceled
    )
}

fn is_active_status(status: ActivityStatus) -> bool {
    matches!(
        status,
        ActivityStatus::Queued | ActivityStatus::Running | ActivityStatus::Waiting
    )
}

fn is_terminal_status(status: ActivityStatus) -> bool {
    matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    )
}

fn newer_observation(previous: Observation, candidate: Observation) -> bool {
    match (previous.event_time, candidate.event_time) {
        (Some(previous_time), Some(candidate_time)) => {
            candidate_time > previous_time
                || candidate_time == previous_time
                    && is_active_status(previous.status)
                    && is_terminal_status(candidate.status)
        }
        (None, Some(_)) => true,
        _ => false,
    }
}

fn matches_rule(
    target: &Target,
    activity: &Activity,
    preferences: &NotificationPreferences,
) -> bool {
    preferences.rules.iter().any(|rule| {
        rule.enabled
            && rule.target_key == target.key()
            && rule.statuses.contains(&activity.status)
            && match rule.scope {
                NotificationScope::Mine => activity.ownership == Ownership::Mine,
                // "all" intentionally includes known others and unknown ownership.
                NotificationScope::All => true,
            }
    })
}

fn localize(activity: &Activity, language: Language) -> NativeNotification {
    let status = activity.status;
    let language = resolved_language(&language);
    let title = match (language, status) {
        (Language::Korean, ActivityStatus::Success) => "실행 성공",
        (Language::Korean, ActivityStatus::Failed) => "실행 실패",
        (Language::Korean, ActivityStatus::Canceled) => "실행 취소",
        (Language::Japanese, ActivityStatus::Success) => "実行成功",
        (Language::Japanese, ActivityStatus::Failed) => "実行失敗",
        (Language::Japanese, ActivityStatus::Canceled) => "実行キャンセル",
        (Language::SimplifiedChinese, ActivityStatus::Success) => "运行成功",
        (Language::SimplifiedChinese, ActivityStatus::Failed) => "运行失败",
        (Language::SimplifiedChinese, ActivityStatus::Canceled) => "运行已取消",
        (_, ActivityStatus::Success) => "Run succeeded",
        (_, ActivityStatus::Failed) => "Run failed",
        (_, ActivityStatus::Canceled) => "Run canceled",
        _ => unreachable!("only notifiable statuses are localized"),
    };
    NativeNotification {
        title: format!("{} · {title}", provider_label(activity.provider)),
        body: notification_body(activity, language),
    }
}

fn notification_body(activity: &Activity, language: Language) -> String {
    let title = notification_text(&activity.title);
    let workflow = activity
        .workflow_name
        .as_deref()
        .map(notification_text)
        .filter(|name| !name.is_empty() && name != &title);
    let heading = [workflow.as_deref(), Some(title.as_str())]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let unknown_actor = match language {
        Language::Korean => "실행자 미확인",
        Language::Japanese => "実行者不明",
        Language::SimplifiedChinese => "执行者未知",
        _ => "Unknown actor",
    };
    let actor = activity
        .actor
        .as_deref()
        .map(notification_text)
        .filter(|actor| !actor.is_empty())
        .unwrap_or_else(|| unknown_actor.into());
    let context = [
        Some(activity.project.as_str()),
        activity.branch.as_deref(),
        Some(actor.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(notification_text)
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    [heading, context]
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn notification_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "GitHub Actions",
        Provider::Vercel => "Vercel",
        Provider::Aws => "AWS",
        Provider::GithubDeployments => "GitHub Deployments",
        Provider::Railway => "Railway",
        Provider::Netlify => "Netlify",
        Provider::Render => "Render",
        Provider::CloudflarePages => "Cloudflare Pages",
        Provider::CloudflareWorkers => "Cloudflare Workers",
        Provider::Heroku => "Heroku",
        Provider::Fly => "Fly.io",
        Provider::Gitlab => "GitLab",
        Provider::Bitbucket => "Bitbucket",
        Provider::Digitalocean => "DigitalOcean",
        Provider::Gcloud => "Google Cloud",
        Provider::Azure => "Azure App Service",
        Provider::Eas => "Expo / EAS",
        Provider::Firebase => "Firebase",
        Provider::Supabase => "Supabase",
        Provider::Fastlane => "fastlane",
    }
}

pub(crate) fn dispatch_native<R: Runtime>(
    app: &AppHandle<R>,
    notification: &NativeNotification,
) -> Result<(), String> {
    let api = app.notification();
    let permission = api
        .permission_state()
        .map_err(|error| format!("Notification permission could not be checked: {error}"))?;
    if permission != PermissionState::Granted {
        return Err(
            "Notifications are enabled, but system permission has not been granted.".into(),
        );
    }
    #[cfg(target_os = "macos")]
    {
        macos_delivery::deliver(
            &app.config().identifier,
            &notification.title,
            &notification.body,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        api.builder()
            .title(&notification.title)
            .body(&notification.body)
            .show()
            .map_err(|error| format!("A system notification could not be delivered: {error}"))
    }
}

#[cfg(target_os = "macos")]
#[path = "notifications/macos_delivery.rs"]
mod macos_delivery;

#[cfg(test)]
#[path = "notifications/tests.rs"]
mod tests;
