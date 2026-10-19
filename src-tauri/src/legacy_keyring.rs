//! Lazy migration of OS credential entries from the former `crowline`
//! keyring service names to the current `ratelline` service names.
//!
//! Reads fall back to the legacy entry once and copy it to the current entry.
//! The legacy entry is left in place on read and removed together with the
//! current entry when the connection's credential is deleted.

pub(crate) trait SecretSlot {
    fn read(&self) -> Result<String, keyring::Error>;
    fn write(&self, value: &str) -> Result<(), keyring::Error>;
    fn remove(&self) -> Result<(), keyring::Error>;
}

impl SecretSlot for keyring::Entry {
    fn read(&self) -> Result<String, keyring::Error> {
        self.get_password()
    }

    fn write(&self, value: &str) -> Result<(), keyring::Error> {
        self.set_password(value)
    }

    fn remove(&self) -> Result<(), keyring::Error> {
        self.delete_credential()
    }
}

/// Reads the current entry, falling back to the legacy entry when the current
/// one does not exist. A legacy value is copied to the current entry; a copy
/// failure is logged and the value is still returned.
pub(crate) fn read_with_fallback(
    current: &impl SecretSlot,
    legacy: Option<&impl SecretSlot>,
) -> Result<Option<String>, keyring::Error> {
    match current.read() {
        Ok(value) => return Ok(Some(value)),
        Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(error),
    }
    let Some(legacy) = legacy else {
        return Ok(None);
    };
    let value = match legacy.read() {
        Ok(value) => value,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(error) => return Err(error),
    };
    if current.write(&value).is_err() {
        eprintln!("Failed to copy a credential from the previous secure storage name.");
    }
    Ok(Some(value))
}

/// Deletes both the current and the legacy entry. Missing entries are ignored.
pub(crate) fn remove_both(
    current: &impl SecretSlot,
    legacy: Option<&impl SecretSlot>,
) -> Result<(), keyring::Error> {
    ignore_missing(current.remove())?;
    match legacy {
        Some(legacy) => ignore_missing(legacy.remove()),
        None => Ok(()),
    }
}

fn ignore_missing(result: Result<(), keyring::Error>) -> Result<(), keyring::Error> {
    match result {
        Err(keyring::Error::NoEntry) => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemorySlot {
        value: RefCell<Option<String>>,
        fail_write: bool,
    }

    impl MemorySlot {
        fn with(value: &str) -> Self {
            Self {
                value: RefCell::new(Some(value.into())),
                fail_write: false,
            }
        }
    }

    impl SecretSlot for MemorySlot {
        fn read(&self) -> Result<String, keyring::Error> {
            self.value.borrow().clone().ok_or(keyring::Error::NoEntry)
        }

        fn write(&self, value: &str) -> Result<(), keyring::Error> {
            if self.fail_write {
                return Err(keyring::Error::NoStorageAccess("denied".into()));
            }
            *self.value.borrow_mut() = Some(value.into());
            Ok(())
        }

        fn remove(&self) -> Result<(), keyring::Error> {
            self.value
                .borrow_mut()
                .take()
                .map(|_| ())
                .ok_or(keyring::Error::NoEntry)
        }
    }

    #[test]
    fn prefers_current_entry() {
        let current = MemorySlot::with("current");
        let legacy = MemorySlot::with("legacy");
        assert_eq!(
            read_with_fallback(&current, Some(&legacy)).unwrap(),
            Some("current".into())
        );
    }

    #[test]
    fn copies_legacy_entry_once() {
        let current = MemorySlot::default();
        let legacy = MemorySlot::with("legacy");
        assert_eq!(
            read_with_fallback(&current, Some(&legacy)).unwrap(),
            Some("legacy".into())
        );
        assert_eq!(current.read().unwrap(), "legacy");
        assert_eq!(legacy.read().unwrap(), "legacy");
    }

    #[test]
    fn returns_legacy_value_when_copy_fails() {
        let current = MemorySlot {
            fail_write: true,
            ..MemorySlot::default()
        };
        let legacy = MemorySlot::with("legacy");
        assert_eq!(
            read_with_fallback(&current, Some(&legacy)).unwrap(),
            Some("legacy".into())
        );
    }

    #[test]
    fn reports_missing_when_neither_exists() {
        let current = MemorySlot::default();
        let legacy = MemorySlot::default();
        assert_eq!(read_with_fallback(&current, Some(&legacy)).unwrap(), None);
        assert_eq!(
            read_with_fallback(&current, None::<&MemorySlot>).unwrap(),
            None
        );
    }

    #[test]
    fn removes_current_and_legacy_entries() {
        let current = MemorySlot::with("current");
        let legacy = MemorySlot::with("legacy");
        remove_both(&current, Some(&legacy)).unwrap();
        assert!(current.read().is_err());
        assert!(legacy.read().is_err());
        remove_both(&current, Some(&legacy)).unwrap();
    }
}
