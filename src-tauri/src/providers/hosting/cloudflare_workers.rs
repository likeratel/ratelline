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

const DEPLOYMENT: &str = "deployment";
const BUILD: &str = "build";
const PAGE_SIZE: usize = 50;
const ACTIVE_PAGE_SIZE: usize = 200;
const MAX_ACTIVE_PAGES: u32 = 2;
const ACTIVE_SCAN_BUDGET: Duration = Duration::from_secs(35);
const MAX_HISTORY_PAGE: u32 = 10_000;

#[derive(Deserialize)]
struct Script {
    id: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    modified_on: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct DeploymentList {
    #[serde(default)]
    deployments: Vec<WorkerDeployment>,
}

#[derive(Deserialize)]
struct WorkerDeployment {
    id: String,
    created_on: DateTime<Utc>,
    source: String,
    #[serde(default)]
    author_email: Option<String>,
    #[serde(default)]
    annotations: HashMap<String, String>,
}

#[derive(Deserialize)]
struct WorkerBuild {
    #[serde(default)]
    build_uuid: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    build_outcome: Option<String>,
    #[serde(default)]
    created_on: Option<DateTime<Utc>>,
    #[serde(default)]
    initializing_on: Option<DateTime<Utc>>,
    #[serde(default)]
    modified_on: Option<DateTime<Utc>>,
    #[serde(default)]
    running_on: Option<DateTime<Utc>>,
    #[serde(default)]
    stopped_on: Option<DateTime<Utc>>,
    #[serde(default)]
    preview_url: Option<String>,
    #[serde(default)]
    build_trigger_metadata: Option<BuildTriggerMetadata>,
}

#[derive(Deserialize)]
struct BuildTriggerMetadata {
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    commit_hash: Option<String>,
    #[serde(default)]
    commit_message: Option<String>,
    #[serde(default)]
    trigger_name: Option<String>,
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
    let path = ["accounts", api.account_id(), "workers", "scripts"];
    let scripts: Vec<Script> = match api.get(&path, &[]).await {
        Ok((scripts, _)) => scripts,
        Err(message) => {
            return Discovery {
                connection,
                targets: Vec::new(),
                aws_profiles: Vec::new(),
                issues: vec![support::issue(
                    Provider::CloudflareWorkers,
                    &account.id,
                    None,
                    message,
                )],
            }
        }
    };
    let mut targets = Vec::new();
    let mut issues = Vec::new();
    for script in scripts {
        if let Err(message) = validate_script(&script) {
            issues.push(support::issue(
                Provider::CloudflareWorkers,
                &account.id,
                None,
                message,
            ));
            continue;
        }
        targets.push(script_target(
            &script,
            api.account_id(),
            &account.id,
            DEPLOYMENT,
        ));
        if script.tag.is_some() {
            targets.push(script_target(&script, api.account_id(), &account.id, BUILD));
        } else {
            issues.push(support::issue(
                Provider::CloudflareWorkers,
                &account.id,
                None,
                format!(
                    "Worker {} has no immutable script tag, so Workers Builds cannot be selected for it.",
                    script.id
                ),
            ));
        }
    }
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

fn validate_script(script: &Script) -> Result<(), &'static str> {
    support::validate_id(&script.id)
        .map_err(|_| "Cloudflare returned a Worker with an invalid script name; it was skipped.")?;
    if let Some(tag) = script.tag.as_deref() {
        support::validate_id(tag).map_err(|_| {
            "Cloudflare returned a Worker with an invalid script tag; it was skipped."
        })?;
    }
    Ok(())
}

fn script_target(script: &Script, account_id: &str, connection_id: &str, kind: &str) -> Target {
    let label = if kind == BUILD {
        "Builds"
    } else {
        "Deployments"
    };
    let mut metadata = HashMap::from([
        ("accountId".into(), account_id.to_owned()),
        ("scriptName".into(), script.id.clone()),
        ("kind".into(), kind.to_owned()),
    ]);
    if let Some(tag) = script.tag.clone() {
        metadata.insert("scriptTag".into(), tag);
    }
    Target {
        id: format!("{account_id}:{}:{kind}", script.id),
        provider: Provider::CloudflareWorkers,
        connection_id: connection_id.to_owned(),
        name: format!("{} · {label}", script.id),
        description: script
            .modified_on
            .map(|modified| format!("Last modified {}", modified.to_rfc3339())),
        metadata,
    }
}

pub async fn page_api(target: &Target, cursor: Option<&str>) -> Result<ActivityPage, String> {
    validate_target(target)?;
    let page = parse_cursor(cursor)?;
    let api = cloudflare::api_for_target(target).await?;
    match target.metadata["kind"].as_str() {
        DEPLOYMENT => deployment_page(&api, target, page).await,
        BUILD => build_page(&api, target, page).await,
        _ => Err("The Cloudflare Workers target kind is invalid.".into()),
    }
}

pub async fn active_api(target: &Target) -> Result<ActivityPage, String> {
    let deadline = active_deadline(tokio::time::Instant::now());
    validate_target(target)?;
    if target.metadata["kind"] == DEPLOYMENT {
        return Ok(ActivityPage::default());
    }
    if target.metadata["kind"] != BUILD {
        return Err("The Cloudflare Workers target kind is invalid.".into());
    }

    let api = tokio::time::timeout_at(deadline, cloudflare::api_for_target(target))
        .await
        .map_err(|_| "Workers Builds active scan timed out before authentication.".to_owned())??;
    let script = target.metadata["scriptName"].as_str();
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    let mut reached_limit = false;
    for page in 1..=MAX_ACTIVE_PAGES {
        let request = fetch_builds(&api, target, page, ACTIVE_PAGE_SIZE);
        let result = match tokio::time::timeout_at(deadline, request).await {
            Ok(result) => result,
            Err(_) => Err("Workers Builds active scan reached the monitoring deadline.".to_owned()),
        };
        let (builds, info) = match result {
            Ok(result) => result,
            Err(message) if page > 1 => {
                issues.push(support::issue(
                    Provider::CloudflareWorkers,
                    &target.connection_id,
                    Some(&target.id),
                    format!("Workers Builds active scan stopped after a partial result: {message}"),
                ));
                break;
            }
            Err(message) => return Err(message),
        };
        let count = builds.len();
        let (mapped, row_issues) = map_builds(builds, target, api.account_id(), script);
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
                    Provider::CloudflareWorkers,
                    &target.connection_id,
                    Some(&target.id),
                    "Workers Builds active scan reached ratelline's 400-build safety limit. Active results are partial.",
                ));
            }
            issues
        },
    })
}

