mod auth;
mod github_binding;
mod history;
mod provider_connections;
mod refresh_wake;
mod tray;

#[cfg(test)]
use tray::active_tray_count;
use tray::update_tray;

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{Mutex, RwLock};

pub use github_binding::GithubAccountAccess;
use history::HistoryCache;
use refresh_wake::RefreshWake;

use crate::{
    cli::Toolchain,
    credentials::BitbucketCredential,
    models::{
        Account, AccountMode, Activity, ActivityStatus, Connection, ConnectionStatus, Issue,
        LoginStatus, Ownership, Provider, QueryHealth, Scope, Settings, SettingsChange, Snapshot,
        Target, LOCAL_GITHUB_ID, LOCAL_VERCEL_ID, MAX_HIDDEN_ACTIVITIES, MAX_TARGETS,
    },
    notifications::{NativeNotification, NotificationEngine},
    providers::aws,
    settings::SettingsStore,
};

const ACTIVE_POLL_INTERVAL: Duration = Duration::from_secs(30);
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(120);
const ERROR_POLL_INTERVAL: Duration = Duration::from_secs(240);

pub(crate) trait SnapshotPublisher {
    fn publish_snapshot(&self, snapshot: &Snapshot);
    fn publish_notification(&self, _notification: &NativeNotification) -> Result<(), String> {
        Ok(())
    }
}

impl<R: Runtime> SnapshotPublisher for AppHandle<R> {
    fn publish_snapshot(&self, snapshot: &Snapshot) {
        let _ = self.emit("snapshot-updated", snapshot.clone());
        update_tray(self, snapshot);
    }

    fn publish_notification(&self, notification: &NativeNotification) -> Result<(), String> {
        crate::notifications::dispatch_native(self, notification)
    }
}

#[cfg(test)]
struct NoopPublisher;

#[cfg(test)]
impl SnapshotPublisher for NoopPublisher {
    fn publish_snapshot(&self, _snapshot: &Snapshot) {}
}

pub struct AppState {
    pub tools: Toolchain,
    store: SettingsStore,
    snapshot: RwLock<Snapshot>,
    refresh_lock: Mutex<()>,
    refresh_wake: RefreshWake,
    publish_lock: Mutex<()>,
    settings_revision: AtomicU64,
    refresh_epoch: AtomicU64,
    history_lock: Mutex<()>,
    history_cache: Mutex<HistoryCache>,
    notification_engine: Mutex<NotificationEngine>,
    pub login_statuses: Mutex<HashMap<String, LoginStatus>>,
    github_pin_attempts: Mutex<HashMap<String, github_binding::PinAttempt>>,
}

