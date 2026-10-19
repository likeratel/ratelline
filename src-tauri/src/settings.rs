use std::{
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use atomicwrites::{AllowOverwrite, AtomicFile};

use crate::models::Settings;

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
    load_error: Arc<OnceLock<String>>,
}

impl SettingsStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            path: app_data_dir.join("settings.json"),
            load_error: Arc::new(OnceLock::new()),
        }
    }

    pub async fn load(&self) -> Result<Settings, String> {
        self.ensure_writable()?;
        let store = self.clone();
        let result = tokio::task::spawn_blocking(move || store.load_from_disk(write_settings))
            .await
            .map_err(|_| "Settings loading was interrupted.".to_owned())
            .and_then(|result| result);
        result.map_err(|message| self.block_saving(message))
    }

    pub fn load_error(&self) -> Option<String> {
        self.load_error.get().cloned()
    }

    pub fn ensure_writable(&self) -> Result<(), String> {
        match self.load_error() {
            Some(message) => Err(message),
            None => Ok(()),
        }
    }

    fn block_saving(&self, message: String) -> String {
        self.load_error
            .get_or_init(|| format!("{message} Settings changes are locked to preserve the original file. Restore a valid settings.json or its folder permissions, then restart ratelline."))
            .clone()
    }

    fn load_from_disk(
        &self,
        persist: impl FnOnce(&Path, &Settings) -> Result<(), String>,
    ) -> Result<Settings, String> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(_) => return Err("Saved settings could not be read.".into()),
        };
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Saved settings are not valid JSON.".to_owned())?;
        let legacy_display_scopes = value
            .as_object()
            .is_some_and(|object| !object.contains_key("displayScopes"));
        let mut settings: Settings = serde_json::from_value(value)
            .map_err(|_| "Saved settings have an unsupported format.".to_owned())?;
        let migrated = legacy_display_scopes | settings.migrate_legacy();
        settings.validate()?;
        if migrated {
            if let Err(message) = persist(&self.path, &settings) {
                // Keep the readable settings available, but never replace them with defaults.
                self.block_saving(message);
            }
        }
        Ok(settings)
    }

    pub async fn save(&self, settings: Settings) -> Result<Settings, String> {
        self.ensure_writable()?;
        settings.validate()?;
        let path = self.path.clone();
        let saved = settings.clone();
        tokio::task::spawn_blocking(move || write_settings(&path, &saved))
            .await
            .map_err(|_| "Settings saving was interrupted.".to_owned())??;
        Ok(settings)
    }
}

