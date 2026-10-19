use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const MAX_NOTIFICATION_RULES: usize = 500;
const MAX_TARGET_KEY_LEN: usize = 1600;
const MAX_ACCOUNT_ALIAS_LEN: usize = 80;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Language {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationScope {
    Mine,
    All,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRule {
    pub target_key: String,
    pub scope: NotificationScope,
    #[serde(default)]
    pub statuses: Vec<crate::models::ActivityStatus>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationPreferences {
    pub enabled: bool,
    pub rules: Vec<NotificationRule>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub language: Language,
    pub notifications: NotificationPreferences,
    pub check_updates_automatically: bool,
    /// Walks the macOS menu-bar ratel while runs are in progress.
    pub animate_menu_bar_icon: bool,
    pub account_aliases: HashMap<String, String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::System,
            notifications: NotificationPreferences::default(),
            check_updates_automatically: true,
            animate_menu_bar_icon: true,
            account_aliases: HashMap::new(),
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.notifications.rules.len() > MAX_NOTIFICATION_RULES {
            return Err(format!(
                "At most {MAX_NOTIFICATION_RULES} notification rules can be configured."
            ));
        }
        let mut target_keys = std::collections::HashSet::new();
        for rule in &self.notifications.rules {
            if rule.target_key.trim().is_empty()
                || rule.target_key.len() > MAX_TARGET_KEY_LEN
                || rule.statuses.len() > 3
                || rule.statuses.iter().any(|status| {
                    !matches!(
                        status,
                        crate::models::ActivityStatus::Success
                            | crate::models::ActivityStatus::Failed
                            | crate::models::ActivityStatus::Canceled
                    )
                })
            {
                return Err("Notification settings are invalid.".into());
            }
            if !target_keys.insert(rule.target_key.as_str()) {
                return Err("Each target can have only one notification rule.".into());
            }
            let mut unique = std::collections::HashSet::new();
            if rule.statuses.iter().any(|status| !unique.insert(*status)) {
                return Err("Notification statuses must be unique within a rule.".into());
            }
        }
        if self.account_aliases.iter().any(|(connection_id, alias)| {
            connection_id.trim().is_empty()
                || connection_id.len() > MAX_TARGET_KEY_LEN
                || alias.trim().is_empty()
                || alias.chars().count() > MAX_ACCOUNT_ALIAS_LEN
        }) {
            return Err("Account aliases are invalid.".into());
        }
        Ok(())
    }
}

pub fn resolved_language(language: &Language) -> Language {
    match language {
        Language::System => system_language(sys_locale::get_locale().as_deref()),
        language => *language,
    }
}

fn system_language(locale: Option<&str>) -> Language {
    let locale = locale.unwrap_or("en").to_ascii_lowercase();
    if locale.starts_with("ko") {
        Language::Korean
    } else if locale.starts_with("ja") {
        Language::Japanese
    } else if locale.starts_with("zh") {
        Language::SimplifiedChinese
    } else {
        Language::English
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_preferences_use_contract_defaults() {
        let preferences: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(preferences, Preferences::default());
        assert!(preferences.check_updates_automatically);
        assert!(preferences.animate_menu_bar_icon);
        assert!(preferences.account_aliases.is_empty());
    }

    #[test]
    fn language_serialization_matches_frontend_contract() {
        for (language, expected) in [
            (Language::System, "\"system\""),
            (Language::Korean, "\"ko\""),
            (Language::English, "\"en\""),
            (Language::Japanese, "\"ja\""),
            (Language::SimplifiedChinese, "\"zh-CN\""),
        ] {
            assert_eq!(serde_json::to_string(&language).unwrap(), expected);
        }
    }

    #[test]
    fn system_locale_resolution_has_a_safe_english_fallback() {
        assert_eq!(system_language(Some("ko-KR")), Language::Korean);
        assert_eq!(system_language(Some("ja_JP")), Language::Japanese);
        assert_eq!(
            system_language(Some("zh-Hant-TW")),
            Language::SimplifiedChinese
        );
        assert_eq!(system_language(None), Language::English);
    }

    #[test]
    fn duplicate_target_rules_are_rejected_but_empty_statuses_are_valid() {
        let empty_rule = NotificationRule {
            target_key: "github:github:local:example/repo".into(),
            scope: NotificationScope::Mine,
            statuses: Vec::new(),
            enabled: false,
        };
        let preferences = Preferences {
            notifications: NotificationPreferences {
                enabled: true,
                rules: vec![empty_rule.clone()],
            },
            ..Preferences::default()
        };
        assert!(preferences.validate().is_ok());
        assert!(Preferences {
            notifications: NotificationPreferences {
                enabled: true,
                rules: vec![
                    empty_rule.clone(),
                    NotificationRule {
                        scope: NotificationScope::All,
                        ..empty_rule
                    },
                ],
            },
            ..Preferences::default()
        }
        .validate()
        .is_err());
    }
}