fn active_deadline(now: tokio::time::Instant) -> tokio::time::Instant {
    now + ACTIVE_SCAN_BUDGET
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

async fn deployment_page(
    api: &cloudflare::Api,
    target: &Target,
    page: u32,
) -> Result<ActivityPage, String> {
    let script = target.metadata["scriptName"].as_str();
    let page_text = page.to_string();
    let size_text = PAGE_SIZE.to_string();
    let path = [
        "accounts",
        api.account_id(),
        "workers",
        "scripts",
        script,
        "deployments",
    ];
    let (list, info): (DeploymentList, _) = api
        .get(
            &path,
            &[
                ("page", page_text.as_str()),
                ("per_page", size_text.as_str()),
            ],
        )
        .await?;
    let count = list.deployments.len();
    let (activities, mut issues) =
        map_deployments(list.deployments, target, api.account_id(), script);
    let has_more = has_more(info, count, page, PAGE_SIZE);
    let reached_limit = has_more && page == MAX_HISTORY_PAGE;
    Ok(ActivityPage {
        activities,
        next_cursor: (has_more && !reached_limit).then(|| (page + 1).to_string()),
        issues: {
            if reached_limit {
                issues.push(support::issue(
                    Provider::CloudflareWorkers,
                    &target.connection_id,
                    Some(&target.id),
                    "Worker deployment history reached ratelline's 10,000-page cursor safety limit.",
                ));
            }
            issues
        },
    })
}

async fn build_page(
    api: &cloudflare::Api,
    target: &Target,
    page: u32,
) -> Result<ActivityPage, String> {
    let (builds, info) = fetch_builds(api, target, page, PAGE_SIZE).await?;
    let count = builds.len();
    let script = target.metadata["scriptName"].as_str();
    let (activities, mut issues) = map_builds(builds, target, api.account_id(), script);
    let has_more = has_more(info, count, page, PAGE_SIZE);
    let reached_limit = has_more && page == MAX_HISTORY_PAGE;
    Ok(ActivityPage {
        activities,
        next_cursor: (has_more && !reached_limit).then(|| (page + 1).to_string()),
        issues: {
            if reached_limit {
                issues.push(support::issue(
                    Provider::CloudflareWorkers,
                    &target.connection_id,
                    Some(&target.id),
                    "Workers Builds history reached ratelline's 10,000-page cursor safety limit.",
                ));
            }
            issues
        },
    })
}

fn map_deployments(
    deployments: Vec<WorkerDeployment>,
    target: &Target,
    account_id: &str,
    script: &str,
) -> (Vec<Activity>, Vec<crate::models::Issue>) {
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    for deployment in deployments {
        match map_deployment(deployment, target, account_id, script) {
            Ok(activity) => activities.push(activity),
            Err(message) => issues.push(support::issue(
                Provider::CloudflareWorkers,
                &target.connection_id,
                Some(&target.id),
                format!("{message} The invalid deployment was skipped."),
            )),
        }
    }
    (activities, issues)
}

fn map_builds(
    builds: Vec<WorkerBuild>,
    target: &Target,
    account_id: &str,
    script: &str,
) -> (Vec<Activity>, Vec<crate::models::Issue>) {
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    for build in builds {
        match map_build(build, target, account_id, script) {
            Ok(activity) => activities.push(activity),
            Err(message) => issues.push(support::issue(
                Provider::CloudflareWorkers,
                &target.connection_id,
                Some(&target.id),
                format!("{message} The invalid build was skipped."),
            )),
        }
    }
    (activities, issues)
}

async fn fetch_builds(
    api: &cloudflare::Api,
    target: &Target,
    page: u32,
    page_size: usize,
) -> Result<(Vec<WorkerBuild>, Option<cloudflare::ResultInfo>), String> {
    let tag = target
        .metadata
        .get("scriptTag")
        .ok_or_else(|| "The Workers Builds target has no immutable script tag.".to_owned())?;
    let page_text = page.to_string();
    let size_text = page_size.to_string();
    let path = [
        "accounts",
        api.account_id(),
        "builds",
        "workers",
        tag,
        "builds",
    ];
    api.get(
        &path,
        &[
            ("page", page_text.as_str()),
            ("per_page", size_text.as_str()),
        ],
    )
    .await
    .map_err(|message| format!("Workers Builds history is unavailable for this target: {message}"))
}

fn validate_target(target: &Target) -> Result<(), String> {
    target.validate()?;
    if target.provider != Provider::CloudflareWorkers {
        return Err("The selected target is not a Cloudflare Worker.".into());
    }
    for key in ["accountId", "scriptName", "kind"] {
        support::metadata(target, key).map_err(|_| "The Cloudflare Workers target is invalid.")?;
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
            .ok_or_else(|| "The Cloudflare Workers history cursor is invalid.".to_owned()),
    }
}

fn map_deployment(
    deployment: WorkerDeployment,
    target: &Target,
    account_id: &str,
    script: &str,
) -> Result<Activity, String> {
    support::validate_id(&deployment.id)
        .map_err(|_| "Cloudflare returned an invalid Worker deployment ID.")?;
    support::validate_text(&deployment.source, 128)
        .map_err(|_| "Cloudflare returned an invalid Worker deployment source.")?;
    let annotation_title = deployment.annotations.get("workers/message").cloned();
    let actor = deployment
        .author_email
        .clone()
        .or_else(|| deployment.annotations.get("workers/triggered_by").cloned());
    for value in annotation_title.iter().chain(actor.iter()) {
        support::validate_text(value, 1024)
            .map_err(|_| "Cloudflare returned invalid Worker deployment metadata.")?;
    }
    let timestamp = deployment.created_on.to_rfc3339();
    Ok(Activity {
        id: format!("cloudflare-worker-deployment:{}", deployment.id),
        provider: Provider::CloudflareWorkers,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        kind: DEPLOYMENT.into(),
        title: annotation_title.unwrap_or_else(|| format!("Deploy {script}")),
        workflow_name: None,
        project: script.into(),
        branch: None,
        url: worker_dashboard_url(account_id, script),
        status: ActivityStatus::Success,
        raw_status: deployment.source,
        actor,
        ownership: Ownership::Unknown,
        started_at: timestamp.clone(),
        finished_at: Some(timestamp.clone()),
        updated_at: timestamp,
    })
}

fn map_build(
    build: WorkerBuild,
    target: &Target,
    account_id: &str,
    script: &str,
) -> Result<Activity, String> {
    let id = build
        .build_uuid
        .ok_or_else(|| "Cloudflare returned a Workers build without an ID.".to_owned())?;
    support::validate_id(&id).map_err(|_| "Cloudflare returned an invalid Workers build ID.")?;
    let raw_status = build.status.clone().unwrap_or_else(|| "unknown".into());
    support::validate_text(&raw_status, 128)
        .map_err(|_| "Cloudflare returned an invalid Workers build status.")?;
    if let Some(outcome) = build.build_outcome.as_deref() {
        support::validate_text(outcome, 128)
            .map_err(|_| "Cloudflare returned an invalid Workers build outcome.")?;
    }
    let created = build
        .created_on
        .or(build.initializing_on)
        .or(build.running_on)
        .ok_or_else(|| "Cloudflare returned a Workers build without a start time.".to_owned())?;
    let metadata = build.build_trigger_metadata;
    for value in metadata
        .as_ref()
        .into_iter()
        .flat_map(|metadata| {
            [
                metadata.author.as_deref(),
                metadata.branch.as_deref(),
                metadata.commit_hash.as_deref(),
                metadata.commit_message.as_deref(),
                metadata.trigger_name.as_deref(),
            ]
        })
        .flatten()
    {
        support::validate_text(value, 1024)
            .map_err(|_| "Cloudflare returned invalid Workers build metadata.")?;
    }
    let status = build_status(&raw_status, build.build_outcome.as_deref());
    let updated = build
        .stopped_on
        .or(build.modified_on)
        .or(build.running_on)
        .unwrap_or(created);
    let terminal = matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    );
    let title = metadata
        .as_ref()
        .and_then(|metadata| metadata.commit_message.clone())
        .filter(|message| !message.trim().is_empty())
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|metadata| metadata.trigger_name.clone())
        })
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|metadata| metadata.commit_hash.as_deref())
                .map(|hash| format!("Build {}", short_hash(hash)))
        })
        .unwrap_or_else(|| format!("Build {script}"));
    let actor = metadata
        .as_ref()
        .and_then(|metadata| metadata.author.clone());
    let branch = metadata.and_then(|metadata| metadata.branch);
    if let Some(preview_url) = build.preview_url.as_deref() {
        support::validate_text(preview_url, 2048)
            .map_err(|_| "Cloudflare returned an invalid Workers preview URL.")?;
    }
    Ok(Activity {
        id: format!("cloudflare-worker-build:{id}"),
        provider: Provider::CloudflareWorkers,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        kind: BUILD.into(),
        title,
        workflow_name: None,
        project: script.into(),
        branch,
        url: worker_dashboard_url(account_id, script),
        status,
        raw_status: build.build_outcome.unwrap_or(raw_status),
        actor,
        ownership: Ownership::Unknown,
        started_at: created.to_rfc3339(),
        finished_at: terminal.then(|| updated.to_rfc3339()),
        updated_at: updated.to_rfc3339(),
    })
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

