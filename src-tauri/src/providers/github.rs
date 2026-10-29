use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use serde::Deserialize;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery, Issue,
        IssueKind, Ownership, Provider, Target,
    },
};

const MAX_REPOSITORY_PAGES: u32 = 5;
const MAX_RUN_PAGES: u32 = 5;
const MAX_ACTIVE_RUN_PAGES: u32 = 2;
const PAGE_SIZE: usize = 100;
const RECENT_PAGE_SIZE: usize = 20;
const ACTIVE_STATUSES: [&str; 5] = ["queued", "in_progress", "waiting", "pending", "requested"];

#[derive(Deserialize)]
struct User {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct Repository {
    full_name: String,
    description: Option<String>,
    html_url: String,
    owner: RepositoryOwner,
    name: String,
}

#[derive(Deserialize)]
struct RepositoryOwner {
    login: String,
}

#[derive(Deserialize)]
struct RunsResponse {
    total_count: usize,
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
    #[serde(default = "default_attempt")]
    run_attempt: u64,
    name: Option<String>,
    display_title: Option<String>,
    event: String,
    status: String,
    conclusion: Option<String>,
    head_branch: Option<String>,
    html_url: String,
    created_at: DateTime<Utc>,
    run_started_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    actor: Option<Actor>,
    triggering_actor: Option<Actor>,
}

#[derive(Deserialize)]
struct Actor {
    login: String,
}

fn default_attempt() -> u64 {
    1
}

pub async fn discover(cli: Option<&CliInvocation>, connection_id: &str) -> Discovery {
    let connection = connection(cli, connection_id).await;
    let Some(cli) = cli.filter(|_| connection.status == ConnectionStatus::Connected) else {
        return Discovery {
            connection,
            targets: Vec::new(),
            aws_profiles: Vec::new(),
            issues: Vec::new(),
        };
    };
    let api = super::api::ApiClient::github(cli);
    let (repositories, truncated) = match discover_repositories(&api).await {
        Ok(result) => result,
        Err(error) => return failed_discovery(error, connection.cli_version, connection_id),
    };
    let issues = truncated
        .then(|| Issue {
            kind: IssueKind::Other,
            provider: Provider::Github,
            connection_id: Some(connection_id.into()),
            message: format!(
                "GitHub repository discovery reached the {MAX_REPOSITORY_PAGES}-page safety limit. Some repositories may be missing."
            ),
            target_id: None,
        })
        .into_iter()
        .collect();
    let targets = match repositories
        .into_iter()
        .map(|repository| repository_target(repository, connection_id))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(targets) => targets,
        Err(error) => return failed_discovery(error, connection.cli_version, connection_id),
    };
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

pub async fn connection(cli: Option<&CliInvocation>, connection_id: &str) -> Connection {
    let Some(cli) = cli else {
        return failed_discovery(CliError::Unavailable, None, connection_id).connection;
    };
    let version = cli.version().await;
    let api = super::api::ApiClient::github(cli);
    match verified_user(&api).await {
        Ok(user) => Connection {
            kind: None,
            connection_id: connection_id.into(),
            provider: Provider::Github,
            status: ConnectionStatus::Connected,
            account: Some(user.login),
            message: None,
            cli_version: version,
        },
        Err(error) => failed_discovery(error, version, connection_id).connection,
    }
}

pub(crate) async fn github_cli_identity(cli: &CliInvocation) -> Result<(String, String), CliError> {
    let user: User = github_json(&super::api::ApiClient::github(cli), "user").await?;
    Ok((user.id.to_string(), user.login))
}

fn identity_matches(api: &super::api::ApiClient, actual: u64) -> bool {
    api.expected_identity()
        .is_none_or(|expected| expected == actual.to_string())
}

async fn verified_user(api: &super::api::ApiClient) -> Result<User, CliError> {
    let user: User = github_json(api, "user").await?;
    if identity_matches(api, user.id) {
        Ok(user)
    } else {
        Err(CliError::AccountMismatch)
    }
}

pub(super) async fn verify_cli_identity(cli: &CliInvocation) -> Result<(), CliError> {
    verified_user(&super::api::ApiClient::github(cli))
        .await
        .map(|_| ())
}

async fn discover_repositories(
    cli: &super::api::ApiClient,
) -> Result<(Vec<Repository>, bool), CliError> {
    let mut repositories = Vec::new();
    let mut full_last_page = false;
    for page in 1..=MAX_REPOSITORY_PAGES {
        let endpoint = format!(
            "user/repos?affiliation=owner,collaborator,organization_member&sort=updated&per_page={PAGE_SIZE}&page={page}"
        );
        let batch: Vec<Repository> = github_json(cli, &endpoint).await?;
        full_last_page = batch.len() == PAGE_SIZE;
        repositories.extend(batch);
        if !full_last_page {
            break;
        }
    }
    Ok((repositories, full_last_page))
}

fn repository_target(repository: Repository, connection_id: &str) -> Result<Target, CliError> {
    validate_github_url(&repository.html_url)?;
    Ok(Target {
        id: repository.full_name,
        provider: Provider::Github,
        connection_id: connection_id.into(),
        name: repository.name,
        description: repository.description,
        metadata: HashMap::from([
            ("owner".into(), repository.owner.login),
            ("url".into(), repository.html_url),
        ]),
    })
}

fn failed_discovery(error: CliError, version: Option<String>, connection_id: &str) -> Discovery {
    let status = match error {
        CliError::SignedOut => ConnectionStatus::SignedOut,
        CliError::Unavailable => ConnectionStatus::MissingCli,
        _ => ConnectionStatus::Error,
    };
    Discovery {
        connection: Connection {
            kind: Some(error.issue_kind()),
            connection_id: connection_id.into(),
            provider: Provider::Github,
            status,
            account: None,
            message: Some(error.user_message("GitHub")),
            cli_version: version,
        },
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: Vec::new(),
    }
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    let cli = super::api::ApiClient::github(cli);
    verified_user(&cli).await?;
    validate_github_target(target)?;
    let page_number = parse_page_cursor(cursor)?;
    let endpoint = runs_endpoint(&target.id, None, RECENT_PAGE_SIZE, page_number);
    let response: RunsResponse = github_json(&cli, &endpoint).await?;
    let next_cursor = next_page_cursor(
        page_number,
        RECENT_PAGE_SIZE,
        response.total_count,
        response.workflow_runs.len(),
    )?;
    let activities = response
        .workflow_runs
        .into_iter()
        .map(|run| map_run(run, target, identity))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ActivityPage {
        activities,
        next_cursor,
        issues: Vec::new(),
    })
}

