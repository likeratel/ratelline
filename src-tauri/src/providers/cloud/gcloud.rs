use std::collections::HashMap;

use serde::Deserialize;
use url::Url;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery,
        Ownership, Provider, Target,
    },
};

use super::support::{
    capped_targets, discovery_from_connection, failed_connection, issue, metadata, missing_cli,
    parse_timestamp, terminal_time, valid_argument, valid_slug, validate_target, MAX_TARGETS,
    PAGE_SIZE,
};

const PROVIDER: Provider = Provider::Gcloud;
const LABEL: &str = "Google Cloud";
const CLOUD_RUN: &str = "cloud-run";
const CLOUD_BUILD: &str = "cloud-build";
const MAX_PROJECTS: usize = 25;

#[derive(Deserialize)]
struct AuthAccount {
    account: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    project_id: String,
    name: String,
}

#[derive(Deserialize)]
struct RunService {
    metadata: RunMetadata,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunMetadata {
    name: String,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default)]
    annotations: HashMap<String, String>,
    #[serde(default)]
    creation_timestamp: Option<String>,
}

#[derive(Deserialize)]
struct RunRevision {
    metadata: RunMetadata,
    #[serde(default)]
    status: Option<RunRevisionStatus>,
}

#[derive(Deserialize)]
struct RunRevisionStatus {
    #[serde(default)]
    conditions: Vec<RunCondition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunCondition {
    #[serde(rename = "type")]
    condition_type: String,
    status: String,
    #[serde(default)]
    last_transition_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Build {
    id: String,
    project_id: String,
    status: String,
    create_time: String,
    #[serde(default)]
    start_time: Option<String>,
    #[serde(default)]
    finish_time: Option<String>,
    #[serde(default)]
    build_trigger_id: Option<String>,
    #[serde(default)]
    substitutions: HashMap<String, String>,
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
    let mut targets = Vec::new();
    let mut issues = Vec::new();
    if projects.len() == MAX_PROJECTS {
        issues.push(issue(
            PROVIDER,
            &account.id,
            None,
            "Google Cloud project discovery is limited to 25 active projects.",
        ));
    }
    for project in projects {
        if !valid_project(&project.project_id) || project.name.trim().is_empty() {
            issues.push(issue(
                PROVIDER,
                &account.id,
                None,
                "Google Cloud returned a project with invalid metadata; it was skipped.",
            ));
            continue;
        }
        targets.push(build_target(&project, &account.id));
        issues.push(issue(
            PROVIDER,
            &account.id,
            None,
            format!(
                "Cloud Build discovery for {} is limited to the explicit global region because gcloud requires a build region and exposes no command to discover all regions.",
                project.project_id
            ),
        ));
        let args = service_args(&project.project_id);
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        match cli.json::<Vec<RunService>>(&refs).await {
            Ok(services) => {
                if services.len() == MAX_TARGETS {
                    issues.push(issue(
                        PROVIDER,
                        &account.id,
                        None,
                        format!(
                            "Cloud Run service discovery for {} reached its 200-service limit.",
                            project.project_id
                        ),
                    ));
                }
                for service in services {
                    match run_target(&project, service, &account.id) {
                        Ok(target) => targets.push(target),
                        Err(_) => issues.push(issue(
                            PROVIDER,
                            &account.id,
                            None,
                            format!(
                                "Cloud Run returned a service in {} without a valid name or region; it was skipped.",
                                project.project_id
                            ),
                        )),
                    }
                }
            }
            Err(error) => issues.push(issue(
                PROVIDER,
                &account.id,
                None,
                format!(
                    "Cloud Run service discovery failed for {}: {}",
                    project.project_id,
                    error.user_message(LABEL)
                ),
            )),
        }
    }
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
        .json::<Vec<AuthAccount>>(&["auth", "list", "--filter=status:ACTIVE", "--format=json"])
        .await
    {
        Ok(accounts) => {
            let active = accounts
                .into_iter()
                .find(|candidate| candidate.status.eq_ignore_ascii_case("ACTIVE"));
            match active {
                Some(active) if !active.account.trim().is_empty() => Connection {
                    kind: None,
                    connection_id: account.id.clone(),
                    provider: PROVIDER,
                    status: ConnectionStatus::Connected,
                    account: Some(active.account),
                    message: Some(
                        "Cloud Run and Cloud Build are read only through enabled APIs and existing IAM access; ratelline does not enable APIs or change projects."
                            .into(),
                    ),
                    cli_version: version,
                },
                _ => failed_connection(
                    account,
                    PROVIDER,
                    LABEL,
                    version,
                    CliError::SignedOut,
                ),
            }
        }
        Err(error) => failed_connection(account, PROVIDER, LABEL, version, error),
    }
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    validate_target(target, PROVIDER)?;
    if cursor.is_some() {
        return Err(CliError::InvalidOutput);
    }
    let kind = metadata(target, "kind")?;
    let project = metadata(target, "project")?;
    if !valid_project(project) {
        return Err(CliError::InvalidOutput);
    }
    match kind {
        CLOUD_RUN => run_page(cli, target, project).await,
        CLOUD_BUILD => {
            let region = metadata(target, "region")?;
            if !valid_region(region) {
                return Err(CliError::InvalidOutput);
            }
            build_page(cli, target, project, region).await
        }
        _ => Err(CliError::InvalidOutput),
    }
}

async fn run_page(
    cli: &CliInvocation,
    target: &Target,
    project: &str,
) -> Result<ActivityPage, CliError> {
    let service = metadata(target, "service")?;
    let region = metadata(target, "region")?;
    if !valid_slug(service) || !valid_region(region) {
        return Err(CliError::InvalidOutput);
    }
    let args = revision_args(project, region, service);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let revisions: Vec<RunRevision> = cli.json(&refs).await?;
    let mut issues = Vec::new();
    let activities = revisions
        .iter()
        .map(|revision| map_revision(revision, target, project, region, service, &mut issues))
        .collect::<Result<Vec<_>, _>>()?;
    if revisions.len() == PAGE_SIZE {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            "gcloud returned the requested 20-revision limit without a reusable continuation token; older Cloud Run revisions may exist.",
        ));
    }
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

async fn build_page(
    cli: &CliInvocation,
    target: &Target,
    project: &str,
    region: &str,
) -> Result<ActivityPage, CliError> {
    let args = build_args(project, region);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let builds: Vec<Build> = cli.json(&refs).await?;
    let mut issues = Vec::new();
    let activities = builds
        .iter()
        .map(|build| map_build(build, target, project, &mut issues))
        .collect::<Result<Vec<_>, _>>()?;
    if builds.len() == PAGE_SIZE {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            "gcloud returned the requested 20-build limit without a reusable continuation token; older Cloud Build runs may exist.",
        ));
    }
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