fn build_status(status: &str, outcome: Option<&str>) -> ActivityStatus {
    match status {
        "queued" | "initializing" => ActivityStatus::Queued,
        "running" => ActivityStatus::Running,
        "stopped" => match outcome {
            Some("success") => ActivityStatus::Success,
            Some("fail" | "terminated") => ActivityStatus::Failed,
            Some("cancelled") => ActivityStatus::Canceled,
            Some("skipped") => ActivityStatus::Skipped,
            _ => ActivityStatus::Unknown,
        },
        _ => ActivityStatus::Unknown,
    }
}

fn worker_dashboard_url(account_id: &str, script: &str) -> String {
    format!(
        "https://dash.cloudflare.com/{}/workers/services/view/{}/production",
        support::path_segment(account_id),
        support::path_segment(script)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: &str) -> Target {
        Target {
            id: format!("0123456789abcdef0123456789abcdef:sample-worker:{kind}"),
            provider: Provider::CloudflareWorkers,
            connection_id: "cloudflare-workers:00000000-0000-0000-0000-000000000001".into(),
            name: "sample-worker".into(),
            description: None,
            metadata: HashMap::from([
                (
                    "accountId".into(),
                    "0123456789abcdef0123456789abcdef".into(),
                ),
                ("scriptName".into(), "sample-worker".into()),
                (
                    "scriptTag".into(),
                    "fedcba9876543210fedcba9876543210".into(),
                ),
                ("kind".into(), kind.into()),
            ]),
        }
    }

    #[test]
    fn discovery_separates_deployment_and_build_targets() {
        let script = Script {
            id: "sample-worker".into(),
            tag: Some("fedcba9876543210fedcba9876543210".into()),
            modified_on: None,
        };
        let deployment = script_target(
            &script,
            "0123456789abcdef0123456789abcdef",
            "cloudflare-workers:00000000-0000-0000-0000-000000000001",
            DEPLOYMENT,
        );
        let build = script_target(
            &script,
            "0123456789abcdef0123456789abcdef",
            "cloudflare-workers:00000000-0000-0000-0000-000000000001",
            BUILD,
        );
        assert_eq!(deployment.metadata["kind"], DEPLOYMENT);
        assert_eq!(build.metadata["kind"], BUILD);
        assert_ne!(deployment.id, build.id);
    }

    #[test]
    fn worker_build_statuses_are_explicit() {
        assert_eq!(build_status("queued", None), ActivityStatus::Queued);
        assert_eq!(build_status("running", None), ActivityStatus::Running);
        assert_eq!(
            build_status("stopped", Some("success")),
            ActivityStatus::Success
        );
        assert_eq!(
            build_status("stopped", Some("terminated")),
            ActivityStatus::Failed
        );
        assert_eq!(
            build_status("stopped", Some("mystery")),
            ActivityStatus::Unknown
        );
    }

    #[test]
    fn hash_shortening_is_utf8_safe() {
        assert_eq!(short_hash("빌드해시abcdef123456"), "빌드해시abcdef12");
    }

    #[test]
    fn worker_build_uses_dashboard_url_for_safe_external_navigation() {
        let build = WorkerBuild {
            build_uuid: Some("00000000-0000-0000-0000-000000000003".into()),
            status: Some("running".into()),
            build_outcome: None,
            created_on: Some("2026-10-06T01:00:00Z".parse().unwrap()),
            initializing_on: None,
            modified_on: None,
            running_on: None,
            stopped_on: None,
            preview_url: Some("https://preview.sample.workers.dev".into()),
            build_trigger_metadata: Some(BuildTriggerMetadata {
                author: None,
                branch: Some("main".into()),
                commit_hash: Some("빌드해시abcdef123456".into()),
                commit_message: None,
                trigger_name: None,
            }),
        };
        let activity = map_build(
            build,
            &target(BUILD),
            "0123456789abcdef0123456789abcdef",
            "sample-worker",
        )
        .unwrap();
        assert_eq!(activity.title, "Build 빌드해시abcdef12");
        assert_eq!(
            activity.url,
            "https://dash.cloudflare.com/0123456789abcdef0123456789abcdef/workers/services/view/sample-worker/production"
        );
    }

    #[test]
    fn pagination_uses_result_info_and_full_page_fallback() {
        assert!(has_more(
            Some(cloudflare::ResultInfo {
                page: 1,
                total_pages: 3,
            }),
            1,
            1,
            PAGE_SIZE,
        ));
        assert!(!has_more(
            Some(cloudflare::ResultInfo {
                page: 3,
                total_pages: 3,
            }),
            1,
            3,
            PAGE_SIZE,
        ));
        assert!(has_more(None, PAGE_SIZE, 1, PAGE_SIZE));
    }

    #[test]
    fn active_scan_deadline_uses_one_absolute_budget() {
        let now = tokio::time::Instant::now();
        assert_eq!(active_deadline(now), now + ACTIVE_SCAN_BUDGET);
    }

    #[test]
    fn target_validation_requires_explicit_kind() {
        assert!(validate_target(&target(BUILD)).is_ok());
        let mut missing = target(BUILD);
        missing.metadata.remove("kind");
        assert!(validate_target(&missing).is_err());
    }
}
