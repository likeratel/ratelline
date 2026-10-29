mod parsing;
#[cfg(test)]
mod tests;

use super::{failed_connection, missing_cli};
use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, ActivityPage, Connection, ConnectionStatus, Discovery, Issue, Provider, Target,
    },
};
use futures::{stream, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    time::Duration,
};

const PAGE_SIZE: usize = 10;
const MAX_OFFSET: usize = 10_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const IDENTITY_RESERVE: Duration = Duration::from_secs(10);
const UPDATE_DETAIL_CONCURRENCY: usize = 3;
const ACTIVE_BUILD_LIMIT: usize = 50;
const ACTIVE_BUILD_STATUSES: [&str; 4] = ["new", "in-queue", "in-progress", "pending-cancel"];

pub async fn identity(cli: &CliInvocation) -> Result<String, CliError> {
    let output = cli.run(&["account:view"], Duration::from_secs(15)).await?;
    let identity = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .filter(|line| line.len() <= 512 && !line.chars().any(char::is_control))
        .ok_or(CliError::InvalidOutput)?;
    if cli
        .expected_identity()
        .is_some_and(|expected| expected != identity)
    {
        return Err(CliError::SignedOut);
    }
    Ok(identity.to_owned())
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    if account.provider != Provider::Eas {
        return failed_connection(account, None, "EAS", CliError::InvalidOutput);
    }
    let Some(cli) = cli else {
        return missing_cli(account, "EAS");
    };
    match identity(cli).await {
        Ok(identity) => Connection {
            kind: None,
            connection_id: account.id.clone(),
            provider: Provider::Eas,
            status: ConnectionStatus::Connected,
            account: Some(identity),
            message: None,
            cli_version: cli.version().await,
        },
        Err(error) => failed_connection(account, cli.version().await, "EAS", error),
    }
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    Discovery {
        connection: connection(cli, account).await,
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: Vec::new(),
    }
}

/// Only app-created static configuration is ever evaluated by EAS.
struct ProjectContext {
    directory: tempfile::TempDir,
    invocation: CliInvocation,
}
impl ProjectContext {
    fn new(
        cli: &CliInvocation,
        project_id: &str,
        owner: &str,
        slug: &str,
    ) -> Result<Self, CliError> {
        validate_project(project_id, owner, slug)?;
        let directory = tempfile::Builder::new()
            .prefix("ratelline-eas-")
            .tempdir()
            .map_err(|_| CliError::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .map_err(|_| CliError::Unavailable)?;
        }
        std::fs::write(directory.path().join("package.json"), b"{\"private\":true}")
            .map_err(|_| CliError::Unavailable)?;
        let config = json!({"expo":{"name":"Ratelline monitor","owner":owner,"slug":slug,"extra":{"eas":{"projectId":project_id}}}});
        std::fs::write(directory.path().join("app.json"), config.to_string())
            .map_err(|_| CliError::Unavailable)?;
        let invocation = cli.for_eas_project(directory.path());
        Ok(Self {
            directory,
            invocation,
        })
    }
}

fn validate_project(project_id: &str, owner: &str, slug: &str) -> Result<(), CliError> {
    if uuid::Uuid::parse_str(project_id).is_err() || !safe_slug(owner) || !safe_slug(slug) {
        return Err(CliError::InvalidOutput);
    }
    Ok(())
}
fn safe_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        && value.as_bytes()[0].is_ascii_alphanumeric()
}

pub async fn register_project(
    cli: &CliInvocation,
    account: &Account,
    project_id: &str,
    owner: &str,
    slug: &str,
) -> Result<Vec<Target>, String> {
    if account.provider != Provider::Eas {
        return Err("Select an EAS connection.".into());
    }
    let project_id = uuid::Uuid::parse_str(project_id)
        .map_err(|_| "Enter a valid EAS project UUID.")?
        .to_string();
    let context = ProjectContext::new(cli, &project_id, owner, slug)
        .map_err(|_| "Enter a valid EAS project ID, owner and project slug.")?;
    let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
    let query_deadline = deadline - IDENTITY_RESERVE;
    let before = identity(cli).await.map_err(|e| e.user_message("EAS"))?;
    // ProjectId context resolves the UUID remotely and rejects mismatched owner/slug.
    let _: Vec<Value> = before_deadline(
        query_deadline,
        context.invocation.json(&[
            "build:list",
            "--json",
            "--non-interactive",
            "--limit",
            "1",
            "--offset",
            "0",
        ]),
    )
    .await
    .map_err(|e| e.user_message("EAS"))?;
    if before
        != before_deadline(deadline, identity(cli))
            .await
            .map_err(|e| e.user_message("EAS"))?
    {
        return Err("The EAS account changed during verification. Reconnect and try again.".into());
    }
    Ok([("build", "Builds"), ("update", "Updates")]
        .into_iter()
        .map(|(kind, label)| Target {
            id: format!("{project_id}:{kind}"),
            provider: Provider::Eas,
            connection_id: account.id.clone(),
            name: format!("@{owner}/{slug} · {label}"),
            description: Some(
                if kind == "build" {
                    "EAS cloud build results"
                } else {
                    "Published EAS updates; failed publish attempts are not available"
                }
                .into(),
            ),
            metadata: HashMap::from([
                ("projectId".into(), project_id.clone()),
                ("owner".into(), owner.into()),
                ("slug".into(), slug.into()),
                ("kind".into(), kind.into()),
            ]),
        })
        .collect())
}