pub async fn active(
    cli: &CliInvocation,
    target: &Target,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    let cli = super::api::ApiClient::github(cli);
    verified_user(&cli).await?;
    validate_github_target(target)?;
    let results = join_all(
        ACTIVE_STATUSES
            .iter()
            .map(|status| fetch_active_status(&cli, &target.id, status)),
    )
    .await;
    let mut runs = Vec::new();
    let mut truncated_statuses = Vec::new();
    let mut failures = Vec::new();
    for (status, (mut status_runs, truncated, error)) in ACTIVE_STATUSES.into_iter().zip(results) {
        runs.append(&mut status_runs);
        if truncated {
            truncated_statuses.push(status);
        }
        if let Some(error) = error {
            failures.push((status, error));
        }
    }
    let mut issues = Vec::new();
    if !truncated_statuses.is_empty() {
        issues.push(target_issue(
            target,
            format!(
                "GitHub active runs ({}) reached the {MAX_ACTIVE_RUN_PAGES}-page safety limit. Active results are partial.",
                truncated_statuses.join(", ")
            ),
            IssueKind::Other,
        ));
    }
    issues.extend(
        super::grouped_probe_failures(&failures)
            .into_iter()
            .map(|(error, statuses)| {
                target_issue(
                    target,
                    format!(
                        "GitHub could not read active runs ({}): {}",
                        statuses.join(", "),
                        error.user_message("GitHub")
                    ),
                    error.issue_kind(),
                )
            }),
    );
    let mut seen = HashSet::new();
    let activities = runs
        .into_iter()
        .map(|run| map_run(run, target, identity))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|activity| seen.insert(activity.id.clone()))
        .collect();
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

