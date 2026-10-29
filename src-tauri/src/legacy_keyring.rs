//! One-time move of Bitbucket and Cloudflare API credentials from the OS
//! keychain to `credentials.json` in the app data directory.
//!
//! At startup every Bitbucket and Cloudflare connection that has no saved file
//! entry and was never looked up before is looked up in the keychain, first
//! under the current `ratelline` service name and then under the former
//! `crowline` one. A value found there is written to the file, and only after
//! that write succeeds are both keychain entries deleted. Every lookup, whether
//! it moved a value, found nothing, or was denied, is recorded in the file, so
//! each connection reads the keychain at most once.
//!
//! This module is the only place that uses the keychain.

use crate::{
    credential_store::{CredentialFile, MoveState},
    models::{Account, Provider},
};

/// Upper bound for a moved value; matches the per-service credential limit.
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;

pub(crate) trait SecretSlot {
    fn read(&self) -> Result<String, keyring::Error>;
    fn remove(&self) -> Result<(), keyring::Error>;
}

impl SecretSlot for keyring::Entry {
    fn read(&self) -> Result<String, keyring::Error> {
        self.get_password()
    }

    fn remove(&self) -> Result<(), keyring::Error> {
        self.delete_credential()
    }
}

/// Destination of moved credentials and record of finished keychain lookups.
pub(crate) trait CredentialSink {
    fn move_state(&self, key: &str) -> Result<MoveState, String>;
    /// Saves the value and records the key as checked in one write.
    fn set_moved(&self, key: &str, value: &str) -> Result<(), String>;
    fn mark_keychain_checked(&self, key: &str) -> Result<(), String>;
}

impl CredentialSink for CredentialFile {
    fn move_state(&self, key: &str) -> Result<MoveState, String> {
        CredentialFile::move_state(self, key)
    }

    fn set_moved(&self, key: &str, value: &str) -> Result<(), String> {
        CredentialFile::set_moved(self, key, value)
    }