fn target_context(cli: &CliInvocation, target: &Target) -> Result<ProjectContext, CliError> {
    target.validate().map_err(|_| CliError::InvalidOutput)?;
    let get = |key: &str| {
        target
            .metadata
            .get(key)
            .map(String::as_str)
            .ok_or(CliError::InvalidOutput)
    };
    let kind = get("kind")?;
    if target.provider != Provider::Eas
        || !matches!(kind, "build" | "update")
        || target.id != format!("{}:{kind}", get("projectId")?)
    {
        return Err(CliError::InvalidOutput);
    }
    ProjectContext::new(cli, get("projectId")?, get("owner")?, get("slug")?)
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
    let query_deadline = deadline - IDENTITY_RESERVE;
    let context = target_context(cli, target)?;
    let before = identity(cli).await?;
    let offset = parse_cursor(target, cursor)?;
    let offset_arg = offset.to_string();
    let limit = PAGE_SIZE.to_string();
    let kind = target.metadata["kind"].as_str();
    let mut result = if kind == "build" {
        let rows: Vec<Value> = before_deadline(
            query_deadline,
            context.invocation.json(&[
                "build:list",
                "--json",
                "--non-interactive",
                "--limit",
                &limit,
                "--offset",
                &offset_arg,
            ]),
        )
        .await?;
        let count = rows.len();
        if count > PAGE_SIZE {
            return Err(CliError::InvalidOutput);
        }
        let mut page = ActivityPage::default();
        for row in &rows {
            match parsing::build(target, row) {
                Ok(activity) => page.activities.push(activity),
                Err(_) => page
                    .issues
                    .push(issue(target, "An EAS build record could not be read.")),
            }
        }
        page.next_cursor = next_cursor(target, offset, count);
        page
    } else {
        update_page(&context.invocation, target, offset, query_deadline).await?
    };
    if before != before_deadline(deadline, identity(cli)).await? {
        return Err(CliError::SignedOut);
    }
    if offset + PAGE_SIZE >= MAX_OFFSET
        && result.next_cursor.is_none()
        && result.activities.len() >= PAGE_SIZE
    {
        result.issues.push(issue(
            target,
            "EAS history reached the supported history limit.",
        ));
    }
    // TempDir stays alive until all child processes have completed or been cancelled.
    drop(context.directory);
    Ok(result)
}

