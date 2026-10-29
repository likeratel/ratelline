use serde::{Deserialize, Serialize};

use crate::models::Provider;

const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
const UNAVAILABLE: &str = "The Cloudflare API token is unavailable. Update the token.";

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloudflareCredential {
    pub account_id: String,
    pub token: String,
}

impl std::fmt::Debug for CloudflareCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CloudflareCredential")
            .field("account_id", &self.account_id)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

pub struct CredentialBackup(Option<String>);

pub async fn get(provider: Provider, connection_id: &str) -> Result<CloudflareCredential, String> {
    let account = credential_account(provider, connection_id)?;
    #[cfg(test)]
    {
        return test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .get(&account)
            .cloned()
            .ok_or_else(|| UNAVAILABLE.to_owned());
    }
    #[cfg(not(test))]
    {
        let store = crate::credential_store::app_store().await?;
        tokio::task::spawn_blocking(move || {
            let value = store.get(&account)?.ok_or_else(|| UNAVAILABLE.to_owned())?;
            if value.len() > MAX_CREDENTIAL_BYTES {
                return Err("The saved Cloudflare credential is invalid.".into());
            }
            let credential: CloudflareCredential = serde_json::from_str(&value)
                .map_err(|_| "The saved Cloudflare credential is invalid.".to_owned())?;
            validate_credential(&credential)?;
            Ok(credential)
        })
        .await
        .map_err(|_| "Credential lookup was interrupted.".to_owned())?
    }
}

pub async fn set(
    provider: Provider,
    connection_id: &str,
    credential: CloudflareCredential,
) -> Result<(), String> {
    let account = credential_account(provider, connection_id)?;
    validate_credential(&credential)?;
    #[cfg(test)]
    {
        test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .insert(account, credential);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let value = serde_json::to_string(&credential)
            .map_err(|_| "The Cloudflare credential could not be encoded.".to_owned())?;
        if value.len() > MAX_CREDENTIAL_BYTES {
            return Err("The Cloudflare credential is too large.".into());
        }
        let store = crate::credential_store::app_store().await?;
        tokio::task::spawn_blocking(move || {
            store
                .set(&account, &value)
                .map_err(|error| format!("The Cloudflare API token could not be saved. {error}"))
        })
        .await
        .map_err(|_| "Credential storage was interrupted.".to_owned())?
    }
}

pub async fn delete(provider: Provider, connection_id: &str) -> Result<(), String> {
    let account = credential_account(provider, connection_id)?;
    #[cfg(test)]
    {
        test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .remove(&account);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let store = crate::credential_store::app_store().await?;
        tokio::task::spawn_blocking(move || remove_entry(store, &account).map(|_| ()))
            .await
            .map_err(|_| "Credential removal was interrupted.".to_owned())?
    }
}

pub async fn backup_and_delete(
    provider: Provider,
    connection_id: &str,
) -> Result<CredentialBackup, String> {
    let account = credential_account(provider, connection_id)?;
    #[cfg(test)]
    {
        let value = test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .remove(&account)
            .map(|credential| serde_json::to_string(&credential))
            .transpose()
            .map_err(|_| "The test credential could not be encoded.".to_owned())?;
        Ok(CredentialBackup(value))
    }
    #[cfg(not(test))]
    {
        let store = crate::credential_store::app_store().await?;
        tokio::task::spawn_blocking(move || remove_entry(store, &account).map(CredentialBackup))
            .await
            .map_err(|_| "Credential removal was interrupted.".to_owned())?
    }
}

