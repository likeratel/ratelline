use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery,
        Ownership, Provider, Target,
    },
};

use super::support::{
    capped_targets, discovery_from_connection, failed_connection, issue, metadata, missing_cli,
    parse_timestamp, provider_url, terminal_time, valid_gitlab_path, validate_target, PAGE_SIZE,
};

const PROVIDER: Provider = Provider::Gitlab;
const LABEL: &str = "GitLab";
const HOST: &str = "gitlab.com";
const DISCOVERY_PAGE_SIZE: usize = 100;

#[derive(Deserialize)]
struct User {
    username: String,
}

#[derive(Deserialize)]
struct Project {
    id: u64,
    name: String,
    path_with_namespace: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct Deployment {
    id: u64,
    status: String,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
    sha: String,
    created_at: String,
    updated_at: String,
    user: Option<DeploymentUser>,
    environment: Environment,
    deployable: Option<Deployable>,
}

#[derive(Deserialize)]
struct DeploymentUser {
    username: String,
}

#[derive(Deserialize)]
struct Environment {
    name: String,
}

#[derive(Deserialize)]
struct Deployable {
    name: Option<String>,
    finished_at: Option<String>,
    commit: Option<Commit>,
    pipeline: Option<Pipeline>,
}

#[derive(Deserialize)]
struct Commit {
    title: String,
}

#[derive(Deserialize)]
struct Pipeline {
    id: u64,
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let Some(cli) = cli else {
        return missing_cli(account, PROVIDER, LABEL);
    };
    let connection = connection(Some(cli), account).await;
    if connection.status != ConnectionStatus::Connected {
        return discovery_from_connection(connection);
    }