async fn update_page(
    cli: &CliInvocation,
    target: &Target,
    offset: usize,
    deadline: tokio::time::Instant,
) -> Result<ActivityPage, CliError> {
    let list: Value = before_deadline(
        deadline,
        cli.json(&[
            "update:list",
            "--all",
            "--json",
            "--non-interactive",
            "--limit",
            &PAGE_SIZE.to_string(),
            "--offset",
            &offset.to_string(),
        ]),
    )
    .await?;
    let rows = list
        .get("currentPage")
        .and_then(Value::as_array)
        .ok_or(CliError::InvalidOutput)?;
    if rows.len() > PAGE_SIZE {
        return Err(CliError::InvalidOutput);
    }
    let row_count = rows.len();
    let requests = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let group = row
                .get("group")
                .and_then(Value::as_str)
                .ok_or(CliError::InvalidOutput)?;
            uuid::Uuid::parse_str(group).map_err(|_| CliError::InvalidOutput)?;
            Ok((index, group.to_owned()))
        })
        .collect::<Vec<Result<_, CliError>>>();
    let mut results = stream::iter(requests)
        .map(|request| async move {
            let (index, group) = request?;
            let details: Vec<Value> = cli.json(&["update:view", &group, "--json"]).await?;
            parsing::update(target, &group, &details).map(|activity| (index, activity))
        })
        .buffer_unordered(UPDATE_DETAIL_CONCURRENCY);
    let mut page = ActivityPage {
        next_cursor: next_cursor(target, offset, row_count),
        ..Default::default()
    };
    let mut ordered_activities = Vec::with_capacity(row_count);
    loop {
        match tokio::time::timeout_at(deadline, results.next()).await {
            Ok(Some(Ok(activity))) => ordered_activities.push(activity),
            Ok(Some(Err(_))) => page.issues.push(issue(
                target,
                "An EAS update could not be read. Other results were retained.",
            )),
            Ok(None) => break,
            Err(_) => {
                page.issues.push(issue(
                    target,
                    "EAS update details timed out. Completed results were retained.",
                ));
                break;
            }
        }
    }
    drop(results);
    ordered_activities.sort_by_key(|(index, _)| *index);
    page.activities = ordered_activities
        .into_iter()
        .map(|(_, activity)| activity)
        .collect();
    Ok(page)
}

async fn before_deadline<T>(
    deadline: tokio::time::Instant,
    future: impl Future<Output = Result<T, CliError>>,
) -> Result<T, CliError> {
    tokio::time::timeout_at(deadline, future)
        .await
        .map_err(|_| CliError::TimedOut)?
}

fn issue(target: &Target, message: &str) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider: Provider::Eas,
        connection_id: Some(target.connection_id.clone()),
        target_id: Some(target.id.clone()),
        message: message.into(),
    }
}
fn parse_cursor(target: &Target, cursor: Option<&str>) -> Result<usize, CliError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let prefix = format!("{}:", target.id);
    let offset = cursor
        .strip_prefix(&prefix)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0 && *value < MAX_OFFSET && value.is_multiple_of(PAGE_SIZE));
    offset.ok_or(CliError::InvalidOutput)
}
fn next_cursor(target: &Target, offset: usize, count: usize) -> Option<String> {
    (count == PAGE_SIZE && offset + PAGE_SIZE < MAX_OFFSET)
        .then(|| format!("{}:{}", target.id, offset + PAGE_SIZE))
}

pub async fn active(cli: &CliInvocation, target: &Target) -> Result<ActivityPage, CliError> {
    let context = target_context(cli, target)?;
    if target.metadata["kind"] == "update" {
        return Ok(ActivityPage::default());
    }
    let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
    let query_deadline = deadline - IDENTITY_RESERVE;
    let before = identity(cli).await?;
    let mut results = stream::iter(ACTIVE_BUILD_STATUSES.map(str::to_owned))
        .map(|status| {
            let invocation = &context.invocation;
            async move {
                let rows = invocation
                    .json::<Vec<Value>>(&[
                        "build:list",
                        "--json",
                        "--non-interactive",
                        "--status",
                        &status,
                        "--limit",
                        "50",
                        "--offset",
                        "0",
                    ])
                    .await;
                (status, rows)
            }
        })
        .buffer_unordered(4);
    let mut page = ActivityPage::default();
    let mut seen = HashSet::new();
    loop {
        let result = match tokio::time::timeout_at(query_deadline, results.next()).await {
            Ok(Some(result)) => result,
            Ok(None) => break,
            Err(_) => {
                page.issues.push(issue(
                    target,
                    "Some EAS active build checks timed out. Completed results were retained.",
                ));
                break;
            }
        };
        let (status, result) = result;
        match result {
            Ok(rows) if rows.len() <= ACTIVE_BUILD_LIMIT => {
                if rows.len() == ACTIVE_BUILD_LIMIT {
                    page.issues.push(issue(
                        target,
                        "EAS active builds reached the 50-record limit for a status.",
                    ));
                }
                for row in rows {
                    match parsing::build(target, &row) {
                        Ok(activity) if seen.insert(activity.id.clone()) => {
                            page.activities.push(activity)
                        }
                        Ok(_) => {}
                        Err(_) => page
                            .issues
                            .push(issue(target, "An EAS active build could not be read.")),
                    }
                }
            }
            _ => page.issues.push(issue(
                target,
                &format!("EAS active {status} builds could not be checked."),
            )),
        }
    }
    drop(results);
    if before != before_deadline(deadline, identity(cli)).await? {
        return Err(CliError::SignedOut);
    }
    Ok(page)
}
