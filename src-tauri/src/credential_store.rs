//! Bitbucket and Cloudflare API credentials saved in `credentials.json` in the
//! app data directory.
//!
//! The file is plain JSON. On Unix it is created and kept with mode 0600, so
//! only the signed-in user (and processes running as that user) can read it.
//! A file that cannot be parsed is never replaced, so a damaged or newer file
//! keeps its contents until the user repairs it.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, OnceLock},
};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};

pub(crate) const FILE_NAME: &str = "credentials.json";
const FORMAT_VERSION: u64 = 1;
/// Upper bound for the whole file; each entry is separately capped by its owner.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Serializes every read-modify-write of the credentials file in this process.
static FILE_LOCK: Mutex<()> = Mutex::new(());
static APP_STORE: OnceLock<CredentialFile> = OnceLock::new();
/// Becomes `true` after the startup keychain move finishes, so no lookup can
/// report a missing credential that is still being moved.
static READY: LazyLock<tokio::sync::watch::Sender<bool>> =
    LazyLock::new(|| tokio::sync::watch::channel(false).0);

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Document {
    version: u64,
    entries: BTreeMap<String, String>,
    /// Keys whose keychain entry was already looked up by the one-time move,
    /// whatever the result, so the keychain is not read for them again.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    keychain_checked: BTreeSet<String>,
}

/// State of one key relevant to the one-time keychain move.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MoveState {
    pub(crate) stored: bool,
    pub(crate) keychain_checked: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CredentialFile {
    path: PathBuf,
}

impl CredentialFile {
    pub(crate) fn in_directory(app_data_dir: &Path) -> Self {
        Self {
            path: app_data_dir.join(FILE_NAME),
        }
    }

    /// Returns the saved value, or `None` when the file or the entry is missing.
    pub(crate) fn get(&self, key: &str) -> Result<Option<String>, String> {
        let _guard = lock()?;
        Ok(self.load()?.entries.remove(key))
    }

    pub(crate) fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let _guard = lock()?;
        let mut document = self.load()?;
        document.entries.insert(key.to_owned(), value.to_owned());
        self.persist(&document)
    }

    /// Removes the entry and its keychain-move record, returning the previous value.
    pub(crate) fn take(&self, key: &str) -> Result<Option<String>, String> {
        let _guard = lock()?;
        let mut document = self.load()?;
        let previous = document.entries.remove(key);
        let was_checked = document.keychain_checked.remove(key);
        if previous.is_some() || was_checked {
            self.persist(&document)?;
        }
        Ok(previous)
    }

    pub(crate) fn move_state(&self, key: &str) -> Result<MoveState, String> {
        let _guard = lock()?;
        let document = self.load()?;
        Ok(MoveState {
            stored: document.entries.contains_key(key),
            keychain_checked: document.keychain_checked.contains(key),
        })
    }

    /// Saves a value moved from the keychain and records the key as checked.
    pub(crate) fn set_moved(&self, key: &str, value: &str) -> Result<(), String> {
        let _guard = lock()?;
        let mut document = self.load()?;
        document.entries.insert(key.to_owned(), value.to_owned());
        document.keychain_checked.insert(key.to_owned());
        self.persist(&document)
    }

    /// Records that the keychain was looked up for the key.
    pub(crate) fn mark_keychain_checked(&self, key: &str) -> Result<(), String> {
        let _guard = lock()?;
        let mut document = self.load()?;
        if document.keychain_checked.insert(key.to_owned()) {
            self.persist(&document)?;
        }
        Ok(())
    }

    fn load(&self) -> Result<Document, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Document {
                    version: FORMAT_VERSION,
                    ..Document::default()
                })
            }
            Err(_) => return Err("Saved credentials could not be read.".into()),
        };
        restrict_permissions(&file);
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Saved credentials could not be read.".to_owned())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Saved credentials are too large.".into());
        }
        parse(&bytes)
    }

    fn persist(&self, document: &Document) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "Credential storage path is invalid.".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|_| "The app data directory could not be created.".to_owned())?;
        let bytes = serde_json::to_vec_pretty(document)
            .map_err(|_| "Credentials could not be serialized.".to_owned())?;
        AtomicFile::new(&self.path, AllowOverwrite)
            .write_with_options(
                |file| {
                    file.write_all(&bytes)?;
                    file.sync_all()
                },
                private_file_options(),
            )
            .map_err(|_| "Credentials could not be saved.".to_owned())
    }
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    FILE_LOCK
        .lock()
        .map_err(|_| "Credential storage is unavailable.".to_owned())
}