fn project_args() -> [&'static str; 5] {
    [
        "projects",
        "list",
        "--filter=lifecycleState:ACTIVE",
        "--limit=25",
        "--format=json",
    ]
}

fn service_args(project: &str) -> Vec<String> {
    vec![
        "run".into(),
        "services".into(),
        "list".into(),
        format!("--project={project}"),
        "--platform=managed".into(),
        "--limit=200".into(),
        "--format=json".into(),
    ]
}

fn revision_args(project: &str, region: &str, service: &str) -> Vec<String> {
    vec![
        "run".into(),
        "revisions".into(),
        "list".into(),
        format!("--project={project}"),
        format!("--region={region}"),
        format!("--service={service}"),
        "--platform=managed".into(),
        "--sort-by=~metadata.creationTimestamp".into(),
        "--limit=20".into(),
        "--format=json".into(),
    ]
}

fn build_args(project: &str, region: &str) -> Vec<String> {
    vec![
        "builds".into(),
        "list".into(),
        format!("--project={project}"),
        format!("--region={region}"),
        "--sort-by=~createTime".into(),
        "--limit=20".into(),
        "--format=json".into(),
    ]
}

fn build_target(project: &Project, connection_id: &str) -> Target {
    Target {
        id: format!("gcloud:build:{}", project.project_id),
        provider: PROVIDER,
        connection_id: connection_id.into(),
        name: format!("{} builds", project.name),
        description: Some(format!("Cloud Build in {}", project.project_id)),
        metadata: HashMap::from([
            ("kind".into(), CLOUD_BUILD.into()),
            ("project".into(), project.project_id.clone()),
            ("region".into(), "global".into()),
        ]),
    }
}