async fn fetch_active_status(
    cli: &super::api::ApiClient,
    repository: &str,
    status: &str,
) -> (Vec<WorkflowRun>, bool, Option<CliError>) {
    let mut runs = Vec::new();
    for page_number in 1..=MAX_ACTIVE_RUN_PAGES {
        let endpoint = runs_endpoint(repository, Some(status), PAGE_SIZE, page_number);
        let response: RunsResponse = match github_json(cli, &endpoint).await {
            Ok(response) => response,
            Err(error) => return (runs, false, Some(error)),
        };
        let has_next = match has_next_page(
            page_number,
            PAGE_SIZE,
            response.total_count,
            response.workflow_runs.len(),
        ) {
            Ok(has_next) => has_next,
            Err(error) => return (runs, false, Some(error)),
        };
        runs.extend(response.workflow_runs);
        if !has_next {
            return (runs, false, None);
        }
    }
    (runs, true, None)
}

fn runs_endpoint(repository: &str, status: Option<&str>, per_page: usize, page: u32) -> String {
    let status_query = status
        .map(|value| format!("status={value}&"))
        .unwrap_or_default();
    format!("repos/{repository}/actions/runs?{status_query}per_page={per_page}&page={page}")
}

fn parse_page_cursor(cursor: Option<&str>) -> Result<u32, CliError> {
    match cursor {
        None => Ok(1),
        Some(value) if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            value
                .parse::<u32>()
                .ok()
                .filter(|page| *page > 0)
                .ok_or(CliError::InvalidOutput)
        }
        Some(_) => Err(CliError::InvalidOutput),
    }
}

fn next_page_cursor(
    page: u32,
    page_size: usize,
    total_count: usize,
    returned: usize,
) -> Result<Option<String>, CliError> {
    has_next_page(page, page_size, total_count, returned)?
        .then(|| {
            page.checked_add(1)
                .map(|next| next.to_string())
                .ok_or(CliError::InvalidOutput)
        })
        .transpose()
}

fn has_next_page(
    page: u32,
    page_size: usize,
    total_count: usize,
    returned: usize,
) -> Result<bool, CliError> {
    if returned > page_size {
        return Err(CliError::InvalidOutput);
    }
    if returned == 0 {
        return Ok(false);
    }
    let offset = usize::try_from(page.saturating_sub(1))
        .ok()
        .and_then(|value| value.checked_mul(page_size))
        .ok_or(CliError::InvalidOutput)?;
    Ok(returned == page_size && offset.saturating_add(returned) < total_count)
}

fn target_issue(target: &Target, message: String, kind: IssueKind) -> Issue {
    Issue {
        kind,
        provider: Provider::Github,
        connection_id: Some(target.connection_id.clone()),
        message,
        target_id: Some(target.id.clone()),
    }
}

pub async fn collect(
    cli: Option<&CliInvocation>,
    targets: &[Target],
    history_hours: u32,
    account: Option<&str>,
    connection_id: &str,
) -> (Vec<Activity>, Vec<Issue>) {
    if targets.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let Some(cli) = cli else {
        return (
            Vec::new(),
            vec![Issue {
                kind: IssueKind::MissingCli,
                provider: Provider::Github,
                connection_id: Some(connection_id.into()),
                message: "GitHub CLI is unavailable. Cached activity is shown when available."
                    .into(),
                target_id: None,
            }],
        );
    };
    let cli = super::api::ApiClient::github(cli);
    if let Err(error) = verified_user(&cli).await {
        return (
            Vec::new(),
            vec![Issue {
                kind: error.issue_kind(),
                provider: Provider::Github,
                connection_id: Some(connection_id.into()),
                message: error.user_message("GitHub"),
                target_id: None,
            }],
        );
    }
    let cutoff = Utc::now() - Duration::hours(i64::from(history_hours));
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    for target in targets {
        match collect_repository(&cli, target, cutoff, account).await {
            Ok((mut target_activities, truncated)) => {
                activities.append(&mut target_activities);
                if truncated {
                    issues.push(Issue {
            kind: IssueKind::Other,
                        provider: Provider::Github,
                        connection_id: Some(connection_id.into()),
                        message: format!(
                            "GitHub history for {} reached the {MAX_RUN_PAGES}-page safety limit. Recent results are partial.",
                            target.name
                        ),
                        target_id: Some(target.id.clone()),
                    });
                }
            }
            Err(error) => issues.push(Issue {
                kind: error.issue_kind(),
                provider: Provider::Github,
                connection_id: Some(connection_id.into()),
                message: error.user_message("GitHub"),
                target_id: Some(target.id.clone()),
            }),
        }
    }
    (activities, issues)
}

