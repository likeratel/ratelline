use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Connection, Discovery, Ownership,
        Provider, Target,
    },
};

use super::{cloudflare, support};

const PAGE_SIZE: usize = 20;
const ACTIVE_PAGE_SIZE: usize = 100;
const MAX_ACTIVE_PAGES: u32 = 2;
const ACTIVE_SCAN_BUDGET: Duration = Duration::from_secs(35);
const DISCOVERY_PAGE_SIZE: usize = 100;
const MAX_DISCOVERY_PAGES: u32 = 50;
const MAX_HISTORY_PAGE: u32 = 10_000;

#[derive(Deserialize)]
struct Project {
    name: String,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    subdomain: Option<String>,
}

#[derive(Deserialize)]
struct Deployment {
    id: String,
    created_on: DateTime<Utc>,
    modified_on: DateTime<Utc>,
    environment: String,
    #[serde(default)]
    is_skipped: bool,
    latest_stage: Stage,
    #[serde(default)]
    deployment_trigger: Option<DeploymentTrigger>,
}

#[derive(Deserialize)]
struct Stage {
    name: String,
    status: String,
    #[serde(default)]
    started_on: Option<DateTime<Utc>>,
    #[serde(default)]
    ended_on: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct DeploymentTrigger {
    #[serde(default)]
    metadata: Option<TriggerMetadata>,
}

#[derive(Deserialize)]
struct TriggerMetadata {
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    commit_hash: Option<String>,
    #[serde(default)]
    commit_message: Option<String>,
}

pub async fn connection(_cli: Option<&CliInvocation>, account: &Account) -> Connection {
    cloudflare::connection(account).await
}

pub async fn discover(_cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let connection = cloudflare::connection(account).await;
    if connection.status != crate::models::ConnectionStatus::Connected {
        return support::discovery_from_connection(connection);
    }
    let api = match cloudflare::api_for_account(account).await {
        Ok(api) => api,
        Err(message) => {
            return Discovery {
                connection: Connection::disconnected(
                    &account.id,
                    account.provider,
                    crate::models::ConnectionStatus::Error,
                    message,
                ),
                targets: Vec::new(),
                aws_profiles: Vec::new(),
                issues: Vec::new(),
            }
        }
    };
    let mut targets = Vec::new();
    let mut issues = Vec::new();
    let mut reached_limit = false;
    for page in 1..=MAX_DISCOVERY_PAGES {
        let page_text = page.to_string();
        let size_text = DISCOVERY_PAGE_SIZE.to_string();
        let path = ["accounts", api.account_id(), "pages", "projects"];
        let (projects, info): (Vec<Project>, _) = match api
            .get(
                &path,
                &[
                    ("page", page_text.as_str()),
                    ("per_page", size_text.as_str()),
                ],
            )
            .await
        {
            Ok(result) => result,
            Err(message) => {
                issues.push(support::issue(
                    Provider::CloudflarePages,
                    &account.id,
                    None,
                    message,
                ));
                break;
            }
        };
        let count = projects.len();
        for project in projects {
            match project_target(project, api.account_id(), &account.id) {
                Ok(target) => targets.push(target),
                Err(_) => issues.push(support::issue(
                    Provider::CloudflarePages,
                    &account.id,
                    None,
                    "Cloudflare returned a Pages project with invalid metadata; it was skipped.",
                )),
            }
        }
        let has_more = info
            .is_some_and(|info| info.total_pages > info.page && info.total_pages > page)
            || count == DISCOVERY_PAGE_SIZE;
        if !has_more {
            break;
        }
        reached_limit = page == MAX_DISCOVERY_PAGES;
    }
    if reached_limit {
        issues.push(support::issue(
            Provider::CloudflarePages,
            &account.id,
            None,
            "Cloudflare Pages discovery reached ratelline's 5,000-project safety limit. Results are partial.",
        ));
    }
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

fn project_target(
    project: Project,
    account_id: &str,
    connection_id: &str,
) -> Result<Target, CliError> {
    support::validate_id(&project.name)?;
    for domain in &project.domains {
        support::validate_text(domain, 512)?;
    }
    if let Some(subdomain) = project.subdomain.as_deref() {
        support::validate_text(subdomain, 512)?;
    }
    let description = project
        .domains
        .first()
        .cloned()
        .or(project.subdomain)
        .map(|domain| format!("{account_id} · {domain}"))
        .or_else(|| Some(account_id.to_owned()));
    Ok(Target {
        id: format!("{account_id}:{}", project.name),
        provider: Provider::CloudflarePages,
        connection_id: connection_id.to_owned(),
        name: project.name.clone(),
        description,
        metadata: HashMap::from([
            ("accountId".into(), account_id.to_owned()),
            ("projectName".into(), project.name),
        ]),
    })
}

pub async fn page_api(target: &Target, cursor: Option<&str>) -> Result<ActivityPage, String> {
    validate_target(target)?;
    let page = parse_cursor(cursor)?;
    let api = cloudflare::api_for_target(target).await?;
    let project = target.metadata["projectName"].as_str();
    let page_text = page.to_string();
    let size_text = PAGE_SIZE.to_string();
    let path = [
        "accounts",
        api.account_id(),
        "pages",
        "projects",
        project,
        "deployments",
    ];
    let (deployments, info): (Vec<Deployment>, _) = api
        .get(
            &path,
            &[
                ("page", page_text.as_str()),
                ("per_page", size_text.as_str()),
            ],
        )
        .await?;
    let count = deployments.len();
    let (activities, mut issues) = map_deployments(deployments, target, api.account_id(), project);
    let has_more = has_more(info, count, page, PAGE_SIZE);
    let reached_limit = has_more && page == MAX_HISTORY_PAGE;
    Ok(ActivityPage {
        activities,
        next_cursor: (has_more && !reached_limit).then(|| (page + 1).to_string()),
        issues: {
            if reached_limit {
                issues.push(support::issue(
                    Provider::CloudflarePages,
                    &target.connection_id,
                    Some(&target.id),
                    "Cloudflare Pages history reached ratelline's 10,000-page cursor safety limit.",
                ));
            }
            issues
        },
    })
}

pub async fn active_api(target: &Target) -> Result<ActivityPage, String> {
    let deadline = active_deadline(tokio::time::Instant::now());
    validate_target(target)?;
    let api = tokio::time::timeout_at(deadline, cloudflare::api_for_target(target))
        .await
        .map_err(|_| {
            "Cloudflare Pages active scan timed out before authentication.".to_owned()
        })??;
    let project = target.metadata["projectName"].as_str();
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    let mut reached_limit = false;

    for page in 1..=MAX_ACTIVE_PAGES {
        let page_text = page.to_string();
        let size_text = ACTIVE_PAGE_SIZE.to_string();
        let path = [
            "accounts",
            api.account_id(),
            "pages",
            "projects",
            project,
            "deployments",
        ];
        let request = api.get::<Vec<Deployment>>(
            &path,
            &[
                ("page", page_text.as_str()),
                ("per_page", size_text.as_str()),
            ],
        );
        let result = match tokio::time::timeout_at(deadline, request).await {
            Ok(result) => result,
            Err(_) => {
                Err("Cloudflare Pages active scan reached the monitoring deadline.".to_owned())
            }
        };
        let (deployments, info) = match result {
            Ok(result) => result,
            Err(message) if page > 1 => {
                issues.push(support::issue(
                    Provider::CloudflarePages,
                    &target.connection_id,
                    Some(&target.id),
                    format!(
                        "Cloudflare Pages active scan stopped after a partial result: {message}"
                    ),
                ));
                break;
            }
            Err(message) => return Err(message),
        };
        let count = deployments.len();
        let (mapped, row_issues) = map_deployments(deployments, target, api.account_id(), project);
        issues.extend(row_issues);
        activities.extend(mapped.into_iter().filter(is_active));
        let has_more = has_more(info, count, page, ACTIVE_PAGE_SIZE);
        if !has_more {
            break;
        }
        reached_limit = page == MAX_ACTIVE_PAGES;
    }

    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues: {
            if reached_limit {
                issues.push(support::issue(
                    Provider::CloudflarePages,
                    &target.connection_id,
                    Some(&target.id),
                    "Cloudflare Pages active scan reached ratelline's 200-deployment safety limit. Active results are partial.",
                ));
            }
            issues
        },
    })
}