fn run_target(
    project: &Project,
    service: RunService,
    connection_id: &str,
) -> Result<Target, CliError> {
    let region = service
        .metadata
        .labels
        .get("cloud.googleapis.com/location")
        .or_else(|| service.metadata.labels.get("run.googleapis.com/location"))
        .filter(|value| valid_region(value))
        .ok_or(CliError::InvalidOutput)?;
    if !valid_slug(&service.metadata.name) {
        return Err(CliError::InvalidOutput);
    }
    Ok(Target {
        id: format!(
            "gcloud:run:{}:{}:{}",
            project.project_id, region, service.metadata.name
        ),
        provider: PROVIDER,
        connection_id: connection_id.into(),
        name: service.metadata.name.clone(),
        description: Some(format!("Cloud Run in {}/{}", project.project_id, region)),
        metadata: HashMap::from([
            ("kind".into(), CLOUD_RUN.into()),
            ("project".into(), project.project_id.clone()),
            ("region".into(), region.clone()),
            ("service".into(), service.metadata.name),
        ]),
    })
}

fn map_revision(
    revision: &RunRevision,
    target: &Target,
    project: &str,
    region: &str,
    service: &str,
    issues: &mut Vec<crate::models::Issue>,
) -> Result<Activity, CliError> {
    if !valid_slug(&revision.metadata.name) {
        return Err(CliError::InvalidOutput);
    }
    let created = revision
        .metadata
        .creation_timestamp
        .as_deref()
        .ok_or(CliError::InvalidOutput)?;
    let started_at = parse_timestamp(created)?;
    let ready = revision.status.as_ref().and_then(|status| {
        status
            .conditions
            .iter()
            .find(|condition| condition.condition_type == "Ready")
    });
    let (status, raw_status) = ready
        .map(|condition| {
            (
                run_condition_status(&condition.status),
                format!("Ready={}", condition.status),
            )
        })
        .unwrap_or((ActivityStatus::Unknown, "Ready condition missing".into()));
    if status == ActivityStatus::Unknown {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            format!(
                "Cloud Run revision {} has no recognized Ready condition.",
                revision.metadata.name
            ),
        ));
    }
    let updated_at = ready
        .and_then(|condition| condition.last_transition_time.as_deref())
        .map(parse_timestamp)
        .transpose()?
        .unwrap_or_else(|| started_at.clone());
    let actor = revision
        .metadata
        .annotations
        .get("serving.knative.dev/creator")
        .cloned()
        .filter(|value| !value.trim().is_empty());
    Ok(Activity {
        id: format!("gcloud:run:{project}:{region}:{}", revision.metadata.name),
        provider: PROVIDER,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: "cloud-run-revision".into(),
        title: revision.metadata.name.clone(),
        project: format!("{project}/{service}"),
        branch: None,
        url: google_console_url(&["run", "detail", region, service, "revisions"], project)?,
        status,
        raw_status,
        actor,
        ownership: Ownership::Unknown,
        started_at,
        finished_at: terminal_time(status, &updated_at),
        updated_at,
    })
}

fn map_build(
    build: &Build,
    target: &Target,
    project: &str,
    issues: &mut Vec<crate::models::Issue>,
) -> Result<Activity, CliError> {
    if !valid_argument(&build.id) || build.project_id != project {
        return Err(CliError::InvalidOutput);
    }
    let created_at = parse_timestamp(&build.create_time)?;
    let started_at = build
        .start_time
        .as_deref()
        .map(parse_timestamp)
        .transpose()?
        .unwrap_or_else(|| created_at.clone());
    let updated_at = build
        .finish_time
        .as_deref()
        .map(parse_timestamp)
        .transpose()?
        .unwrap_or_else(|| started_at.clone());
    let status = build_status(&build.status);
    if status == ActivityStatus::Unknown {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            format!(
                "Cloud Build run {} has an unrecognized status '{}'.",
                build.id, build.status
            ),
        ));
    }
    let branch = build
        .substitutions
        .get("BRANCH_NAME")
        .or_else(|| build.substitutions.get("_BRANCH_NAME"))
        .cloned();
    let title = build
        .build_trigger_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|trigger| format!("Build {} from trigger {trigger}", build.id))
        .unwrap_or_else(|| format!("Build {}", build.id));
    Ok(Activity {
        id: format!("gcloud:build:{project}:{}", build.id),
        provider: PROVIDER,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: crate::providers::execution_name(
            build.substitutions.get("TRIGGER_NAME").map(String::as_str),
        ),
        kind: "cloud-build".into(),
        title,
        project: project.into(),
        branch,
        url: google_console_url(&["cloud-build", "builds", &build.id], project)?,
        status,
        raw_status: build.status.clone(),
        actor: None,
        ownership: Ownership::Unknown,
        started_at,
        finished_at: terminal_time(status, &updated_at),
        updated_at,
    })
}

