use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Discovery, Issue, Ownership, Provider,
        Target,
    },
};
use chrono::{DateTime, Utc};
use futures::{stream, StreamExt};
use serde::Deserialize;

const PAGE_SIZE: usize = 20;

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let found = super::github::discover(cli, &account.id).await;
    Discovery {
        connection: crate::models::Connection {
            provider: Provider::GithubDeployments,
            ..found.connection
        },
        targets: found
            .targets
            .into_iter()
            .map(|t| Target {
                provider: Provider::GithubDeployments,
                ..t
            })
            .collect(),
        issues: found
            .issues
            .into_iter()
            .map(|i| Issue {
                provider: Provider::GithubDeployments,
                ..i
            })
            .collect(),
        aws_profiles: Vec::new(),
    }
}

#[derive(Deserialize)]
struct Creator {
    login: String,
}
#[derive(Deserialize)]
struct Deployment {
    id: u64,
    environment: String,
    description: Option<String>,
    #[serde(rename = "ref")]
    reference: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    creator: Option<Creator>,
}
#[derive(Deserialize)]
struct Status {
    state: String,
    created_at: DateTime<Utc>,
}

fn endpoint(repository: &str, cursor: Option<&str>) -> Result<(String, u32), CliError> {
    let segments: Vec<_> = repository.split('/').collect();
    if segments.len() != 2
        || segments.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
    {
        return Err(CliError::InvalidOutput);
    }
    let page = cursor
        .unwrap_or("1")
        .parse::<u32>()
        .map_err(|_| CliError::InvalidOutput)?;
    if !(1..=500).contains(&page) {
        return Err(CliError::InvalidOutput);
    }
    Ok((
        format!("repos/{repository}/deployments?per_page={PAGE_SIZE}&page={page}"),
        page,
    ))
}

async fn get<T: for<'de> Deserialize<'de>>(
    cli: &super::api::ApiClient,
    path: &str,
) -> Result<T, CliError> {
    cli.github_get(path).await
}

pub async fn page(
    invocation: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    super::github::verify_cli_identity(invocation).await?;
    let cli = super::api::ApiClient::github(invocation);
    let (path, number) = endpoint(&target.id, cursor)?;
    let records: Vec<Deployment> = get(&cli, &path).await?;
    let more = records.len() == PAGE_SIZE;
    let observations = stream::iter(records.into_iter().map(|record| {
        let api = cli.clone();
        async move {
            let path = format!(
                "repos/{}/deployments/{}/statuses?per_page=1",
                target.id, record.id
            );
            let statuses = get::<Vec<Status>>(&api, &path).await;
            (record, statuses)
        }
    }))
    .buffer_unordered(3)
    .collect::<Vec<_>>()
    .await;
    let mut activities = Vec::new();
    let mut failures = Vec::new();
    for (record, result) in observations {
        let status = match result {
            Ok(statuses) => statuses.into_iter().next(),
            Err(error) => {
                failures.push(("status", error));
                None
            }
        };
        activities.push(map(record, status, target, identity));
    }
    // One issue per cause, however many deployment statuses failed.
    let issues = super::grouped_probe_failures(&failures)
        .into_iter()
        .map(|(error, failed)| Issue {
            kind: error.issue_kind(),
            provider: target.provider,
            connection_id: Some(target.connection_id.clone()),
            target_id: Some(target.id.clone()),
            message: format!(
                "GitHub Deployments could not read {} deployment statuses: {}",
                failed.len(),
                error.user_message("GitHub Deployments")
            ),
        })
        .collect();
    Ok(ActivityPage {
        activities,
        issues,
        next_cursor: more.then(|| (number + 1).to_string()),
    })
}

fn map(
    deployment: Deployment,
    status: Option<Status>,
    target: &Target,
    identity: Option<&str>,
) -> Activity {
    let raw = status
        .as_ref()
        .map(|s| s.state.as_str())
        .unwrap_or("unknown");
    let normalized = match raw {
        "queued" | "pending" => ActivityStatus::Queued,
        "in_progress" => ActivityStatus::Running,
        "success" => ActivityStatus::Success,
        "failure" | "error" => ActivityStatus::Failed,
        "inactive" => ActivityStatus::Skipped,
        _ => ActivityStatus::Unknown,
    };
    let updated = status
        .as_ref()
        .map(|s| s.created_at)
        .unwrap_or(deployment.updated_at);
    let actor = deployment.creator.map(|c| c.login);
    let ownership = match (actor.as_deref(), identity) {
        (Some(a), Some(i)) if a.eq_ignore_ascii_case(i) => Ownership::Mine,
        (Some(_), Some(_)) => Ownership::Other,
        _ => Ownership::Unknown,
    };
    Activity {
        id: format!("github-deployments:{}:{}", target.id, deployment.id),
        provider: target.provider,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: deployment.environment.clone(),
        title: deployment
            .description
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{} deployment", deployment.environment)),
        project: target.name.clone(),
        branch: Some(deployment.reference),
        url: format!("https://github.com/{}/deployments", target.id),
        status: normalized,
        raw_status: raw.to_owned(),
        actor,
        ownership,
        started_at: deployment.created_at.to_rfc3339(),
        finished_at: matches!(
            normalized,
            ActivityStatus::Success | ActivityStatus::Failed | ActivityStatus::Skipped
        )
        .then(|| updated.to_rfc3339()),
        updated_at: updated.to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_endpoints_are_bounded_and_reject_injection() {
        assert_eq!(
            endpoint("example/rocket", None).unwrap().0,
            "repos/example/rocket/deployments?per_page=20&page=1"
        );
        assert!(endpoint("example/../secrets", None).is_err());
        assert!(endpoint("example/rocket", Some("2&evil=1")).is_err());
        assert!(endpoint("example/rocket", Some("0")).is_err());
    }
}
