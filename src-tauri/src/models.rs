use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub(crate) const MAX_TARGETS: usize = 500;
pub(crate) const MAX_HIDDEN_ACTIVITIES: usize = 10_000;
const MAX_ACCOUNTS: usize = 32;
const MAX_TEXT_LEN: usize = 512;

pub const LOCAL_GITHUB_ID: &str = "github:local";
pub const LOCAL_VERCEL_ID: &str = "vercel:local";
pub const AWS_DISCOVERY_ID: &str = "aws:local";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Github,
    Vercel,
    Aws,
    GithubDeployments,
    Railway,
    Netlify,
    Render,
    CloudflarePages,
    CloudflareWorkers,
    Heroku,
    Fly,
    Gitlab,
    Bitbucket,
    Digitalocean,
    Gcloud,
    Azure,
    Eas,
    Firebase,
    Supabase,
    Fastlane,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountMode {
    Local,
    Isolated,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider: Provider,
    pub label: String,
    pub mode: AccountMode,
    pub profile: Option<String>,
    #[serde(default)]
    pub cli_username: Option<String>,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Vercel => "vercel",
            Self::Aws => "aws",
            Self::GithubDeployments => "github-deployments",
            Self::Railway => "railway",
            Self::Netlify => "netlify",
            Self::Render => "render",
            Self::CloudflarePages => "cloudflare-pages",
            Self::CloudflareWorkers => "cloudflare-workers",
            Self::Heroku => "heroku",
            Self::Fly => "fly",
            Self::Gitlab => "gitlab",
            Self::Bitbucket => "bitbucket",
            Self::Digitalocean => "digitalocean",
            Self::Gcloud => "gcloud",
            Self::Azure => "azure",
            Self::Eas => "eas",
            Self::Firebase => "firebase",
            Self::Supabase => "supabase",
            Self::Fastlane => "fastlane",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatus {
    Queued,
    Running,
    Waiting,
    Success,
    Failed,
    Canceled,
    Skipped,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ownership {
    Mine,
    Other,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub provider: Provider,
    #[serde(default)]
    pub connection_id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Mine,
    All,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DisplayScopes {
    pub providers: HashMap<Provider, Scope>,
    pub targets: HashMap<String, Scope>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub display_scopes: DisplayScopes,
    #[serde(default = "default_history_hours")]
    pub history_hours: u32,
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default = "default_accounts")]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub panel: crate::panel::PanelSettings,
    #[serde(default)]
    pub preferences: crate::preferences::Preferences,
    #[serde(default)]
    pub hidden_activities: Vec<ActivityKey>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityKey {
    pub provider: Provider,
    pub target_id: String,
    pub activity_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelSettingsPatch {
    pub show_active: Option<bool>,
    pub show_completed: Option<bool>,
    pub row_limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRuleChange {
    pub target_key: String,
    pub enabled: Option<bool>,
    pub scope: Option<Scope>,
    pub statuses: Option<Vec<ActivityStatus>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SettingsChange {
    Scope {
        value: Scope,
    },
    DisplayProviderScope {
        provider: Provider,
        scope: Option<Scope>,
    },
    DisplayTargetScope {
        target_key: String,
        scope: Option<Scope>,
    },
    HistoryHours {
        value: u32,
    },
    Language {
        value: crate::preferences::Language,
    },
    AutomaticUpdates {
        value: bool,
    },
    MenuBarAnimation {
        value: bool,
    },
    Panel {
        patch: PanelSettingsPatch,
    },
    PanelProvider {
        provider: Provider,
        enabled: bool,
    },
    PanelTarget {
        target_key: String,
        visible: bool,
    },
    NotificationEnabled {
        enabled: bool,
    },
    NotificationRules {
        rules: Vec<NotificationRuleChange>,
    },
    Target {
        target: Target,
        selected: bool,
    },
    Targets {
        targets: Vec<Target>,
        selected: bool,
    },
    AccountAlias {
        connection_id: String,
        alias: Option<String>,
    },
    ActivityVisibility {
        activity: ActivityKey,
        hidden: bool,
    },
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scope: Scope::Mine,
            display_scopes: DisplayScopes::default(),
            history_hours: default_history_hours(),
            targets: Vec::new(),
            accounts: default_accounts(),
            panel: crate::panel::PanelSettings::default(),
            preferences: crate::preferences::Preferences::default(),
            hidden_activities: Vec::new(),
        }
    }
}

fn default_accounts() -> Vec<Account> {
    vec![
        Account {
            id: LOCAL_GITHUB_ID.into(),
            provider: Provider::Github,
            label: "GitHub (local)".into(),
            mode: AccountMode::Local,
            profile: None,
            cli_username: None,
        },
        Account {
            id: LOCAL_VERCEL_ID.into(),
            provider: Provider::Vercel,
            label: "Vercel (local)".into(),
            mode: AccountMode::Local,
            profile: None,
            cli_username: None,
        },
    ]
}

fn default_history_hours() -> u32 {
    24
}

impl Settings {
    pub fn prune_target_references(&mut self) -> bool {
        let target_keys: HashSet<_> = self.targets.iter().map(Target::key).collect();
        let rules_before = self.preferences.notifications.rules.len();
        self.preferences
            .notifications
            .rules
            .retain(|rule| target_keys.contains(&rule.target_key));
        let pins_before = self.panel.pinned_targets.len();
        self.panel
            .pinned_targets
            .retain(|key| target_keys.contains(key));
        let hidden_before = self.panel.hidden_targets.len();
        self.panel
            .hidden_targets
            .retain(|key| target_keys.contains(key));
        let display_scopes_before = self.display_scopes.targets.len();
        self.display_scopes
            .targets
            .retain(|key, _| target_keys.contains(key));
        let aliases_before = self.preferences.account_aliases.len();
        let account_ids: HashSet<_> = self
            .accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        self.preferences
            .account_aliases
            .retain(|connection_id, _| account_ids.contains(connection_id.as_str()));
        rules_before != self.preferences.notifications.rules.len()
            || pins_before != self.panel.pinned_targets.len()
            || hidden_before != self.panel.hidden_targets.len()
            || display_scopes_before != self.display_scopes.targets.len()
            || aliases_before != self.preferences.account_aliases.len()
    }

    pub fn migrate_legacy(&mut self) -> bool {
        let mut changed = false;
        for required in default_accounts() {
            if !self
                .accounts
                .iter()
                .any(|account| account.id == required.id)
            {
                self.accounts.push(required);
                changed = true;
            }
        }

        let mut aws_profiles = HashMap::<String, String>::new();
        for account in &self.accounts {
            if account.provider == Provider::Aws {
                if let Some(profile) = &account.profile {
                    aws_profiles.insert(profile.clone(), account.id.clone());
                }
            }
        }
        for target in &mut self.targets {
            if !target.connection_id.is_empty() {
                continue;
            }
            target.connection_id = match target.provider {
                Provider::Github => LOCAL_GITHUB_ID.into(),
                Provider::Vercel => LOCAL_VERCEL_ID.into(),
                Provider::Aws => {
                    let Some(profile) = target.metadata.get("profile").cloned() else {
                        continue;
                    };
                    aws_profiles
                        .entry(profile.clone())
                        .or_insert_with(|| {
                            let id = format!("aws-{}", uuid::Uuid::new_v4());
                            self.accounts.push(Account {
                                id: id.clone(),
                                provider: Provider::Aws,
                                label: profile.clone(),
                                mode: AccountMode::Local,
                                profile: Some(profile),
                                cli_username: None,
                            });
                            id
                        })
                        .clone()
                }
                provider => format!("{}:local", provider.as_str()),
            };
            changed = true;
        }
        changed |= self.prune_target_references();
        changed
    }

    pub fn validate(&self) -> Result<(), String> {
        self.panel.validate()?;
        self.preferences.validate()?;
        if !matches!(self.history_hours, 24 | 168) {
            return Err("History must be either 24 or 168 hours.".into());
        }
        if self.targets.len() > MAX_TARGETS {
            return Err(format!("At most {MAX_TARGETS} targets can be selected."));
        }
        if self.hidden_activities.len() > MAX_HIDDEN_ACTIVITIES {
            return Err(format!(
                "At most {MAX_HIDDEN_ACTIVITIES} activities can be hidden."
            ));
        }
        let mut hidden = HashSet::new();
        for activity in &self.hidden_activities {
            activity.validate()?;
            if !hidden.insert(activity) {
                return Err("Each hidden activity must be unique.".into());
            }
        }
        if self.accounts.len() > MAX_ACCOUNTS {
            return Err(format!(
                "At most {MAX_ACCOUNTS} accounts can be configured."
            ));
        }
        let mut accounts = HashMap::new();
        for account in &self.accounts {
            account.validate()?;
            if accounts.insert(account.id.as_str(), account).is_some() {
                return Err("Each account must be unique.".into());
            }
        }
        for required in [LOCAL_GITHUB_ID, LOCAL_VERCEL_ID] {
            if !accounts.contains_key(required) {
                return Err("The default local CLI accounts cannot be removed.".into());
            }
        }
        let mut ids = HashSet::new();
        for target in &self.targets {
            target.validate()?;
            let account = accounts
                .get(target.connection_id.as_str())
                .ok_or_else(|| "A selected target references an unknown account.".to_owned())?;
            if account.provider != target.provider {
                return Err("A selected target references an account for another provider.".into());
            }
            if target.provider == Provider::Aws
                && target.metadata.get("profile") != account.profile.as_ref()
            {
                return Err("An AWS target does not belong to its selected profile.".into());
            }
            if !ids.insert((
                target.connection_id.as_str(),
                target.provider,
                target.id.as_str(),
            )) {
                return Err("Each selected target must be unique.".into());
            }
        }
        let target_keys: HashSet<_> = self.targets.iter().map(Target::key).collect();
        if self
            .display_scopes
            .targets
            .iter()
            .any(|(key, _)| key.trim().is_empty() || key.len() > 1600 || !target_keys.contains(key))
        {
            return Err("A display scope references an unknown target.".into());
        }
        if self
            .preferences
            .notifications
            .rules
            .iter()
            .any(|rule| !target_keys.contains(&rule.target_key))
        {
            return Err("A notification rule references an unknown target.".into());
        }
        if self
            .preferences
            .account_aliases
            .keys()
            .any(|connection_id| !accounts.contains_key(connection_id.as_str()))
        {
            return Err("An account alias references an unknown account.".into());
        }
        Ok(())
    }

    pub fn effective_display_scope(
        &self,
        provider: Provider,
        connection_id: Option<&str>,
        target_id: Option<&str>,
    ) -> Scope {
        let target_scope = connection_id
            .zip(target_id)
            .and_then(|(connection_id, target_id)| {
                self.display_scopes.targets.get(&format!(
                    "{}:{connection_id}:{target_id}",
                    provider.as_str()
                ))
            });
        target_scope
            .copied()
            .or_else(|| self.display_scopes.providers.get(&provider).copied())
            .unwrap_or(self.scope)
    }
}

impl ActivityKey {
    pub fn from_activity(activity: &Activity) -> Self {
        Self {
            provider: activity.provider,
            target_id: activity.target_id.clone(),
            activity_id: activity.id.clone(),
        }
    }

    pub fn matches(&self, activity: &Activity) -> bool {
        self.provider == activity.provider
            && self.target_id == activity.target_id
            && self.activity_id == activity.id
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_text("Hidden activity target ID", &self.target_id)?;
        validate_text("Hidden activity ID", &self.activity_id)
    }
}

impl Account {
    pub fn validate(&self) -> Result<(), String> {
        validate_text("Account ID", &self.id)?;
        validate_text("Account label", &self.label)?;
        if !self
            .id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, ':' | '-'))
        {
            return Err("Account IDs contain unsupported characters.".into());
        }
        if let Some(username) = self.cli_username.as_deref() {
            validate_github_username(username)?;
        }
        match self.provider {
            Provider::Github
            | Provider::GithubDeployments
            | Provider::Vercel
            | Provider::Bitbucket => {
                if !matches!(
                    self.provider,
                    Provider::Github | Provider::GithubDeployments
                ) && self.cli_username.is_some()
                {
                    return Err("Only GitHub accounts can select a GitHub CLI user.".into());
                }
                if self.provider != Provider::Bitbucket {
                    if let Some(identity) = self.profile.as_deref() {
                        validate_text("Account identity", identity)?;
                    }
                }
                if self.provider == Provider::Bitbucket {
                    if self.mode != AccountMode::Isolated
                        || !self.id.starts_with("bitbucket:")
                        || self.id["bitbucket:".len()..].parse::<uuid::Uuid>().is_err()
                        || self.profile.as_deref().is_none_or(|value| {
                            value.split_once('|').is_none_or(|(uuid, account_id)| {
                                uuid.trim().is_empty() || account_id.trim().is_empty()
                            })
                        })
                    {
                        return Err("Bitbucket accounts must use an isolated connection ID.".into());
                    }
                    return Ok(());
                }
                if self.provider == Provider::GithubDeployments {
                    let legacy_local =
                        self.mode == AccountMode::Local && self.id == "github-deployments:local";
                    let isolated = self.mode == AccountMode::Isolated
                        && self.id.starts_with("github-deployments-");
                    if !legacy_local && !isolated {
                        return Err(
                            "GitHub Deployments accounts must use an isolated connection ID."
                                .into(),
                        );
                    }
                    return Ok(());
                }
                let expected_local = match self.provider {
                    Provider::Github => LOCAL_GITHUB_ID,
                    Provider::Vercel => LOCAL_VERCEL_ID,
                    _ => unreachable!(),
                };
                if (self.mode == AccountMode::Local) != (self.id == expected_local) {
                    return Err(
                        "Only the built-in GitHub and Vercel accounts can use local CLI mode."
                            .into(),
                    );
                }
            }
            Provider::CloudflarePages | Provider::CloudflareWorkers => {
                let legacy = self.mode == AccountMode::Local
                    && self.id == format!("{}:local", self.provider.as_str())
                    && self.profile.is_none();
                let prefix = format!("{}:", self.provider.as_str());
                let valid_api = self.mode == AccountMode::Isolated
                    && self
                        .id
                        .strip_prefix(&prefix)
                        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                    && self.profile.as_deref().is_some_and(|id| {
                        id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
                    });
                if self.cli_username.is_some() || !(legacy || valid_api) {
                    return Err("The Cloudflare connection is invalid.".into());
                }
            }
            Provider::Firebase | Provider::Eas => {
                if self.mode != AccountMode::Local
                    || self.id != format!("{}:local", self.provider.as_str())
                    || self.cli_username.is_some()
                {
                    return Err("This provider reuses one verified local CLI session.".into());
                }
                if let Some(identity) = self.profile.as_deref() {
                    validate_text("CLI account identity", identity)?;
                }
            }
            Provider::Aws => {
                if self.cli_username.is_some() {
                    return Err("AWS accounts cannot select a GitHub CLI user.".into());
                }
                if self.mode != AccountMode::Local {
                    return Err("AWS profile accounts must use local mode.".into());
                }
                validate_text(
                    "AWS profile",
                    self.profile
                        .as_deref()
                        .ok_or_else(|| "AWS accounts require a profile.".to_owned())?,
                )?;
            }
            _ => {
                if self.mode != AccountMode::Local
                    || self.profile.is_some()
                    || self.cli_username.is_some()
                    || self.id != format!("{}:local", self.provider.as_str())
                {
                    return Err("This provider reuses one existing local CLI session.".into());
                }
            }
        }
        if self.id == LOCAL_GITHUB_ID
            && (self.provider != Provider::Github || self.mode != AccountMode::Local)
            || self.id == LOCAL_VERCEL_ID
                && (self.provider != Provider::Vercel || self.mode != AccountMode::Local)
        {
            return Err("The default local account is invalid.".into());
        }
        Ok(())
    }
}

fn validate_github_username(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 39
        || bytes.first() == Some(&b'-')
        || bytes.last() == Some(&b'-')
        || value.contains("--")
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        return Err("The GitHub CLI user name is invalid.".into());
    }
    Ok(())
}