fn google_console_url(segments: &[&str], project: &str) -> Result<String, CliError> {
    let mut url =
        Url::parse("https://console.cloud.google.com/").map_err(|_| CliError::InvalidOutput)?;
    url.path_segments_mut()
        .map_err(|_| CliError::InvalidOutput)?
        .extend(segments);
    url.query_pairs_mut().append_pair("project", project);
    Ok(url.into())
}

fn valid_project(value: &str) -> bool {
    (6..=30).contains(&value.len())
        && value.starts_with(|character: char| character.is_ascii_lowercase())
        && value.ends_with(|character: char| {
            character.is_ascii_lowercase() || character.is_ascii_digit()
        })
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_region(value: &str) -> bool {
    (3..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn run_condition_status(status: &str) -> ActivityStatus {
    match status {
        "True" => ActivityStatus::Success,
        "Unknown" => ActivityStatus::Running,
        "False" => ActivityStatus::Failed,
        _ => ActivityStatus::Unknown,
    }
}

fn build_status(status: &str) -> ActivityStatus {
    match status {
        "PENDING" | "QUEUED" => ActivityStatus::Queued,
        "WORKING" => ActivityStatus::Running,
        "SUCCESS" => ActivityStatus::Success,
        "FAILURE" | "INTERNAL_ERROR" | "TIMEOUT" | "EXPIRED" => ActivityStatus::Failed,
        "CANCELLED" => ActivityStatus::Canceled,
        "STATUS_UNKNOWN" => ActivityStatus::Unknown,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_target_fixture() -> Target {
        Target {
            id: "gcloud:build:example-project".into(),
            provider: PROVIDER,
            connection_id: "gcloud:local".into(),
            name: "example builds".into(),
            description: None,
            metadata: HashMap::from([
                ("kind".into(), CLOUD_BUILD.into()),
                ("project".into(), "example-project".into()),
                ("region".into(), "global".into()),
            ]),
        }
    }

    #[test]
    fn parses_cloud_build_fixture_and_validates_status() {
        let fixture = r#"{
          "id":"build-123", "projectId":"example-project", "status":"WORKING",
          "createTime":"2026-09-28T10:00:00Z", "startTime":"2026-09-28T10:00:05Z",
          "buildTriggerId":"trigger-1", "substitutions":{"BRANCH_NAME":"main", "TRIGGER_NAME":"deploy-production"}
        }"#;
        let build: Build = serde_json::from_str(fixture).expect("fixture");
        let mut issues = Vec::new();
        let activity = map_build(
            &build,
            &build_target_fixture(),
            "example-project",
            &mut issues,
        )
        .expect("activity");
        assert_eq!(activity.status, ActivityStatus::Running);
        assert_eq!(activity.workflow_name.as_deref(), Some("deploy-production"));
        assert_eq!(activity.ownership, Ownership::Unknown);
        assert!(activity
            .url
            .starts_with("https://console.cloud.google.com/"));
        assert!(issues.is_empty());
    }

    #[test]
    fn requests_are_explicitly_project_region_and_service_scoped() {
        let args = revision_args("example-project", "us-central1", "web");
        assert!(args.contains(&"--project=example-project".into()));
        assert!(args.contains(&"--region=us-central1".into()));
        assert!(args.contains(&"--service=web".into()));
        assert!(args.contains(&"--limit=20".into()));
    }

    #[test]
    fn running_service_state_is_not_a_revision_status() {
        assert_eq!(run_condition_status("Unknown"), ActivityStatus::Running);
        assert_eq!(run_condition_status("True"), ActivityStatus::Success);
        assert_eq!(build_status("STATUS_UNKNOWN"), ActivityStatus::Unknown);
    }
}