async fn collect_repository(
    cli: &super::api::ApiClient,
    target: &Target,
    cutoff: DateTime<Utc>,
    account: Option<&str>,
) -> Result<(Vec<Activity>, bool), CliError> {
    validate_repository_id(&target.id).map_err(|_| CliError::InvalidOutput)?;
    let mut activities = Vec::new();
    let (completed, mut truncated) =
        fetch_runs(cli, &target.id, "completed", MAX_RUN_PAGES).await?;
    for run in completed.into_iter().filter(|run| run.updated_at >= cutoff) {
        activities.push(map_run(run, target, account)?);
    }
    for status in ACTIVE_STATUSES {
        let (active, active_truncated) =
            fetch_runs(cli, &target.id, status, MAX_ACTIVE_RUN_PAGES).await?;
        truncated |= active_truncated;
        for run in active {
            activities.push(map_run(run, target, account)?);
        }
    }
    let mut seen = std::collections::HashSet::new();
    activities.retain(|activity| seen.insert(activity.id.clone()));
    Ok((activities, truncated))
}

async fn fetch_runs(
    cli: &super::api::ApiClient,
    repository: &str,
    status: &str,
    max_pages: u32,
) -> Result<(Vec<WorkflowRun>, bool), CliError> {
    let mut runs = Vec::new();
    let mut full_last_page = false;
    for page in 1..=max_pages {
        let endpoint = runs_endpoint(repository, Some(status), PAGE_SIZE, page);
        let response: RunsResponse = github_json(cli, &endpoint).await?;
        full_last_page = response.workflow_runs.len() == PAGE_SIZE;
        runs.extend(response.workflow_runs);
        if !full_last_page {
            return Ok((runs, false));
        }
    }
    Ok((runs, full_last_page))
}

async fn github_json<T: for<'de> Deserialize<'de>>(
    cli: &super::api::ApiClient,
    endpoint: &str,
) -> Result<T, CliError> {
    cli.github_get(endpoint).await
}

fn map_run(run: WorkflowRun, target: &Target, account: Option<&str>) -> Result<Activity, CliError> {
    validate_github_url(&run.html_url)?;
    if run.updated_at < run.created_at
        || run
            .run_started_at
            .is_some_and(|started| started < run.created_at || started > run.updated_at)
    {
        return Err(CliError::InvalidOutput);
    }
    let current_actor = run
        .triggering_actor
        .as_ref()
        .or(run.actor.as_ref())
        .map(|actor| actor.login.clone());
    let ownership = match (current_actor.as_deref(), account) {
        (Some(actor), Some(account)) if actor.eq_ignore_ascii_case(account) => Ownership::Mine,
        (Some(_), Some(_)) => Ownership::Other,
        _ => Ownership::Unknown,
    };
    let raw_status = run.conclusion.clone().unwrap_or_else(|| run.status.clone());
    let status = github_status(&run.status, run.conclusion.as_deref())?;
    let started_at = run.run_started_at.unwrap_or(run.created_at);
    let finished_at = (run.status == "completed").then(|| run.updated_at.to_rfc3339());
    let workflow_name = run
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    Ok(Activity {
        id: format!("github:{}:{}", run.id, run.run_attempt),
        provider: Provider::Github,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: workflow_name.clone(),
        kind: run.event,
        title: run
            .display_title
            .or(workflow_name)
            .unwrap_or_else(|| "GitHub Actions run".into()),
        project: target.id.clone(),
        branch: run.head_branch,
        url: run.html_url,
        status,
        raw_status,
        actor: current_actor,
        ownership,
        started_at: started_at.to_rfc3339(),
        finished_at,
        updated_at: run.updated_at.to_rfc3339(),
    })
}