fn write_settings(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Settings path is invalid.".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|_| "The app data directory could not be created.".to_owned())?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|_| "Settings could not be serialized.".to_owned())?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .map_err(|_| "Settings could not be saved atomically.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ratelline-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[tokio::test]
    async fn atomic_round_trip_preserves_settings() {
        let directory = test_directory("settings-round-trip");
        let store = SettingsStore::new(directory.clone());
        let settings = Settings {
            history_hours: 168,
            panel: crate::panel::PanelSettings {
                row_limit: 3,
                show_completed: false,
                ..crate::panel::PanelSettings::default()
            },
            ..Settings::default()
        };
        store.save(settings.clone()).await.expect("save settings");
        assert_eq!(store.load().await.expect("load settings"), settings);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn load_migrates_and_persists_legacy_targets() {
        let directory = test_directory("settings-migration");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let path = directory.join("settings.json");
        std::fs::write(
            &path,
            br#"{
  "scope": "mine",
  "historyHours": 24,
  "targets": [
    {"id":"example/repo","provider":"github","name":"Repository","description":null,"metadata":{}},
    {"id":"stack-id","provider":"aws","name":"Stack","description":null,"metadata":{"profile":"engineering","region":"us-east-1"}}
  ]
}"#,
        )
        .expect("write legacy settings");

        let store = SettingsStore::new(directory.clone());
        let migrated = store.load().await.expect("migrate settings");
        assert_eq!(migrated.targets.len(), 2);
        assert_eq!(
            migrated.targets[0].connection_id,
            crate::models::LOCAL_GITHUB_ID
        );
        let aws_connection_id = migrated.targets[1].connection_id.clone();
        assert!(migrated.accounts.iter().any(|account| {
            account.id == aws_connection_id
                && account.provider == crate::models::Provider::Aws
                && account.profile.as_deref() == Some("engineering")
        }));

        let persisted = store.load().await.expect("reload migrated settings");
        assert_eq!(persisted, migrated);
        let persisted_json = std::fs::read_to_string(path).expect("read migrated settings");
        assert!(persisted_json.contains("connectionId"));
        assert!(persisted_json.contains("accounts"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn legacy_settings_default_to_no_hidden_activities() {
        let directory = test_directory("hidden-activity-default");
        std::fs::create_dir_all(&directory).expect("create test directory");
        std::fs::write(
            directory.join("settings.json"),
            br#"{"scope":"mine","historyHours":24,"targets":[]}"#,
        )
        .expect("write legacy settings");

        let settings = SettingsStore::new(directory.clone())
            .load()
            .await
            .expect("load legacy settings");

        assert!(settings.hidden_activities.is_empty());
        assert_eq!(
            settings.display_scopes,
            crate::models::DisplayScopes::default()
        );
        let persisted = std::fs::read_to_string(directory.join("settings.json"))
            .expect("read migrated settings");
        assert!(persisted.contains("displayScopes"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn corrupt_settings_cannot_be_overwritten_even_after_external_repair() {
        let directory = test_directory("settings-corrupt");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        let original = b"{incomplete settings";
        std::fs::write(&path, original).unwrap();
        let store = SettingsStore::new(directory.clone());
        assert!(store.load().await.is_err());
        assert!(store.load_error().unwrap().contains("restart ratelline"));
        assert!(store.clone().save(Settings::default()).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);

        let restored = Settings {
            history_hours: 168,
            ..Settings::default()
        };
        write_settings(&path, &restored).unwrap();
        assert!(store.save(Settings::default()).await.is_err());
        let restarted = SettingsStore::new(directory.clone());
        assert_eq!(restarted.load().await.unwrap(), restored);
        assert!(restarted.save(restored).await.is_ok());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn invalid_settings_are_preserved_after_validation_failure() {
        let directory = test_directory("settings-invalid");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        let original = br#"{"historyHours":0}"#;
        std::fs::write(&path, original).unwrap();
        let store = SettingsStore::new(directory.clone());
        assert!(store.load().await.is_err());
        assert!(store.save(Settings::default()).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn unreadable_path_stays_locked_after_it_becomes_readable() {
        let directory = test_directory("settings-unreadable");
        let path = directory.join("settings.json");
        std::fs::create_dir_all(&path).unwrap();
        let store = SettingsStore::new(directory.clone());
        assert!(store.load().await.is_err());
        std::fs::remove_dir(&path).unwrap();
        let restored = Settings {
            history_hours: 168,
            ..Settings::default()
        };
        write_settings(&path, &restored).unwrap();
        assert!(store.save(Settings::default()).await.is_err());
        assert_eq!(
            SettingsStore::new(directory.clone()).load().await.unwrap(),
            restored
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn failed_migration_write_keeps_loaded_targets_and_locks_future_saves() {
        let directory = test_directory("settings-migration-failed");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        let original = br#"{"scope":"mine","historyHours":168,"targets":[{"id":"example/repo","provider":"github","name":"Repository","description":null,"metadata":{}}]}"#;
        std::fs::write(&path, original).unwrap();
        let store = SettingsStore::new(directory.clone());
        let loaded = store
            .load_from_disk(|_, _| Err("Settings could not be saved atomically.".into()))
            .unwrap();
        assert_eq!(loaded.history_hours, 168);
        assert_eq!(loaded.targets[0].id, "example/repo");
        assert_eq!(
            loaded.targets[0].connection_id,
            crate::models::LOCAL_GITHUB_ID
        );
        assert!(store.load_error().is_some());
        assert!(store.save(Settings::default()).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