    let projects: Vec<Project> = match cli.json(&project_args()).await {
        Ok(projects) => projects,
        Err(error) => {
            return discovery_from_connection(failed_connection(
                account,
                PROVIDER,
                LABEL,
                connection.cli_version,
                error,
            ))
        }
    };
    let mut issues = Vec::new();
    if projects.len() == DISCOVERY_PAGE_SIZE {
        issues.push(issue(
            PROVIDER,
            &account.id,
            None,
            "GitLab project discovery is limited to the 100 most recently active projects.",
        ));
    }
    let targets: Vec<Target> = projects
        .into_iter()
        .filter_map(|project| match project_target(project, &account.id) {
            Ok(target) => Some(target),
            Err(_) => {
                issues.push(issue(
                    PROVIDER,
                    &account.id,
                    None,
                    "GitLab returned a project with an invalid identifier; it was skipped.",
                ));
                None
            }
        })
        .collect();
    let targets = targets
        .into_iter()
        .flat_map(|target| vec![super::gitlab_pipelines::target(&target), target])
        .collect();
    let (targets, issues) = capped_targets(PROVIDER, account, targets, issues);
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    let Some(cli) = cli else {
        return missing_cli(account, PROVIDER, LABEL).connection;
    };
    let version = cli.version().await;
    match cli
        .json::<User>(&["api", "--hostname", HOST, "user"])
        .await
    {
        Ok(user) if !user.username.trim().is_empty() => Connection {
            kind: None,
            connection_id: account.id.clone(),
            provider: PROVIDER,
            status: ConnectionStatus::Connected,
            account: Some(user.username),
            message: Some(
                "GitLab.com deployments and CI pipelines are supported. Self-managed GitLab hosts are not auto-discovered because ratelline cannot yet bind and validate a per-account host."
                    .into(),
            ),
            cli_version: version,
        },
        Ok(_) => failed_connection(
            account,
            PROVIDER,
            LABEL,
            version,
            CliError::InvalidOutput,
        ),
        Err(error) => failed_connection(account, PROVIDER, LABEL, version, error),
    }
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    validate_target(target, PROVIDER)?;
    let project_id = metadata(target, "project_id")?;
    let path = metadata(target, "path_with_namespace")?;
    if project_id.parse::<u64>().is_err() || !valid_gitlab_path(path) {
        return Err(CliError::InvalidOutput);
    }
    let page_number = parse_cursor(cursor)?;
    if target
        .metadata
        .get("kind")
        .is_some_and(|kind| kind == "pipelines")
    {
        return super::gitlab_pipelines::page(cli, target, project_id, path, page_number).await;
    }
    let args = deployment_args(project_id, page_number);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let deployments: Vec<Deployment> = cli.json(&arg_refs).await?;
    let mut issues = Vec::new();
    let activities = deployments
        .iter()
        .map(|deployment| map_deployment(deployment, target, path, &mut issues))
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = (deployments.len() == PAGE_SIZE).then(|| (page_number + 1).to_string());
    Ok(ActivityPage {
        activities,
        next_cursor,
        issues,
    })
}

fn project_args() -> [&'static str; 4] {
    [
        "api",
        "--hostname",
        HOST,
        "projects?membership=true&archived=false&simple=true&order_by=last_activity_at&sort=desc&per_page=100&page=1",
    ]
}

fn deployment_args(project_id: &str, page: u32) -> Vec<String> {
    vec![
        "api".into(),
        "--hostname".into(),
        HOST.into(),
        format!(
            "projects/{project_id}/deployments?order_by=created_at&sort=desc&per_page={PAGE_SIZE}&page={page}"
        ),
    ]
}

fn parse_cursor(cursor: Option<&str>) -> Result<u32, CliError> {
    match cursor {
        None => Ok(1),
        Some(value) => value
            .parse::<u32>()
            .ok()
            .filter(|page| (2..=10_000).contains(page))
            .ok_or(CliError::InvalidOutput),
    }
}

fn project_target(project: Project, connection_id: &str) -> Result<Target, CliError> {
    if project.id == 0 || !valid_gitlab_path(&project.path_with_namespace) {
        return Err(CliError::InvalidOutput);
    }
    Ok(Target {
        id: format!("gitlab:{}", project.id),
        provider: PROVIDER,
        connection_id: connection_id.into(),
        name: project.name,
        description: project.description,
        metadata: HashMap::from([
            ("project_id".into(), project.id.to_string()),
            ("path_with_namespace".into(), project.path_with_namespace),
        ]),
    })
}

fn map_deployment(
    deployment: &Deployment,
    target: &Target,
    path: &str,
    issues: &mut Vec<crate::models::Issue>,
) -> Result<Activity, CliError> {
    if deployment.id == 0
        || deployment.sha.trim().is_empty()
        || deployment.environment.name.trim().is_empty()
    {
        return Err(CliError::InvalidOutput);
    }
    let started_at = parse_timestamp(&deployment.created_at)?;
    let updated_at = parse_timestamp(&deployment.updated_at)?;
    let status = gitlab_status(&deployment.status);
    if status == ActivityStatus::Unknown {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            format!(
                "GitLab deployment {} has an unrecognized status '{}'.",
                deployment.id, deployment.status
            ),
        ));
    }
    let explicit_finished = deployment
        .deployable
        .as_ref()
        .and_then(|deployable| deployable.finished_at.as_deref())
        .map(parse_timestamp)
        .transpose()?;
    let finished_at = explicit_finished.or_else(|| terminal_time(status, &updated_at));
    let pipeline_suffix = deployment
        .deployable
        .as_ref()
        .and_then(|deployable| deployable.pipeline.as_ref())
        .map(|pipeline| format!(" (pipeline #{})", pipeline.id))
        .unwrap_or_default();
    let title = deployment
        .deployable
        .as_ref()
        .and_then(|deployable| deployable.commit.as_ref())
        .map(|commit| commit.title.trim())
        .filter(|title| !title.is_empty())
        .map(|title| format!("{title}{pipeline_suffix}"))
        .unwrap_or_else(|| format!("Deploy to {}{pipeline_suffix}", deployment.environment.name));
    Ok(Activity {
        id: format!(
            "gitlab:{}:{}",
            metadata(target, "project_id")?,
            deployment.id
        ),
        provider: PROVIDER,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: deployment
            .deployable
            .as_ref()
            .and_then(|job| crate::providers::execution_name(job.name.as_deref())),
        kind: format!("deployment:{}", deployment.environment.name),
        title,
        project: path.into(),
        branch: deployment.ref_name.clone(),
        url: deployment_url(path, deployment.id)?,
        status,
        raw_status: deployment.status.clone(),
        actor: deployment.user.as_ref().map(|user| user.username.clone()),
        ownership: Ownership::Unknown,
        started_at,
        finished_at,
        updated_at,
    })
}

