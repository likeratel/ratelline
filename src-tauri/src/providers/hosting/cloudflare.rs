use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Deserialize};

use crate::{
    cloudflare_credentials::{self, CloudflareCredential},
    models::{Account, AccountMode, Connection, ConnectionStatus, Provider},
    providers::http::{ApiOrigin, JsonHttpClient},
};

const LABEL: &str = "Cloudflare API";

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    result: T,
    #[serde(default)]
    errors: Vec<ApiMessage>,
    #[serde(default)]
    result_info: Option<ResultInfo>,
}

#[derive(Deserialize)]
struct ApiMessage {
    #[serde(default)]
    message: String,
}

#[derive(Clone, Copy, Deserialize)]
pub(super) struct ResultInfo {
    #[serde(default)]
    pub(super) page: u32,
    #[serde(default)]
    pub(super) total_pages: u32,
}

pub(super) struct Api {
    client: JsonHttpClient,
    credential: CloudflareCredential,
}

impl Api {
    async fn for_connection(provider: Provider, connection_id: &str) -> Result<Self, String> {
        let credential = cloudflare_credentials::get(provider, connection_id).await?;
        Self::new(credential)
    }

    fn new(credential: CloudflareCredential) -> Result<Self, String> {
        cloudflare_credentials::validate_credential(&credential)?;
        let client = JsonHttpClient::new(ApiOrigin::Cloudflare)
            .map_err(|error| error.user_message(LABEL))?;
        Ok(Self { client, credential })
    }

    pub(super) fn account_id(&self) -> &str {
        &self.credential.account_id
    }

    pub(super) fn get<T: DeserializeOwned + Send + 'static>(
        &self,
        path: &[&str],
        query: &[(&str, &str)],
    ) -> BoxFuture<'static, Result<(T, Option<ResultInfo>), String>> {
        let request = self
            .client
            .get::<Envelope<T>>(path, query, &self.credential.token, None);
        Box::pin(async move {
            let envelope = request.await.map_err(|error| error.user_message(LABEL))?;
            if !envelope.success {
                let detail = envelope
                    .errors
                    .into_iter()
                    .find_map(|error| sanitize_api_message(&error.message));
                return Err(detail.unwrap_or_else(|| "Cloudflare rejected this request.".into()));
            }
            Ok((envelope.result, envelope.result_info))
        })
    }

    async fn verify(&self, provider: Provider) -> Result<(), String> {
        let account = self.account_id();
        let path = match provider {
            Provider::CloudflarePages => vec!["accounts", account, "pages", "projects"],
            Provider::CloudflareWorkers => vec!["accounts", account, "workers", "scripts"],
            _ => return Err("The selected provider is not a Cloudflare provider.".into()),
        };
        let query = match provider {
            Provider::CloudflarePages => vec![("page", "1"), ("per_page", "1")],
            Provider::CloudflareWorkers => Vec::new(),
            _ => unreachable!(),
        };
        let _: (serde_json::Value, _) = self.get(&path, &query).await?;
        Ok(())
    }
}

pub async fn authenticate(
    provider: Provider,
    account_id: String,
    token: String,
) -> Result<CloudflareCredential, String> {
    let credential = CloudflareCredential { account_id, token };
    let api = Api::new(credential.clone())?;
    api.verify(provider).await?;
    Ok(credential)
}

pub(super) async fn api_for_target(target: &crate::models::Target) -> Result<Api, String> {
    let api = Api::for_connection(target.provider, &target.connection_id).await?;
    let target_account = target
        .metadata
        .get("accountId")
        .ok_or_else(|| "The Cloudflare target has no account ID.".to_owned())?;
    if target_account != api.account_id() {
        return Err("The Cloudflare target belongs to a different account.".into());
    }
    Ok(api)
}

pub(super) async fn api_for_account(account: &Account) -> Result<Api, String> {
    if !matches!(
        account.provider,
        Provider::CloudflarePages | Provider::CloudflareWorkers
    ) || account.mode != AccountMode::Isolated
    {
        return Err("Reconnect this Cloudflare provider with an account ID and API token.".into());
    }
    let api = Api::for_connection(account.provider, &account.id).await?;
    if account.profile.as_deref() != Some(api.account_id()) {
        return Err("The Cloudflare credential belongs to a different account.".into());
    }
    Ok(api)
}

pub(super) async fn connection(account: &Account) -> Connection {
    if account.mode != AccountMode::Isolated {
        return Connection::disconnected(
            &account.id,
            account.provider,
            ConnectionStatus::Unconfigured,
            "Reconnect this legacy Cloudflare CLI account with an account ID and API token.",
        );
    }
    match api_for_account(account).await {
        Ok(api) => match api.verify(account.provider).await {
            Ok(()) => Connection {
                kind: None,
                connection_id: account.id.clone(),
                provider: account.provider,
                status: ConnectionStatus::Connected,
                account: Some(api.account_id().to_owned()),
                message: None,
                cli_version: None,
            },
            Err(message) => Connection::disconnected(
                &account.id,
                account.provider,
                ConnectionStatus::Error,
                message,
            ),
        },
        Err(message) => Connection::disconnected(
            &account.id,
            account.provider,
            ConnectionStatus::Error,
            message,
        ),
    }
}

fn sanitize_api_message(message: &str) -> Option<String> {
    let message = message.trim();
    (!message.is_empty()
        && message.len() <= 512
        && !message.contains('\0')
        && !message.chars().any(|character| character.is_control()))
    .then(|| format!("Cloudflare rejected this request: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_error_text_is_bounded_and_control_free() {
        assert_eq!(
            sanitize_api_message("missing permission"),
            Some("Cloudflare rejected this request: missing permission".into())
        );
        assert_eq!(sanitize_api_message("bad\nmessage"), None);
        assert_eq!(sanitize_api_message(&"x".repeat(513)), None);
    }

    #[test]
    fn envelope_requires_explicit_success() {
        let envelope: Envelope<Vec<serde_json::Value>> = serde_json::from_str(
            r#"{"success":false,"result":[],"errors":[{"message":"denied"}]}"#,
        )
        .unwrap();
        assert!(!envelope.success);
        assert_eq!(envelope.errors[0].message, "denied");
    }
}
