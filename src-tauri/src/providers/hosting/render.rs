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

const LABEL: &str = "Render";
const CLI_PAGE_SIZE: usize = 20;

#[derive(Deserialize)]
struct RenderUser {
    id: String,
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct Workspace {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct ResourceRow {
    service: Option<Service>,
    project: Option<Named>,
    environment: Option<Named>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Service {
    id: String,
    name: String,
    owner_id: String,
    #[serde(rename = "type")]
    service_type: String,
    branch: Option<String>,
    dashboard_url: String,
}

#[derive(Deserialize)]
struct Named {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeployRow {
    id: String,
    commit: Option<Commit>,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    status: String,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct Commit {
    id: Option<String>,
    message: Option<String>,
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    let Some(cli) = cli else {
        return support::missing_cli(account, LABEL, "render");
    };
    let version = cli.version().await;
    match current_user(cli).await {
        Ok(user) => connected(account, version, user),
        Err(error) => support::failed_connection(account, LABEL, error, version),
    }
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let Some(cli) = cli else {
        return support::discovery_from_connection(support::missing_cli(account, LABEL, "render"));
    };
    let version = cli.version().await;
    let user = match current_user(cli).await {
        Ok(user) => user,
        Err(error) => return support::failed_discovery(account, LABEL, error, version),
    };
    let connection = connected(account, version, user);
    let (workspace, workspaces, resources) = match load_discovery(cli).await {
        Ok(result) => result,
        Err(error) => {
            return support::failed_discovery(account, LABEL, error, connection.cli_version);
        }
    };
    if support::validate_id(&workspace.id).is_err() {
        return support::failed_discovery(
            account,
            LABEL,
            CliError::InvalidOutput,
            connection.cli_version,
        );
    }
    if support::validate_text(&workspace.name, 256).is_err() {
        return support::failed_discovery(
            account,
            LABEL,
            CliError::InvalidOutput,
            connection.cli_version,
        );
    }

    let mut issues = Vec::new();
    if workspaces.len() > 1 {
        issues.push(support::issue(
            Provider::Render,
            &account.id,
            None,
            format!(
                "Render CLI can read only the active workspace without changing global CLI state. Discovered {} only; {} other workspace(s) were not scanned.",
                workspace.name,
                workspaces.len() - 1
            ),
        ));
    }
    let mut targets = Vec::new();
    for row in resources {
        let Some(service) = row.service else {
            continue;
        };
        match service_target(
            service,
            row.project,
            row.environment,
            &workspace,
            &account.id,
        ) {
            Ok(Some(target)) => targets.push(target),
            Ok(None) => {}
            Err(_) => issues.push(support::issue(
                Provider::Render,
                &account.id,
                None,
                "Render returned a service with invalid identifiers or dashboard URL.",
            )),
        }
    }
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

async fn current_user(cli: &CliInvocation) -> Result<RenderUser, CliError> {
    let user: RenderUser = cli
        .json(&["whoami", "--output", "json", "--confirm"])
        .await?;
    support::validate_id(&user.id)?;
    support::validate_text(&user.name, 512)?;
    support::validate_text(&user.email, 512)?;
    Ok(user)
}

fn connected(account: &Account, version: Option<String>, user: RenderUser) -> Connection {
    Connection {
        kind: None,
        connection_id: account.id.clone(),
        provider: Provider::Render,
        status: ConnectionStatus::Connected,
        account: Some(format!("{} ({})", user.name, user.email)),
        message: None,
        cli_version: version,
    }
}

async fn load_discovery(
    cli: &CliInvocation,
) -> Result<(Workspace, Vec<Workspace>, Vec<ResourceRow>), CliError> {
    let workspace = cli
        .json(&["workspace", "current", "--output", "json", "--confirm"])
        .await?;
    let workspaces = cli
        .json(&["workspaces", "--output", "json", "--confirm"])
        .await?;
    let resources = cli
        .json(&["services", "--output", "json", "--confirm"])
        .await?;
    Ok((workspace, workspaces, resources))
}

fn service_target(
    service: Service,
    project: Option<Named>,
    environment: Option<Named>,
    workspace: &Workspace,
    connection_id: &str,
) -> Result<Option<Target>, CliError> {
    if !is_deployable_type(&service.service_type) {
        return Ok(None);
    }
    support::validate_id(&service.id)?;
    support::validate_id(&service.owner_id)?;
    support::validate_text(&service.name, 256)?;
    if service.owner_id != workspace.id || !trusted_dashboard_url(&service.dashboard_url) {
        return Err(CliError::InvalidOutput);
    }
    if let Some(value) = project.as_ref() {
        support::validate_id(&value.id)?;
        support::validate_text(&value.name, 256)?;
    }
    if let Some(value) = environment.as_ref() {
        support::validate_id(&value.id)?;
        support::validate_text(&value.name, 256)?;
    }
    let description = [
        project.as_ref().map(|value| value.name.as_str()),
        environment.as_ref().map(|value| value.name.as_str()),
        Some(service.service_type.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let mut metadata = HashMap::from([
        ("serviceId".into(), service.id.clone()),
        ("serviceType".into(), service.service_type),
        ("workspaceId".into(), workspace.id.clone()),
        ("dashboardUrl".into(), service.dashboard_url),
    ]);
    if let Some(project) = project {
        metadata.insert("projectId".into(), project.id);
    }
    if let Some(environment) = environment {
        metadata.insert("environmentId".into(), environment.id);
    }
    if let Some(branch) = service
        .branch
        .filter(|branch| !branch.trim().is_empty() && branch.len() <= 512 && !branch.contains('\0'))
    {
        metadata.insert("branch".into(), branch);
    }
    Ok(Some(Target {
        id: service.id,
        provider: Provider::Render,
        connection_id: connection_id.to_owned(),
        name: service.name,
        description: Some(format!("{} · {description}", workspace.name)),
        metadata,
    }))
}

fn is_deployable_type(value: &str) -> bool {
    matches!(
        value,
        "web_service" | "background_worker" | "private_service" | "static_site" | "cron_job"
    )
}

fn trusted_dashboard_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("dashboard.render.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    support::reject_cursor(cursor)?;
    let service_id = support::metadata(target, "serviceId")?;
    let args = deploy_args(service_id);
    let rows: Vec<DeployRow> = cli.json(&args).await?;
    deploy_page(rows, target)
}

fn deploy_args(service_id: &str) -> [&str; 6] {
    [
        "deploys",
        "list",
        service_id,
        "--output",
        "json",
        "--confirm",
    ]
}

fn deploy_page(rows: Vec<DeployRow>, target: &Target) -> Result<ActivityPage, CliError> {
    let truncated = rows.len() == CLI_PAGE_SIZE;
    let activities = rows
        .into_iter()
        .map(|row| map_deploy(row, target))
        .collect::<Result<Vec<_>, _>>()?;
    let issues = truncated
        .then(|| {
            support::issue(
                Provider::Render,
                &target.connection_id,
                Some(&target.id),
                "Render CLI returned its 20-deploy page but does not expose the API cursor in non-interactive JSON output. Older history is unavailable.",
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

fn map_deploy(row: DeployRow, target: &Target) -> Result<Activity, CliError> {
    support::validate_id(&row.id)?;
    support::validate_text(&row.status, 128)?;
    let dashboard = target
        .metadata
        .get("dashboardUrl")
        .filter(|url| trusted_dashboard_url(url))
        .ok_or(CliError::InvalidOutput)?
        .trim_end_matches('/');
    let commit_id = row
        .commit
        .as_ref()
        .and_then(|commit| commit.id.as_deref())
        .filter(|value| !value.trim().is_empty());
    let title = row
        .commit
        .as_ref()
        .and_then(|commit| commit.message.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| commit_id.map(|value| format!("Deploy {}", &value[..value.len().min(12)])))
        .unwrap_or_else(|| format!("{} deploy", target.name));
    support::validate_text(&title, 512)?;
    let started = row.started_at.unwrap_or(row.created_at);
    Ok(Activity {
        id: format!("render:{}", row.id),
        provider: Provider::Render,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: target
            .metadata
            .get("serviceType")
            .cloned()
            .unwrap_or_else(|| "service".into()),
        title,
        project: target.name.clone(),
        branch: target.metadata.get("branch").cloned(),
        url: format!("{dashboard}/deploys/{}", support::path_segment(&row.id)),
        status: render_status(&row.status),
        raw_status: row.status,
        actor: None,
        ownership: Ownership::Unknown,
        started_at: started.to_rfc3339(),
        finished_at: row.finished_at.map(|value| value.to_rfc3339()),
        updated_at: row.updated_at.to_rfc3339(),
    })
}

fn render_status(status: &str) -> ActivityStatus {
    match status {
        "created" | "queued" => ActivityStatus::Queued,
        "build_in_progress" | "pre_deploy_in_progress" | "update_in_progress" => {
            ActivityStatus::Running
        }
        "live" | "deactivated" => ActivityStatus::Success,
        "build_failed" | "pre_deploy_failed" | "update_failed" => ActivityStatus::Failed,
        "canceled" => ActivityStatus::Canceled,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "srv-demo".into(),
            provider: Provider::Render,
            connection_id: "render:local".into(),
            name: "api".into(),
            description: None,
            metadata: HashMap::from([
                ("serviceId".into(), "srv-demo".into()),
                ("serviceType".into(), "web_service".into()),
                (
                    "dashboardUrl".into(),
                    "https://dashboard.render.com/web/srv-demo".into(),
                ),
                ("branch".into(), "main".into()),
            ]),
        }
    }

    #[test]
    fn parses_nested_service_response_and_filters_non_deployable_resources() {
        let rows: Vec<ResourceRow> = serde_json::from_str(
            r#"[{"service":{"id":"srv-demo","name":"api","ownerId":"workspace-1","type":"web_service","branch":"main","dashboardUrl":"https://dashboard.render.com/web/srv-demo"},"project":{"id":"project-1","name":"Demo"},"environment":{"id":"environment-1","name":"Production"}},{"postgres":{"id":"dpg-demo"}}]"#,
        )
        .unwrap();
        let workspace = Workspace {
            id: "workspace-1".into(),
            name: "Example Org".into(),
        };
        let row = rows.into_iter().next().unwrap();
        let target = service_target(
            row.service.unwrap(),
            row.project,
            row.environment,
            &workspace,
            "render:local",
        )
        .unwrap()
        .unwrap();
        assert_eq!(target.metadata["serviceId"], "srv-demo");
        assert_eq!(
            deploy_args("srv-demo"),
            [
                "deploys",
                "list",
                "srv-demo",
                "--output",
                "json",
                "--confirm"
            ]
        );
    }

    #[test]
    fn maps_synthetic_deploy_and_keeps_attribution_unknown() {
        let rows: Vec<DeployRow> = serde_json::from_str(
            r#"[{"id":"dep-demo","commit":{"id":"abcdef1234567890","message":"Ship demo"},"createdAt":"2026-01-02T03:04:05Z","startedAt":"2026-01-02T03:04:10Z","finishedAt":"2026-01-02T03:06:05Z","status":"live","updatedAt":"2026-01-02T03:06:05Z"}]"#,
        )
        .unwrap();
        let page = deploy_page(rows, &target()).unwrap();
        assert_eq!(page.activities[0].status, ActivityStatus::Success);
        assert_eq!(page.activities[0].ownership, Ownership::Unknown);
        assert!(page.activities[0]
            .url
            .starts_with("https://dashboard.render.com/"));
    }

    #[test]
    fn rejects_untrusted_dashboard_links() {
        assert!(!trusted_dashboard_url(
            "https://dashboard.render.com.example.test/web/srv-demo"
        ));
    }
}