fn github_status(status: &str, conclusion: Option<&str>) -> Result<ActivityStatus, CliError> {
    if conclusion.is_some() && status != "completed" {
        return Err(CliError::InvalidOutput);
    }
    let mapped = match conclusion {
        Some("success") => ActivityStatus::Success,
        Some("failure" | "timed_out" | "action_required" | "startup_failure" | "stale") => {
            ActivityStatus::Failed
        }
        Some("cancelled") => ActivityStatus::Canceled,
        Some("skipped" | "neutral") => ActivityStatus::Skipped,
        Some(_) => return Err(CliError::InvalidOutput),
        None => match status {
            "queued" | "requested" | "pending" => ActivityStatus::Queued,
            "in_progress" => ActivityStatus::Running,
            "waiting" => ActivityStatus::Waiting,
            "completed" => ActivityStatus::Unknown,
            _ => return Err(CliError::InvalidOutput),
        },
    };
    Ok(mapped)
}

fn validate_github_target(target: &Target) -> Result<(), CliError> {
    if target.provider != Provider::Github {
        return Err(CliError::InvalidOutput);
    }
    validate_repository_id(&target.id).map_err(|_| CliError::InvalidOutput)
}

fn validate_github_url(value: &str) -> Result<(), CliError> {
    let parsed = url::Url::parse(value).map_err(|_| CliError::InvalidOutput)?;
    if parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.port_or_known_default() == Some(443)
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
    {
        Ok(())
    } else {
        Err(CliError::InvalidOutput)
    }
}