fn active_deadline(now: tokio::time::Instant) -> tokio::time::Instant {
    now + ACTIVE_SCAN_BUDGET
}

fn map_deployments(
    deployments: Vec<Deployment>,
    target: &Target,
    account_id: &str,
    project: &str,
) -> (Vec<Activity>, Vec<crate::models::Issue>) {
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    for deployment in deployments {
        match map_deployment(deployment, target, account_id, project) {
            Ok(activity) => activities.push(activity),
            Err(message) => issues.push(support::issue(
                Provider::CloudflarePages,
                &target.connection_id,
                Some(&target.id),
                format!("{message} The invalid deployment was skipped."),
            )),
        }
    }
    (activities, issues)
}

pub async fn page(
    _cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    page_api(target, cursor)
        .await
        .map_err(|_| CliError::InvalidOutput)
}

fn validate_target(target: &Target) -> Result<(), String> {
    target.validate()?;
    if target.provider != Provider::CloudflarePages {
        return Err("The selected target is not a Cloudflare Pages project.".into());
    }
    for key in ["accountId", "projectName"] {
        support::metadata(target, key).map_err(|_| "The Cloudflare Pages target is invalid.")?;
    }
    Ok(())
}

fn parse_cursor(cursor: Option<&str>) -> Result<u32, String> {
    match cursor {
        None => Ok(1),
        Some(value) => value
            .parse::<u32>()
            .ok()
            .filter(|page| (2..=MAX_HISTORY_PAGE).contains(page))
            .ok_or_else(|| "The Cloudflare Pages history cursor is invalid.".to_owned()),
    }
}