impl AppState {
    pub async fn initialize(app_data_dir: std::path::PathBuf, resource_dir: Option<&Path>) -> Self {
        let store = SettingsStore::new(app_data_dir.clone());
        let snapshot = load_initial_snapshot(&store).await;
        Self {
            tools: Toolchain::discover(resource_dir, &app_data_dir).await,
            store,
            snapshot: RwLock::new(snapshot),
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

    pub async fn snapshot(&self) -> Snapshot {
        self.snapshot.read().await.clone()
    }

    pub fn request_refresh(&self) {
        self.refresh_wake.request();
    }

    pub(crate) fn ensure_settings_writable(&self) -> Result<(), String> {
        self.store.ensure_writable()
    }

    pub async fn account(
        &self,
        connection_id: &str,
        provider: Provider,
    ) -> Result<Account, String> {
        self.snapshot
            .read()
            .await
            .settings
            .accounts
            .iter()
            .find(|account| account.id == connection_id && account.provider == provider)
            .cloned()
            .ok_or_else(|| "The selected account does not exist for this provider.".to_owned())
    }

    pub async fn save_settings<P: SnapshotPublisher>(
        &self,
        mut settings: Settings,
        publisher: &P,
    ) -> Result<Settings, String> {
        settings.prune_target_references();
        settings.validate()?;
        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        validate_new_hidden_activities(&previous, &settings)?;
        if settings.accounts != previous.settings.accounts {
            return Err("Account definitions can only be changed with account commands.".into());
        }
        self.publish_settings_locked(settings, previous, publisher)
            .await
    }

    pub async fn change_settings<P: SnapshotPublisher>(
        &self,
        change: SettingsChange,
        publisher: &P,
    ) -> Result<Settings, String> {
        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        let activity_visibility = matches!(&change, SettingsChange::ActivityVisibility { .. });
        let settings =
            apply_settings_change(previous.settings.clone(), change, &previous.activities)?;
        if settings.accounts != previous.settings.accounts {
            return Err("Account definitions can only be changed with account commands.".into());
        }
        if activity_visibility {
            return self
                .publish_activity_visibility_locked(settings, previous, publisher)
                .await;
        }
        self.publish_settings_locked(settings, previous, publisher)
            .await
    }

    async fn publish_activity_visibility_locked<P: SnapshotPublisher>(
        &self,
        settings: Settings,
        previous: Snapshot,
        publisher: &P,
    ) -> Result<Settings, String> {
        let saved = self.store.save(settings).await?;
        let updated = Snapshot {
            settings: saved.clone(),
            ..previous
        };
        *self.snapshot.write().await = updated.clone();
        publisher.publish_snapshot(&updated);
        Ok(saved)
    }

    async fn publish_settings_locked<P: SnapshotPublisher>(
        &self,
        mut settings: Settings,
        previous: Snapshot,
        publisher: &P,
    ) -> Result<Settings, String> {
        self.ensure_settings_writable()?;
        add_default_rules_for_new_targets(&mut settings, &previous.settings);
        settings.prune_target_references();
        settings.validate()?;
        for account in &settings.accounts {
            self.tools.prepare_account(account)?;
        }
        let saved = self.store.save(settings).await?;
        self.notification_engine
            .lock()
            .await
            .sync_preferences(&saved.preferences.notifications);
        let accounts_changed = saved.accounts != previous.settings.accounts;
        let targets_changed = saved.targets != previous.settings.targets;
        let history_hours_changed = saved.history_hours != previous.settings.history_hours;
        let refresh_inputs_changed = accounts_changed || targets_changed || history_hours_changed;
        if refresh_inputs_changed {
            self.settings_revision.fetch_add(1, Ordering::SeqCst);
        }
        let account_ids: HashSet<&str> = saved
            .accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        let target_keys: HashSet<(&str, Provider, &str)> = saved
            .targets
            .iter()
            .map(|target| {
                (
                    target.connection_id.as_str(),
                    target.provider,
                    target.id.as_str(),
                )
            })
            .collect();
        let unchanged_target_keys: HashSet<String> = if history_hours_changed {
            HashSet::new()
        } else {
            let previous_accounts: HashMap<&str, &Account> = previous
                .settings
                .accounts
                .iter()
                .map(|account| (account.id.as_str(), account))
                .collect();
            let unchanged_account_ids: HashSet<&str> = saved
                .accounts
                .iter()
                .filter(|account| {
                    previous_accounts
                        .get(account.id.as_str())
                        .is_some_and(|previous| *previous == *account)
                })
                .map(|account| account.id.as_str())
                .collect();
            let previous_targets: HashMap<(&str, Provider, &str), &Target> = previous
                .settings
                .targets
                .iter()
                .map(|target| {
                    (
                        (
                            target.connection_id.as_str(),
                            target.provider,
                            target.id.as_str(),
                        ),
                        target,
                    )
                })
                .collect();
            saved
                .targets
                .iter()
                .filter(|target| {
                    unchanged_account_ids.contains(target.connection_id.as_str())
                        && previous_targets
                            .get(&(
                                target.connection_id.as_str(),
                                target.provider,
                                target.id.as_str(),
                            ))
                            .is_some_and(|previous| *previous == *target)
                })
                .map(Target::key)
                .collect()
        };
        let history = previous
            .history
            .iter()
            .filter(|entry| {
                unchanged_target_keys.contains(&format!(
                    "{}:{}:{}",
                    entry.provider.as_str(),
                    entry.connection_id,
                    entry.target_id
                ))
            })
            .cloned()
            .collect();
        let revision = self.settings_revision.load(Ordering::SeqCst);
        self.history_cache
            .lock()
            .await
            .retain_target_keys(&unchanged_target_keys, revision);
        let pending_targets = previous
            .pending_targets
            .iter()
            .filter(|key| unchanged_target_keys.contains(*key))
            .cloned()
            .collect();
        let query_health = previous
            .query_health
            .iter()
            .filter(|entry| {
                unchanged_target_keys.contains(&format!(
                    "{}:{}:{}",
                    entry.provider.as_str(),
                    entry.connection_id,
                    entry.target_id
                ))
            })
            .cloned()
            .collect();
        let updated = Snapshot {
            settings: saved.clone(),
            history,
            pending_targets,
            query_health,
            connections: previous
                .connections
                .into_iter()
                .filter(|connection| account_ids.contains(connection.connection_id.as_str()))
                .collect(),
            activities: previous
                .activities
                .into_iter()
                .filter(|activity| {
                    target_keys.contains(&(
                        activity.connection_id.as_str(),
                        activity.provider,
                        activity.target_id.as_str(),
                    ))
                })
                .collect(),
            issues: previous
                .issues
                .into_iter()
                .filter(|issue| {
                    issue
                        .connection_id
                        .as_deref()
                        .is_none_or(|id| account_ids.contains(id))
                })
                .collect(),
            ..previous
        };
        let updated = with_poll_interval(updated);
        *self.snapshot.write().await = updated.clone();
        publisher.publish_snapshot(&updated);
        Ok(saved)
    }

    pub async fn create_connection<P: SnapshotPublisher>(
        &self,
        provider: Provider,
        label: String,
        profile: Option<String>,
        publisher: &P,
    ) -> Result<Account, String> {
        self.ensure_settings_writable()?;
        if matches!(
            provider,
            Provider::CloudflarePages | Provider::CloudflareWorkers | Provider::Bitbucket
        ) {
            return Err("Use the service token connection form.".into());
        }
        let account = match provider {
            Provider::Github | Provider::GithubDeployments | Provider::Vercel => {
                if profile.is_some() {
                    return Err("Only AWS connections accept a profile.".into());
                }
                Account {
                    id: format!("{}-{}", provider.as_str(), uuid::Uuid::new_v4()),
                    provider,
                    label,
                    mode: AccountMode::Isolated,
                    profile: None,
                    cli_username: None,
                }
            }
            Provider::Aws => {
                let profile = profile
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "Select an AWS profile.".to_owned())?;
                if !aws::profile_names()?.contains(&profile) {
                    return Err("The selected AWS profile was not found.".into());
                }
                Account {
                    id: format!("aws-{}", uuid::Uuid::new_v4()),
                    provider,
                    label,
                    mode: AccountMode::Local,
                    profile: Some(profile),
                    cli_username: None,
                }
            }
            _ => {
                if profile.is_some() {
                    return Err("Only AWS connections accept a profile.".into());
                }
                if !crate::providers::catalog::supported(provider) {
                    return Err(crate::providers::catalog::limitation(provider).to_owned());
                }
                Account {
                    id: format!("{}:local", provider.as_str()),
                    provider,
                    label,
                    mode: AccountMode::Local,
                    profile: None,
                    cli_username: None,
                }
            }
        };
        account.validate()?;
        let account = if matches!(provider, Provider::Firebase | Provider::Eas) {
            let cli = self
                .tools
                .invocation_for(&account)?
                .ok_or_else(|| "Install and sign in to the service CLI first.".to_owned())?;
            let identity = if provider == Provider::Firebase {
                crate::providers::mobile::firebase::identity(&cli).await
            } else {
                crate::providers::mobile::eas::identity(&cli).await
            }
            .map_err(|error| error.user_message(provider.as_str()))?;
            Account {
                profile: Some(identity),
                ..account
            }
        } else {
            account
        };
        self.tools.prepare_account(&account)?;
        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        let mut settings = previous.settings.clone();
        if account.provider == Provider::Aws
            && settings.accounts.iter().any(|existing| {
                existing.provider == Provider::Aws && existing.profile == account.profile
            })
        {
            return Err("That AWS profile already has a connection.".into());
        }
        if settings
            .accounts
            .iter()
            .any(|existing| existing.id == account.id)
        {
            return Err("This local CLI session is already connected.".into());
        }
        settings.accounts.push(account.clone());
        if !settings.panel.providers.contains(&provider) {
            settings.panel.providers.push(provider);
        }
        settings.validate()?;
        self.publish_settings_locked(settings, previous, publisher)
            .await?;
        Ok(account)
    }

    pub async fn connect_bitbucket<P: SnapshotPublisher>(
        &self,
        credential: BitbucketCredential,
        connection_id: Option<&str>,
        publisher: &P,
    ) -> Result<Account, String> {
        self.ensure_settings_writable()?;
        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        if let Some(connection_id) = connection_id {
            let account = previous
                .settings
                .accounts
                .iter()
                .find(|account| {
                    account.id == connection_id && account.provider == Provider::Bitbucket
                })
                .cloned()
                .ok_or_else(|| "The selected Bitbucket account does not exist.".to_owned())?;
            let expected_identity = account.profile.as_deref().unwrap_or_default();
            if expected_identity != format!("{}|{}", credential.uuid, credential.account_id) {
                return Err(
                    "The API token belongs to a different Bitbucket account. Existing targets were preserved."
                        .into(),
                );
            }
            crate::credentials::set_bitbucket(connection_id, credential).await?;
            return Ok(account);
        }

        let account = Account {
            id: format!("bitbucket:{}", uuid::Uuid::new_v4()),
            provider: Provider::Bitbucket,
            label: credential.display_name.clone(),
            mode: AccountMode::Isolated,
            profile: Some(format!("{}|{}", credential.uuid, credential.account_id)),
            cli_username: None,
        };
        account.validate()?;
        let mut settings = previous.settings.clone();
        settings.accounts.push(account.clone());
        if !settings.panel.providers.contains(&Provider::Bitbucket) {
            settings.panel.providers.push(Provider::Bitbucket);
        }
        settings.validate()?;
        crate::credentials::set_bitbucket(&account.id, credential).await?;
        if let Err(message) = self
            .publish_settings_locked(settings, previous, publisher)
            .await
        {
            let cleanup = crate::credentials::delete_bitbucket(&account.id).await;
            return Err(match cleanup {
                Ok(()) => message,
                Err(cleanup) => format!("{message} Credential cleanup also failed: {cleanup}"),
            });
        }
        Ok(account)
    }

    pub async fn remove_connection<P: SnapshotPublisher>(
        &self,
        connection_id: &str,
        publisher: &P,
    ) -> Result<Settings, String> {
        self.ensure_settings_writable()?;
        if matches!(connection_id, LOCAL_GITHUB_ID | LOCAL_VERCEL_ID) {
            return Err("The default local CLI accounts cannot be removed.".into());
        }
        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        let removed_account = previous
            .settings
            .accounts
            .iter()
            .find(|account| account.id == connection_id)
            .cloned()
            .ok_or_else(|| "The selected account does not exist.".to_owned())?;
        let restore_credential = if removed_account.provider == Provider::Bitbucket {
            Some(crate::credentials::backup_and_delete_bitbucket(connection_id).await?)
        } else {
            None
        };
        let restore_cloudflare = if matches!(
            removed_account.provider,
            Provider::CloudflarePages | Provider::CloudflareWorkers
        ) && removed_account.mode == AccountMode::Isolated
        {
            Some(
                crate::cloudflare_credentials::backup_and_delete(
                    removed_account.provider,
                    connection_id,
                )
                .await?,
            )
        } else {
            None
        };
        let mut settings = previous.settings.clone();
        let before = settings.accounts.len();
        settings
            .accounts
            .retain(|account| account.id != connection_id);
        if settings.accounts.len() == before {
            return Err("The selected account does not exist.".into());
        }
        settings
            .targets
            .retain(|target| target.connection_id != connection_id);
        let saved = match self
            .publish_settings_locked(settings, previous, publisher)
            .await
        {
            Ok(saved) => saved,
            Err(message) => {
                if let Some(credential) = restore_cloudflare {
                    return match crate::cloudflare_credentials::restore(
                        removed_account.provider,
                        connection_id,
                        credential,
                    )
                    .await
                    {
                        Ok(()) => Err(message),
                        Err(restore) => Err(format!(
                            "{message} Credential rollback also failed: {restore}"
                        )),
                    };
                }
                if let Some(credential) = restore_credential {
                    let restore =
                        crate::credentials::restore_bitbucket(connection_id, credential).await;
                    return Err(match restore {
                        Ok(()) => message,
                        Err(restore) => {
                            format!("{message} Credential rollback also failed: {restore}")
                        }
                    });
                }
                return Err(message);
            }
        };
        self.login_statuses.lock().await.remove(connection_id);
        Ok(saved)
    }
}

async fn load_initial_snapshot(store: &SettingsStore) -> Snapshot {
    let settings = store.load().await.unwrap_or_default();
    Snapshot {
        settings_error: store.load_error(),
        ..Snapshot::initial(settings)
    }
}

fn apply_settings_change(
    mut settings: Settings,
    change: SettingsChange,
    activities: &[Activity],
) -> Result<Settings, String> {
    use crate::preferences::{NotificationRule, NotificationScope};

    match change {
        SettingsChange::Scope { value } => settings.scope = value,
        SettingsChange::DisplayProviderScope { provider, scope } => {
            settings.display_scopes.providers.remove(&provider);
            if let Some(scope) = scope {
                settings.display_scopes.providers.insert(provider, scope);
            }
        }
        SettingsChange::DisplayTargetScope { target_key, scope } => {
            require_target_key(&settings, &target_key)?;
            settings.display_scopes.targets.remove(&target_key);
            if let Some(scope) = scope {
                settings.display_scopes.targets.insert(target_key, scope);
            }
        }
        SettingsChange::HistoryHours { value } => settings.history_hours = value,
        SettingsChange::Language { value } => settings.preferences.language = value,
        SettingsChange::AutomaticUpdates { value } => {
            settings.preferences.check_updates_automatically = value;
        }
        SettingsChange::MenuBarAnimation { value } => {
            settings.preferences.animate_menu_bar_icon = value;
        }
        SettingsChange::Panel { patch } => {
            settings.panel.show_active = patch.show_active.unwrap_or(settings.panel.show_active);
            settings.panel.show_completed = patch
                .show_completed
                .unwrap_or(settings.panel.show_completed);
            settings.panel.row_limit = patch.row_limit.unwrap_or(settings.panel.row_limit);
        }
        SettingsChange::PanelProvider { provider, enabled } => {
            settings.panel.providers.retain(|item| *item != provider);
            if enabled {
                settings.panel.providers.push(provider);
            }
        }
        SettingsChange::PanelTarget {
            target_key,
            visible,
        } => {
            require_target_key(&settings, &target_key)?;
            settings
                .panel
                .hidden_targets
                .retain(|key| key != &target_key);
            if !visible {
                settings.panel.hidden_targets.push(target_key);
            }
        }
        SettingsChange::NotificationEnabled { enabled } => {
            settings.preferences.notifications.enabled = enabled;
        }
        SettingsChange::NotificationRules { rules } => {
            let mut merged = settings.preferences.notifications.rules.clone();
            for change in rules {
                require_target_key(&settings, &change.target_key)?;
                let position = merged
                    .iter()
                    .position(|rule| rule.target_key == change.target_key);
                let current = position.map(|index| &merged[index]);
                let rule = NotificationRule {
                    target_key: change.target_key,
                    enabled: change
                        .enabled
                        .or_else(|| current.map(|rule| rule.enabled))
                        .unwrap_or(false),
                    scope: change
                        .scope
                        .or_else(|| {
                            current.map(|rule| match rule.scope {
                                NotificationScope::Mine => Scope::Mine,
                                NotificationScope::All => Scope::All,
                            })
                        })
                        .map(|scope| match scope {
                            Scope::Mine => NotificationScope::Mine,
                            Scope::All => NotificationScope::All,
                        })
                        .unwrap_or(NotificationScope::Mine),
                    statuses: change
                        .statuses
                        .or_else(|| current.map(|rule| rule.statuses.clone()))
                        .unwrap_or_else(|| {
                            vec![
                                ActivityStatus::Success,
                                ActivityStatus::Failed,
                                ActivityStatus::Canceled,
                            ]
                        }),
                };
                if let Some(position) = position {
                    merged[position] = rule;
                } else {
                    merged.push(rule);
                }
            }
            settings.preferences.notifications.rules = merged;
        }
        SettingsChange::Target { target, selected } => {
            let key = target.key();
            settings.targets.retain(|item| item.key() != key);
            if selected {
                settings.targets.push(target);
            }
        }
        SettingsChange::Targets { targets, selected } => {
            if targets.is_empty() || targets.len() > MAX_TARGETS {
                return Err(format!(
                    "Bulk target changes must contain between 1 and {MAX_TARGETS} targets."
                ));
            }
            let changed_keys = targets
                .iter()
                .map(Target::key)
                .collect::<std::collections::HashSet<_>>();
            settings
                .targets
                .retain(|item| !changed_keys.contains(&item.key()));
            if selected {
                for target in targets {
                    if !settings
                        .targets
                        .iter()
                        .any(|item| item.key() == target.key())
                    {
                        settings.targets.push(target);
                    }
                }
            }
        }
        SettingsChange::AccountAlias {
            connection_id,
            alias,
        } => {
            if !settings
                .accounts
                .iter()
                .any(|account| account.id == connection_id)
            {
                return Err("The selected account does not exist.".into());
            }
            settings.preferences.account_aliases.remove(&connection_id);
            if let Some(alias) = alias {
                let alias = alias.trim();
                if alias.is_empty() || alias.chars().count() > 80 {
                    return Err("Account aliases must be between 1 and 80 characters.".into());
                }
                settings
                    .preferences
                    .account_aliases
                    .insert(connection_id, alias.to_owned());
            }
        }
        SettingsChange::ActivityVisibility { activity, hidden } => {
            activity.validate()?;
            let position = settings
                .hidden_activities
                .iter()
                .position(|item| item == &activity);
            if hidden {
                if !activities.iter().any(|item| activity.matches(item)) {
                    return Err("The selected activity does not exist.".into());
                }
                if position.is_none() {
                    if settings.hidden_activities.len() >= MAX_HIDDEN_ACTIVITIES {
                        return Err(format!(
                            "At most {MAX_HIDDEN_ACTIVITIES} activities can be hidden."
                        ));
                    }
                    settings.hidden_activities.push(activity);
                }
            } else if let Some(position) = position {
                settings.hidden_activities.remove(position);
            }
        }
    }
    settings.prune_target_references();
    settings.validate()?;
    Ok(settings)
}

fn require_target_key(settings: &Settings, target_key: &str) -> Result<(), String> {
    settings
        .targets
        .iter()
        .any(|target| target.key() == target_key)
        .then_some(())
        .ok_or_else(|| "The selected target does not exist.".to_owned())
}

fn add_default_rules_for_new_targets(settings: &mut Settings, previous: &Settings) {
    use crate::preferences::{NotificationRule, NotificationScope};

    let previous_keys: HashSet<_> = previous.targets.iter().map(Target::key).collect();
    let existing_rules: HashSet<_> = settings
        .preferences
        .notifications
        .rules
        .iter()
        .map(|rule| rule.target_key.clone())
        .collect();
    let new_rules = settings
        .targets
        .iter()
        .map(Target::key)
        .filter(|key| !previous_keys.contains(key) && !existing_rules.contains(key))
        .map(|target_key| NotificationRule {
            target_key,
            enabled: true,
            scope: NotificationScope::Mine,
            statuses: vec![
                ActivityStatus::Success,
                ActivityStatus::Failed,
                ActivityStatus::Canceled,
            ],
        })
        .collect::<Vec<_>>();
    settings.preferences.notifications.rules.extend(new_rules);
}

fn validate_new_hidden_activities(previous: &Snapshot, next: &Settings) -> Result<(), String> {
    for hidden in next
        .hidden_activities
        .iter()
        .filter(|hidden| !previous.settings.hidden_activities.contains(hidden))
    {
        if !previous
            .activities
            .iter()
            .any(|activity| hidden.matches(activity))
        {
            return Err("The selected activity does not exist.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
fn merge_with_cache(
    connection: &Connection,
    targets: &[Target],
    fresh: Vec<Activity>,
    provider_issues: &[Issue],
    cached: &[Activity],
    aggregate_issues: &mut Vec<Issue>,
) -> Vec<Activity> {
    let global_failure = provider_issues
        .iter()
        .any(|issue| issue.target_id.is_none());
    let failed_target_ids: HashSet<&str> = provider_issues
        .iter()
        .filter_map(|issue| issue.target_id.as_deref())
        .collect();
    let selected_ids: HashSet<&str> = targets.iter().map(|target| target.id.as_str()).collect();
    let fresh_ids: HashSet<(&str, &str)> = fresh
        .iter()
        .map(|activity| (activity.target_id.as_str(), activity.id.as_str()))
        .collect();
    let retained: Vec<Activity> = cached
        .iter()
        .filter(|activity| {
            activity.connection_id == connection.connection_id
                && activity.provider == connection.provider
                && selected_ids.contains(activity.target_id.as_str())
                && !fresh_ids.contains(&(activity.target_id.as_str(), activity.id.as_str()))
                && (global_failure || failed_target_ids.contains(activity.target_id.as_str()))
        })
        .cloned()
        .collect();
    if !retained.is_empty() {
        aggregate_issues.push(Issue {
            kind: crate::models::IssueKind::Other,
            provider: connection.provider,
            connection_id: Some(connection.connection_id.clone()),
            message: format!(
                "Some {} requests failed; the last known activity is retained and may be stale.",
                connection.provider.as_str()
            ),
            target_id: None,
        });
    }
    fresh.into_iter().chain(retained).collect()
}

fn retain_last_confirmed_accounts(
    previous: &[Connection],
    connections: Vec<Connection>,
) -> Vec<Connection> {
    connections
        .into_iter()
        .map(|connection| {
            if connection.status == ConnectionStatus::Error && connection.account.is_none() {
                let account = previous
                    .iter()
                    .find(|item| item.connection_id == connection.connection_id)
                    .and_then(|item| item.account.clone());
                Connection {
                    account,
                    ..connection
                }
            } else {
                connection
            }
        })
        .collect()
}

fn partition_cache_by_account(
    cached: &[Activity],
    previous_connections: &[Connection],
    connections: &[Connection],
) -> Vec<Activity> {
    cached
        .iter()
        .filter(|activity| {
            let previous = previous_connections
                .iter()
                .find(|item| item.connection_id == activity.connection_id);
            let current = connections
                .iter()
                .find(|item| item.connection_id == activity.connection_id);
            previous.and_then(|item| item.account.as_ref())
                == current.and_then(|item| item.account.as_ref())
        })
        .cloned()
        .collect()
}

pub fn start_polling(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let snapshot = state.refresh(&app).await;
            state.refresh_wake.wait(poll_interval(&snapshot)).await;
        }
    });
}

fn poll_interval(snapshot: &Snapshot) -> Duration {
    let connected: HashSet<&str> = snapshot
        .connections
        .iter()
        .filter(|connection| connection.status == ConnectionStatus::Connected)
        .map(|connection| connection.connection_id.as_str())
        .collect();
    if snapshot.activities.iter().any(|activity| {
        connected.contains(activity.connection_id.as_str())
            && crate::panel::visible_activity(activity, Utc::now())
            && matches!(
                activity.status,
                ActivityStatus::Queued | ActivityStatus::Running | ActivityStatus::Waiting
            )
    }) {
        ACTIVE_POLL_INTERVAL
    } else if !connected.is_empty() {
        IDLE_POLL_INTERVAL
    } else {
        ERROR_POLL_INTERVAL
    }
}

fn with_poll_interval(snapshot: Snapshot) -> Snapshot {
    Snapshot {
        poll_interval_seconds: poll_interval(&snapshot).as_secs(),
        ..snapshot
    }
}

fn newer_observation<'a>(current: &'a Activity, candidate: &'a Activity) -> &'a Activity {
    if candidate.updated_at > current.updated_at
        || candidate.updated_at == current.updated_at
            && is_active_status(current.status)
            && (is_terminal_status(candidate.status) || crate::panel::is_blocked(candidate))
    {
        candidate
    } else {
        current
    }
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

fn is_active_status(status: ActivityStatus) -> bool {
    matches!(
        status,
        ActivityStatus::Queued | ActivityStatus::Running | ActivityStatus::Waiting
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod settings_recovery_tests;

mod refresh;