fn parse(bytes: &[u8]) -> Result<Document, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| "Saved credentials are not valid JSON.".to_owned())?;
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(FORMAT_VERSION) => {}
        Some(_) => {
            return Err(
                "Saved credentials were written by a newer or unknown version of ratelline.".into(),
            )
        }
        None => return Err("Saved credentials have an unsupported format.".into()),
    }
    serde_json::from_value(value)
        .map_err(|_| "Saved credentials have an unsupported format.".to_owned())
}

/// The temporary file is created with mode 0600 before any secret is written.
fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Narrows an existing file that group or other users can access to 0600.
#[cfg(unix)]
fn restrict_permissions(file: &File) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = file.metadata() else {
        return;
    };
    if metadata.permissions().mode() & 0o077 == 0 {
        return;
    }
    if file
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .is_err()
    {
        eprintln!("Failed to restrict the permissions of the saved credentials file.");
    }
}

#[cfg(not(unix))]
fn restrict_permissions(_file: &File) {}

/// Points the app's credential storage at `<app data>/credentials.json`.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn configure(app_data_dir: &Path) -> &'static CredentialFile {
    APP_STORE.get_or_init(|| CredentialFile::in_directory(app_data_dir))
}

/// Opens the gate for credential lookups after the startup keychain move.
pub(crate) fn mark_ready() {
    READY.send_replace(true);
}

async fn wait_until_ready() {
    let mut receiver = READY.subscribe();
    // The sender lives in a static and is never dropped, so this cannot fail.
    let _ = receiver.wait_for(|ready| *ready).await;
}

