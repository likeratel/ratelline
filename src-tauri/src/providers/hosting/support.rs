use crate::{
    cli::CliError,
    models::{Account, Connection, ConnectionStatus, Discovery, Issue, Provider, Target},
};

pub(super) fn missing_cli(account: &Account, label: &str, executable: &str) -> Connection {
    Connection::disconnected(
        &account.id,
        account.provider,
        ConnectionStatus::MissingCli,
        format!("{label} CLI (`{executable}`) was not found."),
    )
}

pub(super) fn failed_connection(
    account: &Account,
    label: &str,
    error: CliError,
    cli_version: Option<String>,
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
        message: Some(error.user_message(label)),
        cli_version,
    }
}

pub(super) fn discovery_from_connection(connection: Connection) -> Discovery {
    Discovery {
        connection,
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: Vec::new(),
    }
}

pub(super) fn failed_discovery(
    account: &Account,
    label: &str,
    error: CliError,
    cli_version: Option<String>,
) -> Discovery {
    discovery_from_connection(failed_connection(account, label, error, cli_version))
}

pub(super) fn issue(
    provider: Provider,
    connection_id: &str,
    target_id: Option<&str>,
    message: impl Into<String>,
) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider,
        connection_id: Some(connection_id.to_owned()),
        message: message.into(),
        target_id: target_id.map(str::to_owned),
    }
}

pub(super) fn validate_id(value: &str) -> Result<(), CliError> {
    if !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
    {
        Ok(())
    } else {
        Err(CliError::InvalidOutput)
    }
}

pub(super) fn validate_text(value: &str, max_len: usize) -> Result<(), CliError> {
    if !value.trim().is_empty() && value.len() <= max_len && !value.contains('\0') {
        Ok(())
    } else {
        Err(CliError::InvalidOutput)
    }
}

pub(super) fn metadata<'a>(target: &'a Target, key: &str) -> Result<&'a str, CliError> {
    let value = target
        .metadata
        .get(key)
        .map(String::as_str)
        .ok_or(CliError::InvalidOutput)?;
    validate_id(value)?;
    Ok(value)
}

pub(super) fn reject_cursor(cursor: Option<&str>) -> Result<(), CliError> {
    if cursor.is_some() {
        Err(CliError::InvalidOutput)
    } else {
        Ok(())
    }
}

pub(super) fn path_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub(super) fn unsupported_connection(account: &Account) -> Connection {
    Connection::disconnected(
        &account.id,
        account.provider,
        ConnectionStatus::Error,
        "The selected account is not a hosting CLI provider.",
    )
}

pub(super) fn unsupported_discovery(account: &Account) -> Discovery {
    discovery_from_connection(unsupported_connection(account))
}