fn deployment_url(path: &str, deployment_id: u64) -> Result<String, CliError> {
    let deployment_id = deployment_id.to_string();
    let mut segments = path.split('/').collect::<Vec<_>>();
    segments.extend(["-", "deployments", deployment_id.as_str()]);
    provider_url(HOST, &segments)
}

fn gitlab_status(status: &str) -> ActivityStatus {
    match status {
        "created" | "pending" => ActivityStatus::Queued,
        "running" => ActivityStatus::Running,
        "blocked" => ActivityStatus::Waiting,
        "success" => ActivityStatus::Success,
        "failed" => ActivityStatus::Failed,
        "canceled" => ActivityStatus::Canceled,
        "skipped" => ActivityStatus::Skipped,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "gitlab:42".into(),
            provider: PROVIDER,
            connection_id: "gitlab:local".into(),
            name: "rocket".into(),
            description: None,
            metadata: HashMap::from([
                ("project_id".into(), "42".into()),
                ("path_with_namespace".into(), "example-org/rocket".into()),
            ]),
        }
    }

    #[test]
    fn parses_official_deployment_shape_and_keeps_actor_unknown() {
        let fixture = r#"[{
          "id": 41, "status": "success", "ref": "main", "sha": "abc123",
          "created_at": "2026-09-28T07:36:40.222Z", "updated_at": "2026-09-28T07:38:12.414Z",
          "user": {"username": "example-user"}, "environment": {"name": "production"},
          "deployable": {"name": "deploy-production", "finished_at": "2026-09-28T07:38:10Z", "commit": {"title": "Ship rocket"}, "pipeline": {"id": 36}}
        } ]"#;
        let deployments: Vec<Deployment> = serde_json::from_str(fixture).expect("fixture");
        let mut issues = Vec::new();
        let activity = map_deployment(
            &deployments[0],
            &target(),
            "example-org/rocket",
            &mut issues,
        )
        .expect("activity");
        assert_eq!(activity.status, ActivityStatus::Success);
        assert_eq!(activity.actor.as_deref(), Some("example-user"));
        assert_eq!(activity.ownership, Ownership::Unknown);
        assert_eq!(activity.branch.as_deref(), Some("main"));
        assert!(!target().metadata.contains_key("current_user"));
        assert_eq!(activity.title, "Ship rocket (pipeline #36)");
        assert_eq!(activity.workflow_name.as_deref(), Some("deploy-production"));
        assert!(activity.url.starts_with("https://gitlab.com/"));
        assert!(issues.is_empty());
    }

    #[test]
    fn request_and_cursor_are_explicit_and_validated() {
        let args = deployment_args("42", 3);
        assert_eq!(args[1..3], ["--hostname", "gitlab.com"]);
        assert!(args[3].contains("per_page=20&page=3"));
        assert_eq!(parse_cursor(Some("2")).expect("cursor"), 2);
        assert!(parse_cursor(Some("../2")).is_err());
        assert!(parse_cursor(Some("1")).is_err());
    }

    #[test]
    fn invalid_dates_and_unknown_states_are_not_silent() {
        assert!(parse_timestamp("yesterday").is_err());
        assert_eq!(gitlab_status("new-state"), ActivityStatus::Unknown);
    }
}
