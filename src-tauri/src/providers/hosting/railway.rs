use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery,
        Ownership, Provider, Target,
    },
};

use super::support;

const LABEL: &str = "Railway";
const PAGE_SIZE: usize = 20;
const DISCOVERY_LIMIT: usize = 500;

#[derive(Deserialize)]
struct RailwayUser {
    name: Option<String>,
    email: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRow {
    workspace: Named,
    id: String,
    name: String,
    environments: EdgeList<EnvironmentRow>,
}

#[derive(Deserialize)]
struct Named {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct EdgeList<T> {
    edges: Vec<Edge<T>>,
}

#[derive(Deserialize)]
struct Edge<T> {
    node: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentRow {
    id: String,
    name: String,
    can_access: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceRow {
    id: String,
    name: String,
    source: Option<ServiceSource>,
}

#[derive(Deserialize)]
struct ServiceSource {
    repo: Option<String>,
    image: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentRow {
    id: String,
    status: String,
    created_at: DateTime<Utc>,
    meta: Option<serde_json::Value>,
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    let Some(cli) = cli else {
        return support::missing_cli(account, LABEL, "railway");
    };
    let version = cli.version().await;
    match cli.json::<RailwayUser>(&["whoami", "--json"]).await {
        Ok(user)
            if support::validate_text(&user.email, 512).is_ok()
                && user
                    .name
                    .as_deref()
                    .is_none_or(|name| support::validate_text(name, 512).is_ok()) =>
        {
            Connection {
                kind: None,
                connection_id: account.id.clone(),
                provider: Provider::Railway,
                status: ConnectionStatus::Connected,
                account: Some(
                    user.name
                        .filter(|name| !name.trim().is_empty())
                        .map(|name| format!("{name} ({})", user.email))
                        .unwrap_or(user.email),
                ),
                message: None,
                cli_version: version,
            }
        }
        Ok(_) => support::failed_connection(account, LABEL, CliError::InvalidOutput, version),
        Err(error) => support::failed_connection(account, LABEL, error, version),
    }
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let connection = connection(cli, account).await;
    let Some(cli) = cli.filter(|_| connection.status == ConnectionStatus::Connected) else {
        return support::discovery_from_connection(connection);
    };
    let projects = match cli
        .json::<Vec<ProjectRow>>(&["project", "list", "--json"])
        .await
    {
        Ok(projects) => projects,
        Err(error) => {
            return support::failed_discovery(account, LABEL, error, connection.cli_version);
        }
    };

    let mut targets = Vec::new();
    let mut issues = Vec::new();
    'projects: for project in projects {
        if validate_project(&project).is_err() {
            issues.push(support::issue(
                Provider::Railway,
                &account.id,
                None,
                "Railway returned invalid project or workspace identifiers.",
            ));
            continue;
        }
        for environment in project.environments.edges.iter().map(|edge| &edge.node) {
            if !environment.can_access {
                issues.push(support::issue(
                    Provider::Railway,
                    &account.id,
                    None,
                    format!(
                        "Railway environment {} / {} is restricted and was not discovered.",
                        project.name, environment.name
                    ),
                ));
                continue;
            }
            if support::validate_id(&environment.id).is_err() {
                issues.push(support::issue(
                    Provider::Railway,
                    &account.id,
                    None,
                    "Railway returned an invalid environment identifier.",
                ));
                continue;
            }
            match list_services(cli, &project.id, &environment.id).await {
                Ok(services) => {
                    for service in services {
                        if support::validate_id(&service.id).is_err() {
                            issues.push(support::issue(
                                Provider::Railway,
                                &account.id,
                                None,
                                "Railway returned an invalid service identifier.",
                            ));
                            continue;
                        }
                        match service_target(&project, environment, service, &account.id) {
                            Ok(target) => targets.push(target),
                            Err(_) => {
                                issues.push(support::issue(
                                    Provider::Railway,
                                    &account.id,
                                    None,
                                    "Railway returned invalid service metadata.",
                                ));
                                continue;
                            }
                        }
                        if targets.len() == DISCOVERY_LIMIT {
                            issues.push(support::issue(
                                Provider::Railway,
                                &account.id,
                                None,
                                format!(
                                    "Railway discovery reached the {DISCOVERY_LIMIT}-target safety limit."
                                ),
                            ));
                            break 'projects;
                        }
                    }
                }
                Err(error) => issues.push(support::issue(
                    Provider::Railway,
                    &account.id,
                    None,
                    format!(
                        "Railway could not list services for {} / {}: {}",
                        project.name,
                        environment.name,
                        error.user_message(LABEL)
                    ),
                )),
            }
        }
    }

    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

async fn list_services(
    cli: &CliInvocation,
    project_id: &str,
    environment_id: &str,
) -> Result<Vec<ServiceRow>, CliError> {
    support::validate_id(project_id)?;
    support::validate_id(environment_id)?;
    cli.json(&[
        "service",
        "list",
        "--project",
        project_id,
        "--environment",
        environment_id,
        "--json",
    ])
    .await
}

fn validate_project(project: &ProjectRow) -> Result<(), CliError> {
    support::validate_id(&project.id)?;
    support::validate_id(&project.workspace.id)?;
    support::validate_text(&project.name, 256)?;
    support::validate_text(&project.workspace.name, 256)
}

fn service_target(
    project: &ProjectRow,
    environment: &EnvironmentRow,
    service: ServiceRow,
    connection_id: &str,
) -> Result<Target, CliError> {
    support::validate_id(&service.id)?;
    support::validate_text(&environment.name, 256)?;
    support::validate_text(&service.name, 256)?;
    let source = service.source.as_ref().and_then(|source| {
        source
            .repo
            .as_deref()
            .or(source.image.as_deref())
            .filter(|value| value.len() <= 512 && !value.contains('\0'))
            .map(str::to_owned)
    });
    let description = Some(match source {
        Some(source) => format!(
            "{} · {} · {} · {source}",
            project.workspace.name, project.name, environment.name
        ),
        None => format!(
            "{} · {} · {}",
            project.workspace.name, project.name, environment.name
        ),
    });
    Ok(Target {
        id: format!("{}:{}:{}", project.id, environment.id, service.id),
        provider: Provider::Railway,
        connection_id: connection_id.to_owned(),
        name: service.name,
        description,
        metadata: HashMap::from([
            ("projectId".into(), project.id.clone()),
            ("environmentId".into(), environment.id.clone()),
            ("environmentName".into(), environment.name.clone()),
            ("serviceId".into(), service.id),
            ("workspaceId".into(), project.workspace.id.clone()),
        ]),
    })
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    support::reject_cursor(cursor)?;
    let project_id = support::metadata(target, "projectId")?;
    let environment_id = support::metadata(target, "environmentId")?;
    let service_id = support::metadata(target, "serviceId")?;
    let args = deployment_args(project_id, environment_id, service_id);
    let rows: Vec<DeploymentRow> = cli.json(&args).await?;
    deployment_page(rows, target)
}

fn deployment_args<'a>(
    project_id: &'a str,
    environment_id: &'a str,
    service_id: &'a str,
) -> [&'a str; 11] {
    [
        "deployment",
        "list",
        "--project",
        project_id,
        "--environment",
        environment_id,
        "--service",
        service_id,
        "--limit",
        "21",
        "--json",
    ]
}

fn deployment_page(
    mut rows: Vec<DeploymentRow>,
    target: &Target,
) -> Result<ActivityPage, CliError> {
    let truncated = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let activities = rows
        .into_iter()
        .map(|row| map_deployment(row, target))
        .collect::<Result<Vec<_>, _>>()?;
    let issues = truncated
        .then(|| {
            support::issue(
                Provider::Railway,
                &target.connection_id,
                Some(&target.id),
                "Railway returned more than 20 recent deployments, but its CLI does not expose a deployment cursor. Older history is unavailable.",
            )
        })
        .into_iter()
        .collect();
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

fn map_deployment(row: DeploymentRow, target: &Target) -> Result<Activity, CliError> {
    support::validate_id(&row.id)?;
    support::validate_text(&row.status, 128)?;
    let branch = row
        .meta
        .as_ref()
        .and_then(|meta| meta.get("branch"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let title = row
        .meta
        .as_ref()
        .and_then(|meta| meta.get("commitMessage"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{} deployment", target.name));
    let project_id = support::metadata(target, "projectId")?;
    let environment_id = support::metadata(target, "environmentId")?;
    let service_id = support::metadata(target, "serviceId")?;
    let status = railway_status(&row.status);
    Ok(Activity {
        id: format!("railway:{}", row.id),
        provider: Provider::Railway,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: target
            .metadata
            .get("environmentName")
            .cloned()
            .unwrap_or_else(|| "deployment".into()),
        title,
        project: target.name.clone(),
        branch,
        url: format!(
            "https://railway.com/project/{}/service/{}?environmentId={}",
            support::path_segment(project_id),
            support::path_segment(service_id),
            support::path_segment(environment_id)
        ),
        status,
        raw_status: row.status,
        actor: None,
        ownership: Ownership::Unknown,
        started_at: row.created_at.to_rfc3339(),
        finished_at: None,
        updated_at: row.created_at.to_rfc3339(),
    })
}

fn railway_status(status: &str) -> ActivityStatus {
    match status {
        "QUEUED" | "INITIALIZING" => ActivityStatus::Queued,
        "BUILDING" | "DEPLOYING" | "REMOVING" => ActivityStatus::Running,
        "WAITING" | "NEEDS_APPROVAL" | "SLEEPING" => ActivityStatus::Waiting,
        "SUCCESS" => ActivityStatus::Success,
        "FAILED" | "CRASHED" => ActivityStatus::Failed,
        "REMOVED" => ActivityStatus::Canceled,
        "SKIPPED" => ActivityStatus::Skipped,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "project-1:environment-1:service-1".into(),
            provider: Provider::Railway,
            connection_id: "railway:local".into(),
            name: "api".into(),
            description: None,
            metadata: HashMap::from([
                ("projectId".into(), "project-1".into()),
                ("environmentId".into(), "environment-1".into()),
                ("environmentName".into(), "production".into()),
                ("serviceId".into(), "service-1".into()),
            ]),
        }
    }

    #[test]
    fn parses_upstream_project_shape_and_builds_explicit_target() {
        let rows: Vec<ProjectRow> = serde_json::from_str(
            r#"[{"workspace":{"id":"workspace-1","name":"Example Org"},"isFavorite":false,"id":"project-1","name":"Demo","environments":{"edges":[{"node":{"id":"environment-1","name":"production","canAccess":true}}]}}]"#,
        )
        .unwrap();
        let environment = &rows[0].environments.edges[0].node;
        let target = service_target(
            &rows[0],
            environment,
            ServiceRow {
                id: "service-1".into(),
                name: "api".into(),
                source: None,
            },
            "railway:local",
        )
        .unwrap();
        assert_eq!(target.metadata["projectId"], "project-1");
        assert_eq!(target.metadata["environmentId"], "environment-1");
        assert_eq!(target.metadata["serviceId"], "service-1");
        assert_eq!(
            deployment_args("project-1", "environment-1", "service-1"),
            [
                "deployment",
                "list",
                "--project",
                "project-1",
                "--environment",
                "environment-1",
                "--service",
                "service-1",
                "--limit",
                "21",
                "--json",
            ]
        );
    }

    #[test]
    fn maps_synthetic_deployment_without_inventing_attribution() {
        let rows: Vec<DeploymentRow> = serde_json::from_str(
            r#"[{"id":"deployment-1","status":"SUCCESS","createdAt":"2026-01-02T03:04:05Z","meta":{"branch":"main","commitMessage":"Ship demo"}}]"#,
        )
        .unwrap();
        let page = deployment_page(rows, &target()).unwrap();
        assert_eq!(page.activities[0].status, ActivityStatus::Success);
        assert_eq!(page.activities[0].ownership, Ownership::Unknown);
        assert_eq!(page.activities[0].branch.as_deref(), Some("main"));
        assert!(page.activities[0].url.starts_with("https://railway.com/"));
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn rejects_missing_or_invalid_target_metadata() {
        let mut target = target();
        target.metadata.insert("serviceId".into(), "--help".into());
        assert_eq!(
            support::metadata(&target, "serviceId"),
            Err(CliError::InvalidOutput)
        );
    }
}