fn map_deployment(
    deployment: Deployment,
    target: &Target,
    account_id: &str,
    project: &str,
) -> Result<Activity, String> {
    support::validate_id(&deployment.id)
        .map_err(|_| "Cloudflare returned an invalid deployment ID.")?;
    support::validate_text(&deployment.environment, 128)
        .map_err(|_| "Cloudflare returned an invalid deployment environment.")?;
    support::validate_text(&deployment.latest_stage.name, 128)
        .map_err(|_| "Cloudflare returned an invalid deployment stage.")?;
    support::validate_text(&deployment.latest_stage.status, 128)
        .map_err(|_| "Cloudflare returned an invalid deployment status.")?;
    let trigger = deployment
        .deployment_trigger
        .and_then(|trigger| trigger.metadata);
    for value in trigger
        .as_ref()
        .into_iter()
        .flat_map(|metadata| {
            [
                metadata.branch.as_deref(),
                metadata.commit_hash.as_deref(),
                metadata.commit_message.as_deref(),
            ]
        })
        .flatten()
    {
        support::validate_text(value, 1024)
            .map_err(|_| "Cloudflare returned invalid deployment metadata.")?;
    }
    let status = pages_status(
        &deployment.latest_stage.name,
        &deployment.latest_stage.status,
        deployment.is_skipped,
    );
    let terminal = matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    );
    let finished_at = terminal.then(|| {
        deployment
            .latest_stage
            .ended_on
            .unwrap_or(deployment.modified_on)
            .to_rfc3339()
    });
    let started_at = deployment
        .latest_stage
        .started_on
        .unwrap_or(deployment.created_on)
        .to_rfc3339();
    let branch = trigger
        .as_ref()
        .and_then(|metadata| metadata.branch.clone());
    let title = trigger
        .as_ref()
        .and_then(|metadata| metadata.commit_message.clone())
        .filter(|message| !message.trim().is_empty())
        .or_else(|| {
            trigger
                .as_ref()
                .and_then(|metadata| metadata.commit_hash.as_deref())
                .map(|hash| format!("Deploy {}", short_hash(hash)))
        })
        .unwrap_or_else(|| format!("{} deployment", target.name));
    Ok(Activity {
        id: format!("cloudflare-pages:{}", deployment.id),
        provider: Provider::CloudflarePages,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        kind: deployment.environment,
        title,
        workflow_name: None,
        project: target.name.clone(),
        branch,
        url: dashboard_url(account_id, project, &deployment.id),
        status,
        raw_status: deployment.latest_stage.status,
        actor: None,
        ownership: Ownership::Unknown,
        started_at,
        finished_at,
        updated_at: deployment.modified_on.to_rfc3339(),
    })
}

