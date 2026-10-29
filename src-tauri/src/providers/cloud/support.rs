use chrono::{DateTime, SecondsFormat, Utc};
use url::Url;

use crate::{
    cli::CliError,
    models::{
        Account, ActivityStatus, Connection, ConnectionStatus, Discovery, Issue, Provider, Target,
    },
};

pub(super) const PAGE_SIZE: usize = 20;
pub(super) const MAX_TARGETS: usize = 200;

pub(super) fn missing_cli(account: &Account, provider: Provider, label: &str) -> Discovery {
    Discovery {
        connection: Connection::disconnected(
            &account.id,
            provider,
            ConnectionStatus::MissingCli,
            format!("{label} CLI was not found."),
        ),
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: Vec::new(),
    }
}

pub(super) fn failed_connection(
    account: &Account,
    provider: Provider,
    label: &str,
    version: Option<String>,
    error: CliError,
) -> Connection {
    Connection {
        kind: None,
        connection_id: account.id.clone(),
        provider,
        status: match error {
            CliError::SignedOut => ConnectionStatus::SignedOut,
            CliError::Unavailable => ConnectionStatus::MissingCli,
            _ => ConnectionStatus::Error,
        },
        account: None,
        message: Some(error.user_message(label)),
        cli_version: version,
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

pub(super) fn metadata<'a>(target: &'a Target, key: &str) -> Result<&'a str, CliError> {
    target
        .metadata
        .get(key)
        .map(String::as_str)
        .filter(|value| valid_argument(value))
        .ok_or(CliError::InvalidOutput)
}

pub(super) fn validate_target(target: &Target, provider: Provider) -> Result<(), CliError> {
    if target.provider != provider
        || !valid_argument(&target.id)
        || !valid_argument(&target.connection_id)
    {
        return Err(CliError::InvalidOutput);
    }
    Ok(())
}

pub(super) fn valid_argument(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

pub(super) fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(super) fn valid_gitlab_path(value: &str) -> bool {
    value.len() <= 512 && value.split('/').count() >= 2 && value.split('/').all(valid_slug)
}

pub(super) fn parse_timestamp(value: &str) -> Result<String, CliError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| {
            time.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        })
        .map_err(|_| CliError::InvalidOutput)
}

pub(super) fn terminal_time(status: ActivityStatus, value: &str) -> Option<String> {
    matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    )
    .then(|| value.to_owned())
}

pub(super) fn provider_url(domain: &str, segments: &[&str]) -> Result<String, CliError> {
    let mut url = Url::parse(&format!("https://{domain}/")).map_err(|_| CliError::InvalidOutput)?;
    url.path_segments_mut()
        .map_err(|_| CliError::InvalidOutput)?
        .extend(segments);
    Ok(url.into())
}

pub(super) fn capped_targets(
    provider: Provider,
    account: &Account,
    mut targets: Vec<Target>,
    mut issues: Vec<Issue>,
) -> (Vec<Target>, Vec<Issue>) {
    if targets.len() > MAX_TARGETS {
        targets.truncate(MAX_TARGETS);
        issues.push(issue(
            provider,
            &account.id,
            None,
            format!(
                "{} discovery reached the {MAX_TARGETS}-target safety limit; additional targets are not shown.",
                provider.as_str()
            ),
        ));
    }
    (targets, issues)
}