/// Waits for the startup keychain move, then returns the app's credential file.
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn app_store() -> Result<&'static CredentialFile, String> {
    wait_until_ready().await;
    APP_STORE
        .get()
        .ok_or_else(|| "Credential storage is unavailable.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, CredentialFile) {
        let directory = tempfile::tempdir().unwrap();
        let store = CredentialFile::in_directory(directory.path());
        (directory, store)
    }

    fn write_raw(store: &CredentialFile, bytes: &[u8]) {
        std::fs::write(&store.path, bytes).unwrap();
    }

    #[test]
    fn missing_file_has_no_entries() {
        let (_directory, store) = store();
        assert_eq!(store.get("bitbucket:fixture").unwrap(), None);
        assert_eq!(store.take("bitbucket:fixture").unwrap(), None);
        assert!(!store.path.exists());
    }

    #[test]
    fn saves_reads_and_removes_entries() {
        let (_directory, store) = store();
        store.set("bitbucket:one", "first-value").unwrap();
        store.set("cloudflare-pages:one", "second-value").unwrap();
        assert_eq!(
            store.get("bitbucket:one").unwrap(),
            Some("first-value".into())
        );
        assert_eq!(
            store.get("cloudflare-pages:one").unwrap(),
            Some("second-value".into())
        );

        store.set("bitbucket:one", "replaced-value").unwrap();
        assert_eq!(
            store.take("bitbucket:one").unwrap(),
            Some("replaced-value".into())
        );
        assert_eq!(store.get("bitbucket:one").unwrap(), None);
        assert_eq!(
            store.get("cloudflare-pages:one").unwrap(),
            Some("second-value".into())
        );
    }

    #[test]
    fn writes_versioned_document() {
        let (_directory, store) = store();
        store.set("bitbucket:one", "fixture-value").unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&store.path).unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"version": 1, "entries": {"bitbucket:one": "fixture-value"}})
        );
    }

    #[test]
    fn reads_files_without_the_keychain_record() {
        let (_directory, store) = store();
        write_raw(
            &store,
            br#"{"version":1,"entries":{"bitbucket:one":"fixture-value"}}"#,
        );
        assert_eq!(
            store.get("bitbucket:one").unwrap(),
            Some("fixture-value".into())
        );
        assert_eq!(
            store.move_state("bitbucket:one").unwrap(),
            MoveState {
                stored: true,
                keychain_checked: false
            }
        );
        store.set("bitbucket:two", "other-value").unwrap();
        assert_eq!(
            store.get("bitbucket:one").unwrap(),
            Some("fixture-value".into())
        );
    }

    #[test]
    fn records_keychain_checks_and_clears_them_on_removal() {
        let (_directory, store) = store();
        store.mark_keychain_checked("bitbucket:one").unwrap();
        store
            .set_moved("cloudflare-pages:one", "fixture-value")
            .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&store.path).unwrap()).unwrap();
        assert_eq!(
            value["keychainChecked"],
            serde_json::json!(["bitbucket:one", "cloudflare-pages:one"])
        );
        assert_eq!(
            store.move_state("bitbucket:one").unwrap(),
            MoveState {
                stored: false,
                keychain_checked: true
            }
        );

        assert_eq!(store.take("bitbucket:one").unwrap(), None);
        assert_eq!(
            store.take("cloudflare-pages:one").unwrap(),
            Some("fixture-value".into())
        );
        for key in ["bitbucket:one", "cloudflare-pages:one"] {
            assert_eq!(
                store.move_state(key).unwrap(),
                MoveState {
                    stored: false,
                    keychain_checked: false
                }
            );
        }
    }

    #[test]
    fn backup_and_rollback_restore_the_previous_value() {
        let (_directory, store) = store();
        store.set("bitbucket:one", "fixture-value").unwrap();
        let backup = store.take("bitbucket:one").unwrap();
        assert_eq!(store.get("bitbucket:one").unwrap(), None);
        store.set("bitbucket:one", &backup.unwrap()).unwrap();
        assert_eq!(
            store.get("bitbucket:one").unwrap(),
            Some("fixture-value".into())
        );
    }

    #[test]
    fn corrupt_file_is_reported_and_never_overwritten() {
        let (_directory, store) = store();
        let original = b"{\"version\":1,\"entries\":";
        write_raw(&store, original);
        assert!(store.get("bitbucket:one").is_err());
        assert!(store.set("bitbucket:one", "fixture-value").is_err());
        assert!(store.take("bitbucket:one").is_err());
        assert_eq!(std::fs::read(&store.path).unwrap(), original);
    }

    #[test]
    fn unknown_version_is_reported_and_never_overwritten() {
        let (_directory, store) = store();
        let original = br#"{"version":2,"entries":{"bitbucket:one":"fixture-value"}}"#;
        write_raw(&store, original);
        let error = store.get("bitbucket:one").unwrap_err();
        assert!(error.contains("version"));
        assert!(!error.contains("fixture-value"));
        assert!(store.set("bitbucket:two", "other-value").is_err());
        assert!(store.take("bitbucket:one").is_err());
        assert_eq!(std::fs::read(&store.path).unwrap(), original);
    }

    #[test]
    fn missing_version_or_unexpected_shape_is_rejected() {
        let (_directory, store) = store();
        for original in [
            &br#"{"entries":{}}"#[..],
            br#"{"version":1,"entries":{"bitbucket:one":5}}"#,
            br#"{"version":1,"entries":{},"extra":true}"#,
            br#"[]"#,
        ] {
            write_raw(&store, original);
            assert!(store.get("bitbucket:one").is_err());
            assert!(store.set("bitbucket:one", "fixture-value").is_err());
            assert_eq!(std::fs::read(&store.path).unwrap(), original);
        }
    }

    #[test]
    fn oversized_file_is_rejected() {
        let (_directory, store) = store();
        let file = File::create(&store.path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert!(store.get("bitbucket:one").is_err());
        assert!(store.set("bitbucket:one", "fixture-value").is_err());
        assert_eq!(
            std::fs::metadata(&store.path).unwrap().len(),
            MAX_FILE_BYTES + 1
        );
    }

    #[test]
    fn creates_the_app_data_directory() {
        let directory = tempfile::tempdir().unwrap();
        let store = CredentialFile::in_directory(&directory.path().join("nested"));
        store.set("bitbucket:one", "fixture-value").unwrap();
        assert_eq!(
            store.get("bitbucket:one").unwrap(),
            Some("fixture-value".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn new_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_directory, store) = store();
        store.set("bitbucket:one", "fixture-value").unwrap();
        let mode = std::fs::metadata(&store.path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn temporary_file_options_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("temporary");
        private_file_options().open(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn broad_permissions_are_narrowed() {
        use std::os::unix::fs::PermissionsExt;
        let (_directory, store) = store();
        write_raw(&store, br#"{"version":1,"entries":{}}"#);
        std::fs::set_permissions(&store.path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(store.get("bitbucket:one").unwrap(), None);
        let mode = std::fs::metadata(&store.path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn lookups_wait_until_the_keychain_move_finishes() {
        let waiting = tokio::spawn(wait_until_ready());
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        mark_ready();
        tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
            .await
            .expect("lookup gate opened")
            .unwrap();
    }
}