fn pages_status(stage: &str, raw: &str, skipped: bool) -> ActivityStatus {
    if skipped {
        return ActivityStatus::Skipped;
    }
    match raw {
        "idle" | "queued" => ActivityStatus::Queued,
        "active" | "running" => ActivityStatus::Running,
        "success" if stage == "deploy" => ActivityStatus::Success,
        "success" => ActivityStatus::Running,
        "failure" | "failed" => ActivityStatus::Failed,
        "canceled" | "cancelled" => ActivityStatus::Canceled,
        _ => ActivityStatus::Unknown,
    }
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

fn is_active(activity: &Activity) -> bool {
    matches!(
        activity.status,
        ActivityStatus::Queued | ActivityStatus::Running
    )
}

fn has_more(
    info: Option<cloudflare::ResultInfo>,
    count: usize,
    page: u32,
    page_size: usize,
) -> bool {
    info.is_some_and(|info| info.total_pages > info.page && info.total_pages > page)
        || count == page_size
}

fn dashboard_url(account_id: &str, project: &str, deployment_id: &str) -> String {
    format!(
        "https://dash.cloudflare.com/{}/pages/view/{}/{}",
        support::path_segment(account_id),
        support::path_segment(project),
        support::path_segment(deployment_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "0123456789abcdef0123456789abcdef:demo-pages".into(),
            provider: Provider::CloudflarePages,
            connection_id: "cloudflare-pages:00000000-0000-0000-0000-000000000001".into(),
            name: "demo-pages".into(),
            description: None,
            metadata: HashMap::from([
                (
                    "accountId".into(),
                    "0123456789abcdef0123456789abcdef".into(),
                ),
                ("projectName".into(), "demo-pages".into()),
            ]),
        }
    }

    #[test]
    fn maps_synthetic_pages_deployment() {
        let deployment: Deployment = serde_json::from_str(
            r#"{
              "id":"f64788e9-fccd-4d4a-a28a-cb84f88f6",
              "created_on":"2026-10-06T01:00:00Z",
              "modified_on":"2026-10-06T01:03:00Z",
              "environment":"production",
              "is_skipped":false,
              "latest_stage":{"name":"deploy","status":"success","started_on":"2026-10-06T01:01:00Z","ended_on":"2026-10-06T01:03:00Z"},
              "deployment_trigger":{"metadata":{"branch":"main","commit_hash":"abcdef1234567890","commit_message":"Ship sample"}}
            }"#,
        )
        .unwrap();
        let activity = map_deployment(
            deployment,
            &target(),
            "0123456789abcdef0123456789abcdef",
            "demo-pages",
        )
        .unwrap();
        assert_eq!(activity.status, ActivityStatus::Success);
        assert_eq!(activity.title, "Ship sample");
        assert_eq!(activity.branch.as_deref(), Some("main"));
        assert_eq!(activity.ownership, Ownership::Unknown);
    }

    #[test]
    fn cursor_and_status_mapping_are_explicit() {
        assert_eq!(parse_cursor(None), Ok(1));
        assert_eq!(parse_cursor(Some("2")), Ok(2));
        assert!(parse_cursor(Some("1")).is_err());
        assert_eq!(
            pages_status("build", "active", false),
            ActivityStatus::Running
        );
        assert_eq!(
            pages_status("build", "success", false),
            ActivityStatus::Running
        );
        assert_eq!(
            pages_status("deploy", "success", false),
            ActivityStatus::Success
        );
        assert_eq!(
            pages_status("deploy", "mystery", false),
            ActivityStatus::Unknown
        );
        assert_eq!(short_hash("배포해시abcdef123456"), "배포해시abcdef12");
        assert_eq!(
            pages_status("deploy", "success", true),
            ActivityStatus::Skipped
        );
    }

    #[test]
    fn active_scan_deadline_uses_one_absolute_budget() {
        let now = tokio::time::Instant::now();
        assert_eq!(active_deadline(now), now + ACTIVE_SCAN_BUDGET);
    }
}