pub async fn restore(
    provider: Provider,
    connection_id: &str,
    backup: CredentialBackup,
) -> Result<(), String> {
    let account = credential_account(provider, connection_id)?;
    let Some(value) = backup.0 else {
        return Ok(());
    };
    if value.len() > MAX_CREDENTIAL_BYTES {
        return Err("The Cloudflare credential backup is invalid.".into());
    }
    let credential: CloudflareCredential = serde_json::from_str(&value)
        .map_err(|_| "The Cloudflare credential backup is invalid.".to_owned())?;
    validate_credential(&credential)?;
    #[cfg(test)]
    {
        test_store()
            .lock()
            .map_err(|_| "Test credential storage is unavailable.".to_owned())?
            .insert(account, credential);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let store = crate::credential_store::app_store().await?;
        tokio::task::spawn_blocking(move || {
            store
                .set(&account, &value)
                .map_err(|_| "The Cloudflare API token rollback failed.".to_owned())
        })
        .await
        .map_err(|_| "Credential rollback was interrupted.".to_owned())?
    }
}

pub(crate) fn validate_credential(credential: &CloudflareCredential) -> Result<(), String> {
    if credential.account_id.len() != 32
        || !credential
            .account_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Enter a valid 32-character Cloudflare account ID.".into());
    }
    if !(16..=4096).contains(&credential.token.len())
        || !credential.token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err("Enter a valid Cloudflare API token.".into());
    }
    Ok(())
}

/// Key of a Cloudflare connection in the credentials file.
pub(crate) fn credential_account(
    provider: Provider,
    connection_id: &str,
) -> Result<String, String> {
    let prefix = match provider {
        Provider::CloudflarePages => "cloudflare-pages:",
        Provider::CloudflareWorkers => "cloudflare-workers:",
        _ => return Err("The selected provider does not use a Cloudflare API token.".into()),
    };
    let uuid = connection_id
        .strip_prefix(prefix)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| "The Cloudflare connection ID is invalid.".to_owned())?;
    Ok(format!("{}:{uuid}", provider.as_str()))
}

/// Removes the saved entry and its keychain-move record, returning the value.
/// The keychain is not accessed.
#[cfg(not(test))]
fn remove_entry(
    store: &crate::credential_store::CredentialFile,
    account: &str,
) -> Result<Option<String>, String> {
    let value = store
        .take(account)
        .map_err(|error| format!("The Cloudflare API token could not be removed. {error}"))?;
    Ok(value)
}

#[cfg(test)]
fn test_store() -> &'static std::sync::Mutex<std::collections::HashMap<String, CloudflareCredential>>
{
    static STORE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, CloudflareCredential>>,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(Default::default)
}

#[cfg(test)]
pub(crate) fn test_contains_token(token: &str) -> bool {
    test_store()
        .lock()
        .expect("test Cloudflare credential store")
        .values()
        .any(|credential| credential.token == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential() -> CloudflareCredential {
        CloudflareCredential {
            account_id: "0123456789abcdef0123456789abcdef".into(),
            token: "synthetic-token-value".into(),
        }
    }

    #[test]
    fn debug_redacts_token() {
        let debug = format!("{:?}", credential());
        assert!(debug.contains("0123456789abcdef0123456789abcdef"));
        assert!(!debug.contains("synthetic-token-value"));
    }

    #[test]
    fn validates_provider_scoped_connection_ids() {
        let id = "cloudflare-pages:00000000-0000-0000-0000-000000000001";
        assert!(credential_account(Provider::CloudflarePages, id).is_ok());
        assert!(credential_account(Provider::CloudflareWorkers, id).is_err());
        assert!(credential_account(Provider::Firebase, id).is_err());
    }

    #[tokio::test]
    async fn backup_delete_and_restore_round_trip() {
        let id = "cloudflare-workers:00000000-0000-0000-0000-000000000002";
        set(Provider::CloudflareWorkers, id, credential())
            .await
            .unwrap();
        let backup = backup_and_delete(Provider::CloudflareWorkers, id)
            .await
            .unwrap();
        assert!(get(Provider::CloudflareWorkers, id).await.is_err());
        restore(Provider::CloudflareWorkers, id, backup)
            .await
            .unwrap();
        assert_eq!(
            get(Provider::CloudflareWorkers, id).await.unwrap(),
            credential()
        );
        delete(Provider::CloudflareWorkers, id).await.unwrap();
    }
}