pub(crate) fn validate_repository_id(value: &str) -> Result<(), ()> {
    let mut parts = value.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
    };
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repository), None) if valid_part(owner) && valid_part(repository) => {
            Ok(())
        }
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unused_provider_never_requires_authentication() {
        let (activities, issues) = collect(None, &[], 24, None, "github:local").await;
        assert!(activities.is_empty());
        assert!(issues.is_empty());
    }

    fn test_run(actor: &str, triggering_actor: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id: 10,
            run_attempt: 2,
            name: Some("Deploy".into()),
            display_title: None,
            event: "workflow_dispatch".into(),
            status: "completed".into(),
            conclusion: Some("success".into()),
            head_branch: Some("main".into()),
            html_url: "https://github.com/acme/rocket/actions/runs/10".into(),
            created_at: Utc::now(),
            run_started_at: None,
            updated_at: Utc::now(),
            actor: Some(Actor {
                login: actor.into(),
            }),
            triggering_actor: triggering_actor.map(|login| Actor {
                login: login.into(),
            }),
        }
    }

    fn target() -> Target {
        Target {
            id: "acme/rocket".into(),
            provider: Provider::Github,
            connection_id: "github:local".into(),
            name: "rocket".into(),
            description: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn rerun_uses_current_attempt_actor_for_ownership() {
        let activity = map_run(
            test_run("automation", Some("octo-user")),
            &target(),
            Some("octo-user"),
        )
        .expect("map run");
        assert_eq!(activity.actor.as_deref(), Some("octo-user"));
        assert_eq!(activity.ownership, Ownership::Mine);
        assert_eq!(activity.id, "github:10:2");
    }

    #[test]
    fn github_status_mapping_is_explicit() {
        assert_eq!(
            github_status("in_progress", None),
            Ok(ActivityStatus::Running)
        );
        assert_eq!(
            github_status("completed", Some("cancelled")),
            Ok(ActivityStatus::Canceled)
        );
        assert_eq!(
            github_status("completed", Some("timed_out")),
            Ok(ActivityStatus::Failed)
        );
        assert_eq!(
            github_status("future_state", None),
            Err(CliError::InvalidOutput)
        );
        assert_eq!(
            github_status("in_progress", Some("success")),
            Err(CliError::InvalidOutput)
        );
    }

    #[test]
    fn repository_id_validation_blocks_path_injection() {
        assert!(validate_repository_id("acme/rocket").is_ok());
        assert!(validate_repository_id("acme/../secrets").is_err());
        assert!(validate_repository_id("acme/rocket?x=1").is_err());
    }

    #[test]
    fn active_and_completed_windows_are_independent() {
        let mut run = test_run("person", None);
        run.status = "in_progress".into();
        run.conclusion = None;
        run.updated_at = Utc::now() - Duration::hours(200);
        let cutoff = Utc::now() - Duration::hours(24);
        assert!(run.conclusion.is_none() && run.status != "completed");

        run.status = "completed".into();
        run.conclusion = Some("success".into());
        assert!(run.updated_at < cutoff);
    }

    #[test]
    fn recent_page_plans_one_twenty_item_request_and_numeric_cursor() {
        assert_eq!(
            runs_endpoint("acme/rocket", None, RECENT_PAGE_SIZE, 1),
            "repos/acme/rocket/actions/runs?per_page=20&page=1"
        );
        assert_eq!(parse_page_cursor(None), Ok(1));
        assert_eq!(parse_page_cursor(Some("7")), Ok(7));
        assert_eq!(
            parse_page_cursor(Some("7&status=queued")),
            Err(CliError::InvalidOutput)
        );
        assert_eq!(
            next_page_cursor(1, RECENT_PAGE_SIZE, 21, 20),
            Ok(Some("2".into()))
        );
        assert_eq!(next_page_cursor(2, RECENT_PAGE_SIZE, 21, 0), Ok(None));
    }

    #[test]
    fn active_probe_plan_is_five_parallel_status_queries_per_round() {
        let endpoints = ACTIVE_STATUSES
            .iter()
            .map(|status| runs_endpoint("acme/rocket", Some(status), PAGE_SIZE, 1))
            .collect::<Vec<_>>();
        assert_eq!(endpoints.len(), 5);
        assert_eq!(MAX_ACTIVE_RUN_PAGES as usize * endpoints.len(), 10);
        assert!(endpoints
            .iter()
            .any(|endpoint| endpoint.contains("status=in_progress")));
        assert!(endpoints
            .iter()
            .all(|endpoint| endpoint.contains("per_page=100")));
    }

    #[test]
    fn synthetic_workflow_response_parses_rerun_timestamps_and_actor() {
        let response: RunsResponse = serde_json::from_value(serde_json::json!({
            "total_count": 1,
            "workflow_runs": [{
                "id": 44,
                "run_attempt": 3,
                "name": "Deploy",
                "display_title": "Release",
                "event": "workflow_dispatch",
                "status": "in_progress",
                "conclusion": null,
                "head_branch": "main",
                "html_url": "https://github.com/example-org/rocket/actions/runs/44",
                "created_at": "2025-01-01T00:00:00Z",
                "run_started_at": "2026-09-29T01:02:03Z",
                "updated_at": "2026-09-29T01:03:00Z",
                "actor": { "login": "automation" },
                "triggering_actor": { "login": "sample-user" }
            }]
        }))
        .expect("synthetic response");
        let activity = map_run(
            response.workflow_runs.into_iter().next().expect("run"),
            &target(),
            Some("sample-user"),
        )
        .expect("map run");
        assert_eq!(activity.id, "github:44:3");
        assert_eq!(activity.title, "Release");
        assert_eq!(activity.workflow_name.as_deref(), Some("Deploy"));
        assert_eq!(activity.status, ActivityStatus::Running);
        assert_eq!(activity.started_at, "2026-09-29T01:02:03+00:00");
        assert_eq!(activity.ownership, Ownership::Mine);
    }

    #[test]
    fn untrusted_action_link_is_rejected() {
        let mut run = test_run("person", None);
        run.html_url = "https://github.com.example.invalid/acme/rocket/actions/runs/10".into();
        assert!(matches!(
            map_run(run, &target(), Some("person")),
            Err(CliError::InvalidOutput)
        ));
    }
}
