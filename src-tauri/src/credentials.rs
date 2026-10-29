use serde::{Deserialize, Serialize};

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
            .ok_or_else(|| UNAVAILABLE.to_owned());
    }
    #[cfg(not(test))]
    {
        let store = crate::credential_store::app_store().await?;
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let value = store.get(&account)?.ok_or_else(|| UNAVAILABLE.to_owned())?;
            if value.len() > MAX_CREDENTIAL_BYTES {
                return Err("The saved Bitbucket credential is invalid.".into());
            }
            serde_json::from_str(&value)
                .map_err(|_| "The saved Bitbucket credential is invalid.".to_owned())
        })
        .await
        .map_err(|_| "Credential lookup was interrupted.".to_owned())?
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
        let store = crate::credential_store::app_store().await?;
        tokio::task::spawn_blocking(move || {
            store
                .set(&account, &value)
                .map_err(|error| format!("The Bitbucket credential could not be saved. {error}"))
        })
        .await
        .map_err(|_| "Credential storage was interrupted.".to_owned())?
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
        let store = crate::credential_store::app_store().await?;
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || remove_entry(store, &account).map(|_| ()))
            .await
            .map_err(|_| "Credential removal was interrupted.".to_owned())?
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
        let store = crate::credential_store::app_store().await?;
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || remove_entry(store, &account).map(CredentialBackup))
            .await
            .map_err(|_| "Credential removal was interrupted.".to_owned())?
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
        let store = crate::credential_store::app_store().await?;
        let account = connection_id.to_owned();
        tokio::task::spawn_blocking(move || {
            store
                .set(&account, &value)
                .map_err(|_| "The Bitbucket credential rollback failed.".to_owned())
        })
        .await
        .map_err(|_| "Credential rollback was interrupted.".to_owned())?
    }
}

const UNAVAILABLE: &str = "The Bitbucket credential is unavailable. Update the token.";

/// Removes the saved entry and its keychain-move record, returning the value.
/// The keychain is not accessed.
#[cfg(not(test))]
fn remove_entry(
    store: &crate::credential_store::CredentialFile,
    account: &str,
) -> Result<Option<String>, String> {
    let value = store
        .take(account)
        .map_err(|error| format!("The Bitbucket credential could not be removed. {error}"))?;
    Ok(value)
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

/// Key of a Bitbucket connection in the credentials file.
pub(crate) fn storage_key(connection_id: &str) -> Result<String, String> {
    validate_connection_id(connection_id).map(|()| connection_id.to_owned())
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