    fn mark_keychain_checked(&self, key: &str) -> Result<(), String> {
        CredentialFile::mark_keychain_checked(self, key)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum MoveOutcome {
    /// The file already holds the credential; the keychain was not accessed.
    AlreadyInFile,
    /// An earlier start already looked this key up; the keychain was not accessed.
    AlreadyChecked,
    NotInKeychain,
    Moved,
    /// The value was moved, but a keychain entry could not be deleted.
    MovedWithLeftover,
    Failed(&'static str),
}

/// Keychain service names used for one credential kind.
struct KeychainService {
    label: &'static str,
    current: &'static str,
    /// Service name used by releases published before the product was renamed.
    legacy: &'static str,
}

const BITBUCKET: KeychainService = KeychainService {
    label: "Bitbucket",
    current: "com.likeratel.ratelline.bitbucket",
    legacy: "com.likeratel.crowline.bitbucket",
};

const CLOUDFLARE: KeychainService = KeychainService {
    label: "Cloudflare",
    current: "com.likeratel.ratelline.cloudflare",
    legacy: "com.likeratel.crowline.cloudflare",
};

/// Moves one credential. `slots` are tried in order (current, then legacy).
pub(crate) fn move_entry<S: SecretSlot>(
    sink: &impl CredentialSink,
    key: &str,
    slots: &[S],
) -> MoveOutcome {
    match sink.move_state(key) {
        Ok(MoveState { stored: true, .. }) => return MoveOutcome::AlreadyInFile,
        Ok(MoveState {
            keychain_checked: true,
            ..
        }) => return MoveOutcome::AlreadyChecked,
        Ok(_) => {}
        Err(_) => return MoveOutcome::Failed("the credentials file could not be read"),
    }
    let value = match read_first(slots) {
        Ok(Some(value)) => value,
        Ok(None) => return record_check(sink, key, MoveOutcome::NotInKeychain),
        Err(_) => {
            return record_check(
                sink,
                key,
                MoveOutcome::Failed("the keychain entry could not be read"),
            )
        }
    };
    if value.len() > MAX_CREDENTIAL_BYTES {
        return record_check(
            sink,
            key,
            MoveOutcome::Failed("the keychain entry is too large"),
        );
    }
    if sink.set_moved(key, &value).is_err() {
        return MoveOutcome::Failed("the credentials file could not be written");
    }
    match remove_all(slots) {
        Ok(()) => MoveOutcome::Moved,
        Err(_) => MoveOutcome::MovedWithLeftover,
    }
}

/// Records a finished lookup so the next start does not read the keychain
/// again. When recording fails, the lookup is retried on the next start.
fn record_check(sink: &impl CredentialSink, key: &str, outcome: MoveOutcome) -> MoveOutcome {
    match sink.mark_keychain_checked(key) {
        Ok(()) => outcome,
        Err(_) => MoveOutcome::Failed("the keychain lookup could not be recorded"),
    }
}

/// Returns the first existing value. An error other than a missing entry
/// stops the lookup, so an inaccessible current entry is not masked.
fn read_first<S: SecretSlot>(slots: &[S]) -> Result<Option<String>, keyring::Error> {
    for slot in slots {
        match slot.read() {
            Ok(value) => return Ok(Some(value)),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

/// Deletes every slot, ignoring missing entries. All slots are attempted even
/// when one fails.
fn remove_all<S: SecretSlot>(slots: &[S]) -> Result<(), keyring::Error> {
    let results: Vec<_> = slots
        .iter()
        .map(|slot| match slot.remove() {
            Err(keyring::Error::NoEntry) => Ok(()),
            other => other,
        })
        .collect();
    results.into_iter().collect()
}

fn keychain_slots(service: &KeychainService, key: &str) -> Vec<keyring::Entry> {
    [service.current, service.legacy]
        .into_iter()
        .filter_map(|name| keyring::Entry::new(name, key).ok())
        .collect()
}

fn storage_key(account: &Account) -> Option<(&'static KeychainService, String)> {
    let key = match account.provider {
        Provider::Bitbucket => {
            crate::credentials::storage_key(&account.id).map(|key| (&BITBUCKET, key))
        }
        Provider::CloudflarePages | Provider::CloudflareWorkers => {
            crate::cloudflare_credentials::credential_account(account.provider, &account.id)
                .map(|key| (&CLOUDFLARE, key))
        }
        _ => return None,
    };
    key.ok()
}

/// Moves the keychain credentials of every Bitbucket and Cloudflare
/// connection into the file. Blocking; failures are logged without values.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn move_connections(store: &CredentialFile, accounts: &[Account]) {
    for (service, key) in accounts.iter().filter_map(storage_key) {
        match move_entry(store, &key, &keychain_slots(service, &key)) {
            MoveOutcome::AlreadyInFile
            | MoveOutcome::AlreadyChecked
            | MoveOutcome::NotInKeychain => {}
            MoveOutcome::Moved => eprintln!(
                "Moved a {} credential from the keychain to the credentials file.",
                service.label
            ),
            MoveOutcome::MovedWithLeftover => eprintln!(
                "Moved a {} credential to the credentials file, but its keychain entry could not be deleted.",
                service.label
            ),
            MoveOutcome::Failed(reason) => eprintln!(
                "Failed to move a {} credential from the keychain: {reason}.",
                service.label
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct MemorySlot {
        value: RefCell<Option<String>>,
        fail_read: bool,
        fail_remove: bool,
        reads: Cell<usize>,
        removes: Cell<usize>,
    }

    impl MemorySlot {
        fn with(value: &str) -> Self {
            Self {
                value: RefCell::new(Some(value.into())),
                ..Self::default()
            }
        }

        fn touched(&self) -> bool {
            self.reads.get() + self.removes.get() > 0
        }
    }

    impl SecretSlot for MemorySlot {
        fn read(&self) -> Result<String, keyring::Error> {
            self.reads.set(self.reads.get() + 1);
            if self.fail_read {
                return Err(keyring::Error::NoStorageAccess("denied".into()));
            }
            self.value.borrow().clone().ok_or(keyring::Error::NoEntry)
        }

        fn remove(&self) -> Result<(), keyring::Error> {
            self.removes.set(self.removes.get() + 1);
            if self.fail_remove {
                return Err(keyring::Error::NoStorageAccess("denied".into()));
            }
            self.value
                .borrow_mut()
                .take()
                .map(|_| ())
                .ok_or(keyring::Error::NoEntry)
        }
    }

    #[derive(Default)]
    struct MemorySink {
        entries: RefCell<HashMap<String, String>>,
        checked: RefCell<HashSet<String>>,
        fail_read: bool,
        fail_write: bool,
    }

    impl MemorySink {
        fn get(&self, key: &str) -> Option<String> {
            self.entries.borrow().get(key).cloned()
        }

        fn is_checked(&self, key: &str) -> bool {
            self.checked.borrow().contains(key)
        }
    }

    impl CredentialSink for MemorySink {
        fn move_state(&self, key: &str) -> Result<MoveState, String> {
            if self.fail_read {
                return Err("unreadable".into());
            }
            Ok(MoveState {
                stored: self.entries.borrow().contains_key(key),
                keychain_checked: self.is_checked(key),
            })
        }

        fn set_moved(&self, key: &str, value: &str) -> Result<(), String> {
            if self.fail_write {
                return Err("unwritable".into());
            }
            self.entries
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            self.checked.borrow_mut().insert(key.to_owned());
            Ok(())
        }

        fn mark_keychain_checked(&self, key: &str) -> Result<(), String> {
            if self.fail_write {
                return Err("unwritable".into());
            }
            self.checked.borrow_mut().insert(key.to_owned());
            Ok(())
        }
    }

    const KEY: &str = "bitbucket:00000000-0000-0000-0000-000000000001";

    #[test]
    fn moves_current_entry_and_deletes_both_keychain_entries() {
        let sink = MemorySink::default();
        let slots = [MemorySlot::with("current"), MemorySlot::with("legacy")];
        assert_eq!(move_entry(&sink, KEY, &slots), MoveOutcome::Moved);
        assert_eq!(sink.get(KEY), Some("current".into()));
        assert!(sink.is_checked(KEY));
        assert!(slots.iter().all(|slot| slot.value.borrow().is_none()));
    }

    #[test]
    fn falls_back_to_the_legacy_entry() {
        let sink = MemorySink::default();
        let slots = [MemorySlot::default(), MemorySlot::with("legacy")];
        assert_eq!(move_entry(&sink, KEY, &slots), MoveOutcome::Moved);
        assert_eq!(sink.get(KEY), Some("legacy".into()));
        assert!(slots[1].value.borrow().is_none());
    }

    #[test]
    fn keeps_keychain_entries_when_the_file_write_fails() {
        let sink = MemorySink {
            fail_write: true,
            ..MemorySink::default()
        };
        let slots = [MemorySlot::with("current"), MemorySlot::with("legacy")];
        assert!(matches!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::Failed(_)
        ));
        assert!(slots.iter().all(|slot| slot.removes.get() == 0));
        assert_eq!(slots[0].value.borrow().as_deref(), Some("current"));
    }

    #[test]
    fn does_not_touch_the_keychain_when_the_file_has_the_entry() {
        let sink = MemorySink::default();
        sink.entries
            .borrow_mut()
            .insert(KEY.to_owned(), "saved".to_owned());
        let slots = [MemorySlot::with("current"), MemorySlot::with("legacy")];
        assert_eq!(move_entry(&sink, KEY, &slots), MoveOutcome::AlreadyInFile);
        assert!(!slots.iter().any(MemorySlot::touched));
        assert_eq!(sink.get(KEY), Some("saved".into()));
    }

    #[test]
    fn does_not_touch_the_keychain_when_the_file_is_unreadable() {
        let sink = MemorySink {
            fail_read: true,
            ..MemorySink::default()
        };
        let slots = [MemorySlot::with("current")];
        assert!(matches!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::Failed(_)
        ));
        assert!(!slots[0].touched());
    }

    #[test]
    fn does_not_touch_the_keychain_when_the_key_was_checked() {
        let sink = MemorySink::default();
        sink.checked.borrow_mut().insert(KEY.to_owned());
        let slots = [MemorySlot::with("current"), MemorySlot::with("legacy")];
        assert_eq!(move_entry(&sink, KEY, &slots), MoveOutcome::AlreadyChecked);
        assert!(!slots.iter().any(MemorySlot::touched));
        assert_eq!(sink.get(KEY), None);
    }

    #[test]
    fn records_a_denied_lookup_so_it_is_not_repeated() {
        let sink = MemorySink::default();
        let slots = [MemorySlot {
            fail_read: true,
            ..MemorySlot::with("current")
        }];
        assert!(matches!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::Failed(_)
        ));
        assert!(sink.is_checked(KEY));
        assert_eq!(move_entry(&sink, KEY, &slots), MoveOutcome::AlreadyChecked);
        assert_eq!(slots[0].reads.get(), 1);
    }

    #[test]
    fn unrecorded_lookup_is_reported_as_a_failure() {
        let sink = MemorySink {
            fail_write: true,
            ..MemorySink::default()
        };
        let slots = [MemorySlot::default()];
        assert_eq!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::Failed("the keychain lookup could not be recorded")
        );
        assert!(!sink.is_checked(KEY));
    }

    #[test]
    fn reports_missing_when_no_keychain_entry_exists() {
        let sink = MemorySink::default();
        let slots = [MemorySlot::default(), MemorySlot::default()];
        assert_eq!(move_entry(&sink, KEY, &slots), MoveOutcome::NotInKeychain);
        assert_eq!(sink.get(KEY), None);
        assert!(sink.is_checked(KEY));
    }

    #[test]
    fn read_failure_leaves_everything_in_place() {
        let sink = MemorySink::default();
        let slots = [
            MemorySlot {
                fail_read: true,
                ..MemorySlot::with("current")
            },
            MemorySlot::with("legacy"),
        ];
        assert!(matches!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::Failed(_)
        ));
        assert_eq!(sink.get(KEY), None);
        assert!(sink.is_checked(KEY));
        assert_eq!(slots[1].reads.get(), 0);
        assert!(slots.iter().all(|slot| slot.removes.get() == 0));
    }

    #[test]
    fn delete_failure_still_keeps_the_moved_value() {
        let sink = MemorySink::default();
        let slots = [
            MemorySlot {
                fail_remove: true,
                ..MemorySlot::with("current")
            },
            MemorySlot::with("legacy"),
        ];
        assert_eq!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::MovedWithLeftover
        );
        assert_eq!(sink.get(KEY), Some("current".into()));
        assert!(slots[1].value.borrow().is_none());
    }

    #[test]
    fn oversized_keychain_value_is_not_moved() {
        let sink = MemorySink::default();
        let slots = [MemorySlot::with(&"x".repeat(MAX_CREDENTIAL_BYTES + 1))];
        assert!(matches!(
            move_entry(&sink, KEY, &slots),
            MoveOutcome::Failed(_)
        ));
        assert_eq!(sink.get(KEY), None);
        assert_eq!(slots[0].removes.get(), 0);
    }

    #[test]
    fn remove_all_ignores_missing_entries() {
        let slots = [MemorySlot::with("current"), MemorySlot::default()];
        remove_all(&slots).unwrap();
        remove_all(&slots).unwrap();
        assert!(slots[0].value.borrow().is_none());
    }

    #[test]
    fn moves_into_the_credentials_file() {
        let directory = tempfile::tempdir().unwrap();
        let store = CredentialFile::in_directory(directory.path());
        let slots = [MemorySlot::with("fixture-value")];
        assert_eq!(move_entry(&store, KEY, &slots), MoveOutcome::Moved);
        assert_eq!(store.get(KEY).unwrap(), Some("fixture-value".into()));
        assert!(store.move_state(KEY).unwrap().keychain_checked);
    }

    #[test]
    fn checked_record_in_the_file_skips_the_keychain() {
        let directory = tempfile::tempdir().unwrap();
        let store = CredentialFile::in_directory(directory.path());
        let denied = [MemorySlot {
            fail_read: true,
            ..MemorySlot::with("current")
        }];
        assert!(matches!(
            move_entry(&store, KEY, &denied),
            MoveOutcome::Failed(_)
        ));
        let later = [MemorySlot::with("current")];
        assert_eq!(move_entry(&store, KEY, &later), MoveOutcome::AlreadyChecked);
        assert!(!later[0].touched());
    }

    #[test]
    fn maps_connections_to_distinct_storage_keys() {
        let account = |id: &str, provider| Account {
            id: id.into(),
            provider,
            label: "Fixture".into(),
            mode: crate::models::AccountMode::Isolated,
            profile: None,
            cli_username: None,
        };
        let uuid = "00000000-0000-0000-0000-000000000001";
        let keys: Vec<_> = [
            account(&format!("bitbucket:{uuid}"), Provider::Bitbucket),
            account(
                &format!("cloudflare-pages:{uuid}"),
                Provider::CloudflarePages,
            ),
            account(
                &format!("cloudflare-workers:{uuid}"),
                Provider::CloudflareWorkers,
            ),
            account("bitbucket:not-a-uuid", Provider::Bitbucket),
            account(&format!("github:{uuid}"), Provider::Github),
        ]
        .iter()
        .filter_map(storage_key)
        .map(|(service, key)| (service.label, key))
        .collect();
        assert_eq!(
            keys,
            [
                ("Bitbucket", format!("bitbucket:{uuid}")),
                ("Cloudflare", format!("cloudflare-pages:{uuid}")),
                ("Cloudflare", format!("cloudflare-workers:{uuid}")),
            ]
        );
    }
}
