use serde::{Deserialize, Serialize};

#[cfg(not(test))]
const SERVICE: &str = "com.likeratel.ratelline.bitbucket";
/// Service name used by releases published before the product was renamed.
#[cfg(not(test))]
const LEGACY_SERVICE: &str = "com.likeratel.crowline.bitbucket";
#[cfg(not(test))]
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketCredential {
    pub email: String,
    pub token: String,
    pub uuid: String,
    pub account_id: String,
    pub display_name: String,
}

impl std::fmt::Debug for BitbucketCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BitbucketCredential")
            .field("email", &"[redacted]")
            .field("token", &"[redacted]")
            .field("uuid", &self.uuid)
            .field("account_id", &self.account_id)
            .field("display_name", &self.display_name)
            .finish()
    }
}

pub async fn get_bitbucket(connection_id: &str) -> Result<BitbucketCredential, String> {
    validate_connection_id(connection_id)?;
    #[cfg(test)]
    {
        return test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .get(connection_id)
            .cloned()
            .ok_or_else(|| {
                "The Bitbucket credential is unavailable in secure storage.".to_owned()
            });
    }
    #[cfg(not(test))]
    {
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let value = read_entry(&account).ok().flatten().ok_or_else(|| {
                "The Bitbucket credential is unavailable in secure storage.".to_owned()
            })?;
            if value.len() > MAX_CREDENTIAL_BYTES {
                return Err("The Bitbucket credential in secure storage is invalid.".into());
            }
            serde_json::from_str(&value)
                .map_err(|_| "The Bitbucket credential in secure storage is invalid.".to_owned())
        })
        .await
        .map_err(|_| "Secure credential lookup was interrupted.".to_owned())?
    }
}

pub async fn set_bitbucket(
    connection_id: &str,
    credential: BitbucketCredential,
) -> Result<(), String> {
    validate_connection_id(connection_id)?;
    #[cfg(test)]
    {
        test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .insert(connection_id.to_owned(), credential);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let account = connection_id.to_owned();
        let value = serde_json::to_string(&credential)
            .map_err(|_| "The Bitbucket credential could not be encoded.".to_owned())?;
        if value.len() > MAX_CREDENTIAL_BYTES {
            return Err("The Bitbucket credential is too large.".into());
        }
        tokio::task::spawn_blocking(move || {
            entry(&account)?
                .set_password(&value)
                .map_err(|_| "The Bitbucket credential could not be saved securely.".to_owned())
        })
        .await
        .map_err(|_| "Secure credential storage was interrupted.".to_owned())?
    }
}

pub async fn delete_bitbucket(connection_id: &str) -> Result<(), String> {
    validate_connection_id(connection_id)?;
    #[cfg(test)]
    {
        test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .remove(connection_id);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || {
            remove_entries(&account).map_err(|_| {
                "The Bitbucket credential could not be removed from secure storage.".to_owned()
            })
        })
        .await
        .map_err(|_| "Secure credential removal was interrupted.".to_owned())?
    }
}

pub struct CredentialBackup(Option<String>);

pub async fn backup_and_delete_bitbucket(connection_id: &str) -> Result<CredentialBackup, String> {
    validate_connection_id(connection_id)?;
    #[cfg(test)]
    {
        let value = test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .remove(connection_id)
            .map(|credential| serde_json::to_string(&credential))
            .transpose()
            .map_err(|_| "The test credential could not be encoded.".to_owned())?;
        Ok(CredentialBackup(value))
    }
    #[cfg(not(test))]
    {
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let value = read_entry(&account).map_err(|_| {
                "The Bitbucket credential is unavailable in secure storage.".to_owned()
            })?;
            remove_entries(&account).map_err(|_| {
                "The Bitbucket credential could not be removed from secure storage.".to_owned()
            })?;
            Ok(CredentialBackup(value))
        })
        .await
        .map_err(|_| "Secure credential removal was interrupted.".to_owned())?
    }
}

pub async fn restore_bitbucket(
    connection_id: &str,
    backup: CredentialBackup,
) -> Result<(), String> {
    let Some(value) = backup.0 else {
        return Ok(());
    };
    validate_connection_id(connection_id)?;
    #[cfg(test)]
    {
        let credential = serde_json::from_str(&value)
            .map_err(|_| "The test credential backup is invalid.".to_owned())?;
        test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .insert(connection_id.to_owned(), credential);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || {
            entry(&account)?
                .set_password(&value)
                .map_err(|_| "The Bitbucket credential rollback failed.".to_owned())
        })
        .await
        .map_err(|_| "Secure credential rollback was interrupted.".to_owned())?
    }
}

#[cfg(not(test))]
fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account)
        .map_err(|_| "Secure credential storage is unavailable.".to_owned())
}

#[cfg(not(test))]
fn legacy_entry(account: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(LEGACY_SERVICE, account).ok()
}

#[cfg(not(test))]
fn read_entry(account: &str) -> Result<Option<String>, String> {
    crate::legacy_keyring::read_with_fallback(&entry(account)?, legacy_entry(account).as_ref())
        .map_err(|_| "The Bitbucket credential is unavailable in secure storage.".to_owned())
}

#[cfg(not(test))]
fn remove_entries(account: &str) -> Result<(), String> {
    crate::legacy_keyring::remove_both(&entry(account)?, legacy_entry(account).as_ref()).map_err(
        |_| "The Bitbucket credential could not be removed from secure storage.".to_owned(),
    )
}

#[cfg(test)]
fn test_store() -> &'static std::sync::Mutex<std::collections::HashMap<String, BitbucketCredential>>
{
    static STORE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, BitbucketCredential>>,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(Default::default)
}

#[cfg(test)]
pub fn test_contains_token(token: &str) -> bool {
    test_store()
        .lock()
        .expect("test credential store")
        .values()
        .any(|credential| credential.token == token)
}

fn validate_connection_id(value: &str) -> Result<(), String> {
    value
        .strip_prefix("bitbucket:")
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .map(|_| ())
        .ok_or_else(|| "The Bitbucket connection ID is invalid.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_email_and_token() {
        let credential = BitbucketCredential {
            email: "person@example.invalid".into(),
            token: "secret-token".into(),
            uuid: "{user-id}".into(),
            account_id: "account-id".into(),
            display_name: "Example Person".into(),
        };
        let debug = format!("{credential:?}");
        assert!(!debug.contains("person@example.invalid"));
        assert!(!debug.contains("secret-token"));
    }
}
