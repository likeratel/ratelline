pub mod eas;
pub mod fastlane;
pub mod firebase;
pub mod supabase;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, ActivityPage, Connection, ConnectionStatus, Discovery, Issue, Provider, Target,
    },
};

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    match account.provider {
        Provider::Eas => eas::discover(cli, account).await,
        Provider::Firebase => firebase::discover(cli, account).await,
        Provider::Supabase => supabase::discover(cli, account).await,
        Provider::Fastlane => fastlane::discover(cli, account).await,
        _ => unsupported_discovery(account),
    }
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    match account.provider {
        Provider::Eas => eas::connection(cli, account).await,
        Provider::Firebase => firebase::connection(cli, account).await,
        Provider::Supabase => supabase::connection(cli, account).await,
        Provider::Fastlane => fastlane::connection(cli, account).await,
        _ => Connection::disconnected(
            &account.id,
            account.provider,
            ConnectionStatus::Error,
            "The mobile provider adapter received an unsupported provider.",
        ),
    }
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    match target.provider {
        Provider::Eas => eas::page(cli, target, cursor).await,
        Provider::Firebase => firebase::page(cli, target, cursor).await,
        Provider::Supabase => supabase::page(cli, target, cursor).await,
        Provider::Fastlane => fastlane::page(cli, target, cursor).await,
        _ => Err(CliError::InvalidOutput),
    }
}

pub(super) fn unavailable_discovery(
    connection: Connection,
    account: &Account,
    message: &'static str,
) -> Discovery {
    Discovery {
        connection,
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: vec![Issue {
            kind: crate::models::IssueKind::Other,
            provider: account.provider,
            connection_id: Some(account.id.clone()),
            message: message.into(),
            target_id: None,
        }],
    }
}

pub(super) fn unavailable_page(
    target: &Target,
    cursor: Option<&str>,
    message: &'static str,
) -> Result<ActivityPage, CliError> {
    validate_unavailable_request(target, cursor)?;
    Ok(ActivityPage {
        activities: Vec::new(),
        next_cursor: None,
        issues: vec![Issue {
            kind: crate::models::IssueKind::Other,
            provider: target.provider,
            connection_id: Some(target.connection_id.clone()),
            message: message.into(),
            target_id: Some(target.id.clone()),
        }],
    })
}

pub(super) fn missing_cli(account: &Account, provider_name: &str) -> Connection {
    Connection::disconnected(
        &account.id,
        account.provider,
        ConnectionStatus::MissingCli,
        format!("{provider_name} CLI was not found."),
    )
}

pub(super) fn unavailable_connection(
    account: &Account,
    version: Option<String>,
    authenticated_account: Option<String>,
    message: &'static str,
) -> Connection {
    Connection {
        kind: None,
        connection_id: account.id.clone(),
        provider: account.provider,
        status: ConnectionStatus::Unconfigured,
        account: authenticated_account,
        message: Some(message.into()),
        cli_version: version,
    }
}

pub(super) fn failed_connection(
    account: &Account,
    version: Option<String>,
    provider_name: &str,
    error: CliError,
) -> Connection {
    let status = match error {
        CliError::SignedOut => ConnectionStatus::SignedOut,
        CliError::Unavailable => ConnectionStatus::MissingCli,
        _ => ConnectionStatus::Error,
    };
    Connection {
        kind: None,
        connection_id: account.id.clone(),
        provider: account.provider,
        status,
        account: None,
        message: Some(error.user_message(provider_name)),
        cli_version: version,
    }
}

fn validate_unavailable_request(target: &Target, cursor: Option<&str>) -> Result<(), CliError> {
    target.validate().map_err(|_| CliError::InvalidOutput)?;
    if target.connection_id.trim().is_empty() || target.connection_id.contains('\0') {
        return Err(CliError::InvalidOutput);
    }
    if cursor.is_some() {
        return Err(CliError::InvalidOutput);
    }
    Ok(())
}

fn unsupported_discovery(account: &Account) -> Discovery {
    let message = "The mobile provider adapter received an unsupported provider.";
    Discovery {
        connection: Connection::disconnected(
            &account.id,
            account.provider,
            ConnectionStatus::Error,
            message,
        ),
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: vec![Issue {
            kind: crate::models::IssueKind::Other,
            provider: account.provider,
            connection_id: Some(account.id.clone()),
            message: message.into(),
            target_id: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn unavailable_page_rejects_invented_cursor() {
        let target = Target {
            id: "mobile-target".into(),
            provider: Provider::Fastlane,
            connection_id: "fastlane:local".into(),
            name: "Mobile release".into(),
            description: None,
            metadata: HashMap::new(),
        };

        assert!(matches!(
            unavailable_page(&target, Some("1"), "Unavailable"),
            Err(CliError::InvalidOutput)
        ));
    }

    #[test]
    fn unavailable_page_returns_an_explicit_issue() {
        let target = Target {
            id: "mobile-target".into(),
            provider: Provider::Fastlane,
            connection_id: "fastlane:local".into(),
            name: "Mobile release".into(),
            description: None,
            metadata: HashMap::new(),
        };

        let page = unavailable_page(&target, None, "No provider history.")
            .expect("valid unavailable page");

        assert!(page.activities.is_empty());
        assert!(page.next_cursor.is_none());
        assert_eq!(page.issues.len(), 1);
        assert_eq!(page.issues[0].message, "No provider history.");
    }
}