impl Target {
    pub fn validate(&self) -> Result<(), String> {
        validate_text("Target ID", &self.id)?;
        validate_text("Target name", &self.name)?;
        if self.id.chars().any(char::is_whitespace) {
            return Err("Target IDs cannot contain whitespace.".into());
        }
        if self
            .description
            .as_ref()
            .is_some_and(|value| value.len() > MAX_TEXT_LEN * 4)
        {
            return Err("Target descriptions are too long.".into());
        }
        if self.metadata.len() > 32
            || self
                .metadata
                .iter()
                .any(|(key, value)| key.is_empty() || key.len() > 128 || value.len() > MAX_TEXT_LEN)
        {
            return Err("Target metadata is invalid or too large.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod settings_change_tests {
    use super::*;

    #[test]
    fn settings_change_uses_camel_case_and_rejects_unknown_fields() {
        let change: SettingsChange = serde_json::from_str(
            r#"{"kind":"accountAlias","connectionId":"github:local","alias":"Personal"}"#,
        )
        .unwrap();
        assert!(matches!(change, SettingsChange::AccountAlias { .. }));
        assert!(serde_json::from_str::<SettingsChange>(
            r#"{"kind":"scope","value":"mine","unexpected":true}"#
        )
        .is_err());
        let activity: SettingsChange = serde_json::from_str(
            r#"{"kind":"activityVisibility","activity":{"provider":"github","targetId":"example/repo","activityId":"run-1"},"hidden":true}"#,
        )
        .unwrap();
        assert!(matches!(
            activity,
            SettingsChange::ActivityVisibility { hidden: true, .. }
        ));
        assert!(serde_json::from_str::<SettingsChange>(
            r#"{"kind":"activityVisibility","activity":{"provider":"github","targetId":"example/repo","activityId":"run-1","connectionId":"forged"},"hidden":true}"#
        )
        .is_err());
        let clear_scope: SettingsChange = serde_json::from_str(
            r#"{"kind":"displayTargetScope","targetKey":"github:github:local:example/repo","scope":null}"#,
        )
        .unwrap();
        assert!(matches!(
            clear_scope,
            SettingsChange::DisplayTargetScope { scope: None, .. }
        ));
        let animation: SettingsChange =
            serde_json::from_str(r#"{"kind":"menuBarAnimation","value":false}"#).unwrap();
        assert_eq!(animation, SettingsChange::MenuBarAnimation { value: false });
    }
}

#[cfg(test)]
mod account_tests {
    use super::*;

    /// Unbound GitHub connections still load so existing settings are never
    /// locked; they report NeedsAccount at query time instead of using the
    /// active gh account (see `cli::tests`).
    #[test]
    fn legacy_github_account_without_cli_username_still_loads_for_explicit_rebind() {
        let account: Account = serde_json::from_str(
            r#"{"id":"github-00000000-0000-0000-0000-000000000000","provider":"github","label":"Legacy","mode":"isolated","profile":"42"}"#,
        )
        .expect("legacy account");
        assert_eq!(account.cli_username, None);
        account.validate().expect("legacy account remains valid");
    }

    #[test]
    fn local_github_slot_loads_unbound_and_accepts_a_selected_user() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"targets":[{"id":"example-org/app","provider":"github","connectionId":"github:local","name":"app","description":null,"metadata":{}}],"accounts":[{"id":"github:local","provider":"github","label":"GitHub (local)","mode":"local","profile":null}]}"#,
        )
        .expect("legacy settings");
        settings.migrate_legacy();
        settings.validate().expect("unbound local slot stays valid");
        let local = settings
            .accounts
            .iter_mut()
            .find(|account| account.id == LOCAL_GITHUB_ID)
            .unwrap();
        local.cli_username = Some("example-user".into());
        local.profile = Some("42".into());
        settings
            .validate()
            .expect("the local slot keeps its ID and mode when bound");
        let deployments = Account {
            id: "github-deployments:local".into(),
            provider: Provider::GithubDeployments,
            label: "Deployments".into(),
            mode: AccountMode::Local,
            profile: None,
            cli_username: Some("example-user".into()),
        };
        deployments
            .validate()
            .expect("legacy deployments slot may bind");
    }

    #[test]
    fn issue_and_connection_kinds_default_for_older_payloads() {
        let issue: Issue = serde_json::from_str(
            r#"{"provider":"github","connectionId":"github:local","message":"m","targetId":null}"#,
        )
        .unwrap();
        assert_eq!(issue.kind, IssueKind::Other);
        let connection: Connection = serde_json::from_str(
            r#"{"connectionId":"github:local","provider":"github","status":"signed-out","account":null,"message":null,"cliVersion":null}"#,
        )
        .unwrap();
        assert_eq!(connection.kind, None);
        assert_eq!(connection.normalized().kind, Some(IssueKind::SignedOut));
        assert_eq!(
            serde_json::to_value(IssueKind::NeedsAccount).unwrap(),
            "needsAccount"
        );
    }

    #[test]
    fn github_cli_username_is_provider_scoped_and_strictly_validated() {
        let valid = Account {
            id: "github-00000000-0000-0000-0000-000000000000".into(),
            provider: Provider::Github,
            label: "Example".into(),
            mode: AccountMode::Isolated,
            profile: Some("42".into()),
            cli_username: Some("example-user".into()),
        };
        valid.validate().expect("valid GitHub account");
        for username in ["", "-example", "example-", "example--user", "example_user"] {
            assert!(Account {
                cli_username: Some(username.into()),
                ..valid.clone()
            }
            .validate()
            .is_err());
        }
        assert!(Account {
            id: "vercel-00000000-0000-0000-0000-000000000000".into(),
            provider: Provider::Vercel,
            cli_username: Some("example-user".into()),
            ..valid
        }
        .validate()
        .is_err());
    }
}

fn validate_text(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains('\0') {
        Err(format!("{label} is invalid."))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionStatus {
    Connected,
    SignedOut,
    MissingCli,
    Error,
    Unconfigured,
}

/// Why a connection or query needs attention. The UI translates this value;
/// `message` stays as an English fallback for older clients and logs.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    SignedOut,
    MissingCli,
    NoAccess,
    RateLimited,
    Network,
    TimedOut,
    NeedsAccount,
    AccountMismatch,
    UnsupportedVersion,
    #[default]
    Other,
}

impl IssueKind {
    pub fn for_status(status: ConnectionStatus) -> Option<Self> {
        match status {
            ConnectionStatus::Connected => None,
            ConnectionStatus::SignedOut => Some(Self::SignedOut),
            ConnectionStatus::MissingCli => Some(Self::MissingCli),
            ConnectionStatus::Error | ConnectionStatus::Unconfigured => Some(Self::Other),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub connection_id: String,
    pub provider: Provider,
    pub status: ConnectionStatus,
    pub account: Option<String>,
    pub message: Option<String>,
    pub cli_version: Option<String>,
    #[serde(default)]
    pub kind: Option<IssueKind>,
}

impl Connection {
    pub fn disconnected(
        connection_id: impl Into<String>,
        provider: Provider,
        status: ConnectionStatus,
        message: impl Into<String>,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            provider,
            status,
            account: None,
            message: Some(message.into()),
            cli_version: None,
            kind: IssueKind::for_status(status),
        }
    }

    pub fn with_kind(self, kind: IssueKind) -> Self {
        Self {
            kind: Some(kind),
            ..self
        }
    }

    /// Fills in a kind from the status for providers that do not classify their
    /// failures, and clears it for connected results.
    pub fn normalized(self) -> Self {
        let kind = match self.status {
            ConnectionStatus::Connected => None,
            status => self.kind.or_else(|| IssueKind::for_status(status)),
        };
        Self { kind, ..self }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    pub provider: Provider,
    pub connection_id: String,
    pub target_id: String,
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
    pub project: String,
    pub branch: Option<String>,
    pub url: String,
    pub status: ActivityStatus,
    pub raw_status: String,
    pub actor: Option<String>,
    pub ownership: Ownership,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub provider: Provider,
    pub connection_id: Option<String>,
    pub message: String,
    pub target_id: Option<String>,
    #[serde(default)]
    pub kind: IssueKind,
}

impl Issue {
    pub fn same_cause(&self, other: &Self) -> bool {
        self.kind != IssueKind::Other
            && self.kind == other.kind
            && self.provider == other.provider
            && self.connection_id == other.connection_id
            && self.target_id == other.target_id
    }
}

/// A failed target query with a translatable cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryError {
    pub kind: IssueKind,
    pub message: String,
}

impl QueryError {
    pub fn new(kind: IssueKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl From<String> for QueryError {
    fn from(message: String) -> Self {
        Self::new(IssueKind::Other, message)
    }
}

impl From<&str> for QueryError {
    fn from(message: &str) -> Self {
        Self::new(IssueKind::Other, message)
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryHealthStatus {
    Checking,
    Success,
    Partial,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHealth {
    pub provider: Provider,
    pub connection_id: String,
    pub target_id: String,
    pub status: QueryHealthStatus,
    pub last_attempt_at: String,
    pub last_successful_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_error: Option<String>,
    pub connections: Vec<Connection>,
    pub activities: Vec<Activity>,
    pub issues: Vec<Issue>,
    pub refreshing: bool,
    pub last_updated_at: Option<String>,
    #[serde(default = "default_poll_interval_seconds")]
    pub poll_interval_seconds: u64,
    #[serde(default)]
    pub query_health: Vec<QueryHealth>,
    #[serde(default)]
    pub history: Vec<TargetHistory>,
    #[serde(default)]
    pub pending_targets: Vec<String>,
}

impl Snapshot {
    pub fn initial(settings: Settings) -> Self {
        Self {
            settings,
            settings_error: None,
            connections: Vec::new(),
            activities: Vec::new(),
            issues: Vec::new(),
            refreshing: false,
            last_updated_at: None,
            poll_interval_seconds: default_poll_interval_seconds(),
            query_health: Vec::new(),
            history: Vec::new(),
            pending_targets: Vec::new(),
        }
    }
}

fn default_poll_interval_seconds() -> u64 {
    120
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub connection: Connection,
    pub targets: Vec<Target>,
    pub aws_profiles: Vec<String>,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginState {
    Waiting,
    Complete,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStatus {
    pub connection_id: String,
    pub provider: Provider,
    pub state: LoginState,
    pub message: String,
    pub url: Option<String>,
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_reject_invalid_history_and_duplicate_targets() {
        let target = Target {
            id: "repo".into(),
            provider: Provider::Github,
            connection_id: LOCAL_GITHUB_ID.into(),
            name: "Repository".into(),
            description: None,
            metadata: HashMap::new(),
        };
        assert!(Settings {
            history_hours: 48,
            ..Settings::default()
        }
        .validate()
        .is_err());
        assert!(Settings {
            targets: vec![target.clone(), target],
            ..Settings::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn pruning_removed_targets_cleans_notification_rules_and_pins() {
        let mut settings = Settings::default();
        settings.panel.pinned_targets = vec!["github:github:local:org/removed".into()];
        settings.panel.hidden_targets = vec!["github:github:local:org/removed".into()];
        settings
            .display_scopes
            .targets
            .insert("github:github:local:org/removed".into(), Scope::All);
        settings.preferences.notifications.rules = vec![crate::preferences::NotificationRule {
            target_key: "github:github:local:org/removed".into(),
            scope: crate::preferences::NotificationScope::Mine,
            statuses: vec![ActivityStatus::Failed],
            enabled: true,
        }];
        assert!(settings.prune_target_references());
        assert!(settings.panel.pinned_targets.is_empty());
        assert!(settings.panel.hidden_targets.is_empty());
        assert!(settings.display_scopes.targets.is_empty());
        assert!(settings.preferences.notifications.rules.is_empty());
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn effective_display_scope_prefers_target_then_provider_then_global() {
        let selected = Target {
            id: "org/app".into(),
            provider: Provider::Github,
            connection_id: LOCAL_GITHUB_ID.into(),
            name: "app".into(),
            description: None,
            metadata: HashMap::new(),
        };
        let mut settings = Settings {
            scope: Scope::Mine,
            targets: vec![selected.clone()],
            ..Settings::default()
        };
        settings
            .display_scopes
            .providers
            .insert(Provider::Github, Scope::All);
        settings
            .display_scopes
            .targets
            .insert(selected.key(), Scope::Mine);

        assert_eq!(
            settings.effective_display_scope(
                Provider::Github,
                Some(LOCAL_GITHUB_ID),
                Some("org/app")
            ),
            Scope::Mine
        );
        assert_eq!(
            settings.effective_display_scope(Provider::Github, None, None),
            Scope::All
        );
        assert_eq!(
            settings.effective_display_scope(Provider::Vercel, None, None),
            Scope::Mine
        );
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    pub activities: Vec<Activity>,
    pub next_cursor: Option<String>,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetHistory {
    pub connection_id: String,
    pub target_id: String,
    pub provider: Provider,
    pub next_cursor: Option<String>,
    pub checked_at: String,
    pub generation: u64,
}

impl Target {
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.provider.as_str(),
            self.connection_id,
            self.id
        )
    }
}
