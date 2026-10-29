use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, TimeZone, Utc};
use futures::future::join_all;
use serde::Deserialize;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery, Issue,
        IssueKind, Ownership, Provider, Target,
    },
};

const MAX_TEAM_PAGES: usize = 3;
const MAX_PROJECT_PAGES: usize = 5;
const MAX_DEPLOYMENT_PAGES: usize = 5;
const PAGE_SIZE: usize = 100;
const RECENT_PAGE_SIZE: usize = 20;
const MAX_ACTIVE_DEPLOYMENT_PAGES: usize = 2;
const ACTIVE_STATES: [&str; 3] = ["BUILDING", "INITIALIZING", "QUEUED"];

#[derive(Clone, Deserialize)]
struct UserEnvelope {
    user: VercelUser,
}

#[derive(Clone, Deserialize)]
struct VercelUser {
    id: String,
    username: String,
}

#[derive(Deserialize)]
struct TeamsResponse {
    teams: Vec<Team>,
    pagination: Pagination,
}

#[derive(Deserialize)]
struct Team {
    id: String,
    slug: String,
}

#[derive(Deserialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
    pagination: Pagination,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    name: String,
    #[serde(default)]
    framework: Option<String>,
}

#[derive(Deserialize)]
struct DeploymentsResponse {
    deployments: Vec<Deployment>,
    pagination: Pagination,
}

#[derive(Deserialize)]
struct Pagination {
    next: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Deployment {
    uid: String,
    name: String,
    inspector_url: Option<String>,
    created: i64,
    ready: Option<i64>,
    updated_at: Option<i64>,
    state: String,
    creator: Option<Creator>,
    #[serde(default)]
    meta: HashMap<String, serde_json::Value>,
    target: Option<String>,
}

#[derive(Deserialize)]
struct Creator {
    uid: Option<String>,
    username: Option<String>,
}

#[derive(Clone, Copy)]
struct VerifiedIdentity<'a> {
    uid: Option<&'a str>,
    username: Option<&'a str>,
}

struct ProjectContext {
    project: Project,
    scope_id: Option<String>,
    scope_slug: String,
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
    let api = super::api::ApiClient::vercel(cli);
    let (teams, teams_truncated) = match discover_teams(&api).await {
        Ok(teams) => teams,
        Err(error) => return failed_discovery(error, connection.cli_version, connection_id),
    };
    // Current Vercel accounts, including Hobby accounts, keep projects in teams.
    // The CLI explicitly rejects a user's personal username as --scope.
    let contexts = teams.into_iter().map(|team| (Some(team.id), team.slug));
    let mut targets = Vec::new();
    let mut issues = Vec::new();
    if teams_truncated {
        issues.push(Issue {
            kind: IssueKind::Other,
            provider: Provider::Vercel,
            connection_id: Some(connection_id.into()),
            message: format!(
                "Vercel team discovery reached the {MAX_TEAM_PAGES}-page safety limit."
            ),
            target_id: None,
        });
    }
    for (scope_id, scope_slug) in contexts {
        match discover_projects(&api, scope_id.as_deref(), &scope_slug).await {
            Ok((projects, truncated)) => {
                targets.extend(
                    projects
                        .into_iter()
                        .map(|project| project_target(project, connection_id)),
                );
                if truncated {
                    issues.push(Issue {
            kind: IssueKind::Other,
                        provider: Provider::Vercel,
                        connection_id: Some(connection_id.into()),
                        message: format!(
                            "Vercel project discovery for {scope_slug} reached the {MAX_PROJECT_PAGES}-page safety limit."
                        ),
                        target_id: None,
                    });
                }
            }
            Err(error) => issues.push(Issue {
                kind: error.issue_kind(),
                provider: Provider::Vercel,
                connection_id: Some(connection_id.into()),
                message: error.user_message("Vercel"),
                target_id: None,
            }),
        }
    }
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
    let api = super::api::ApiClient::vercel(cli);
    match current_user(&api).await {
        Ok(user) if identity_matches(&api, &user.id) => Connection {
            kind: None,
            connection_id: connection_id.into(),
            provider: Provider::Vercel,
            status: ConnectionStatus::Connected,
            account: Some(user.username),
            message: None,
            cli_version: version,
        },
        Ok(_) => failed_discovery(CliError::AccountMismatch, version, connection_id).connection,
        Err(error) => failed_discovery(error, version, connection_id).connection,
    }
}

fn identity_matches(api: &super::api::ApiClient, actual: &str) -> bool {
    api.expected_identity()
        .is_none_or(|expected| expected == actual)
}

async fn discover_teams(cli: &super::api::ApiClient) -> Result<(Vec<Team>, bool), CliError> {
    let mut teams = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_TEAM_PAGES {
        let mut endpoint = format!("/v2/teams?limit={PAGE_SIZE}");
        if let Some(next) = cursor.as_deref() {
            endpoint.push_str("&until=");
            endpoint.push_str(next);
        }
        // This membership endpoint is account-wide; project/deployment calls below
        // always receive an explicit team scope, independent of currentTeam.
        let response: TeamsResponse = vercel_json(cli, &endpoint, None).await?;
        teams.extend(response.teams);
        let next_cursor = pagination_cursor(response.pagination)?;
        if next_cursor.is_some() && next_cursor == cursor {
            return Err(CliError::InvalidOutput);
        }
        cursor = next_cursor;
        if cursor.is_none() {
            return Ok((teams, false));
        }
    }
    Ok((teams, cursor.is_some()))
}

async fn discover_projects(
    cli: &super::api::ApiClient,
    scope_id: Option<&str>,
    scope_slug: &str,
) -> Result<(Vec<ProjectContext>, bool), CliError> {
    if let Some(scope) = scope_id {
        validate_vercel_id(scope).map_err(|_| CliError::InvalidOutput)?;
    }
    let mut projects = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PROJECT_PAGES {
        let mut endpoint = format!("/v9/projects?limit={PAGE_SIZE}");
        if let Some(next) = cursor.as_deref() {
            endpoint.push_str("&until=");
            endpoint.push_str(next);
        }
        let response: ProjectsResponse = vercel_json(cli, &endpoint, Some(scope_slug)).await?;
        projects.extend(response.projects.into_iter().map(|project| ProjectContext {
            project,
            scope_id: scope_id.map(str::to_owned),
            scope_slug: scope_slug.to_owned(),
        }));
        let next_cursor = pagination_cursor(response.pagination)?;
        if next_cursor.is_some() && next_cursor == cursor {
            return Err(CliError::InvalidOutput);
        }
        cursor = next_cursor;
        if cursor.is_none() {
            return Ok((projects, false));
        }
    }
    Ok((projects, cursor.is_some()))
}

fn project_target(context: ProjectContext, connection_id: &str) -> Target {
    let description = Some(match context.project.framework.as_deref() {
        Some(framework) => format!("{} · {framework}", context.scope_slug),
        None => context.scope_slug.clone(),
    });
    let id = match context.scope_id.as_deref() {
        Some(scope) => format!("{scope}:{}", context.project.id),
        None => format!("personal:{}", context.project.id),
    };
    let mut metadata = HashMap::from([
        ("projectId".into(), context.project.id),
        ("scopeSlug".into(), context.scope_slug),
    ]);
    if let Some(scope) = context.scope_id {
        metadata.insert("teamId".into(), scope);
    }
    Target {
        id,
        provider: Provider::Vercel,
        connection_id: connection_id.into(),
        name: context.project.name,
        description,
        metadata,
    }
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
            provider: Provider::Vercel,
            status,
            account: None,
            message: Some(error.user_message("Vercel")),
            cli_version: version,
        },
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: Vec::new(),
    }
}

pub async fn page(
    invocation: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    let cli = super::api::ApiClient::vercel(invocation);
    verified_user(&cli).await?;
    let (project_id, scope) = validate_vercel_target(target)?;
    let endpoint = deployment_endpoint(project_id, RECENT_PAGE_SIZE, cursor, None)?;
    let response: DeploymentsResponse = vercel_json(&cli, &endpoint, scope).await?;
    let next_cursor = pagination_cursor(response.pagination)?;
    if next_cursor
        .as_deref()
        .is_some_and(|next| Some(next) == cursor)
    {
        return Err(CliError::InvalidOutput);
    }
    let verified_identity = VerifiedIdentity {
        uid: None,
        username: identity,
    };
    let activities = response
        .deployments
        .into_iter()
        .map(|deployment| map_deployment(deployment, target, verified_identity))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ActivityPage {
        activities,
        next_cursor,
        issues: Vec::new(),
    })
}

pub async fn active(
    invocation: &CliInvocation,
    target: &Target,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    let cli = super::api::ApiClient::vercel(invocation);
    verified_user(&cli).await?;
    let (project_id, scope) = validate_vercel_target(target)?;
    let results = join_all(
        ACTIVE_STATES
            .iter()
            .map(|state| fetch_active_state(&cli, project_id, scope, state)),
    )
    .await;
    let mut deployments = Vec::new();
    let mut truncated_states = Vec::new();
    let mut failures = Vec::new();
    for (state, (mut state_deployments, truncated, error)) in ACTIVE_STATES.into_iter().zip(results)
    {
        deployments.append(&mut state_deployments);
        if truncated {
            truncated_states.push(state);
        }
        if let Some(error) = error {
            failures.push((state, error));
        }
    }
    let mut issues = Vec::new();
    if !truncated_states.is_empty() {
        issues.push(target_issue(
            target,
            format!(
                "Vercel active deployments ({}) reached the {MAX_ACTIVE_DEPLOYMENT_PAGES}-page safety limit. Active results are partial.",
                truncated_states.join(", ")
            ),
            IssueKind::Other,
        ));
    }
    issues.extend(
        super::grouped_probe_failures(&failures)
            .into_iter()
            .map(|(error, states)| {
                target_issue(
                    target,
                    format!(
                        "Vercel could not read active deployments ({}): {}",
                        states.join(", "),
                        error.user_message("Vercel")
                    ),
                    error.issue_kind(),
                )
            }),
    );
    let verified_identity = VerifiedIdentity {
        uid: None,
        username: identity,
    };
    let mut seen = HashSet::new();
    let activities = deployments
        .into_iter()
        .map(|deployment| map_deployment(deployment, target, verified_identity))
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

async fn fetch_active_state(
    cli: &super::api::ApiClient,
    project_id: &str,
    scope: Option<&str>,
    state: &str,
) -> (Vec<Deployment>, bool, Option<CliError>) {
    let mut deployments = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_ACTIVE_DEPLOYMENT_PAGES {
        let endpoint =
            match deployment_endpoint(project_id, PAGE_SIZE, cursor.as_deref(), Some(state)) {
                Ok(endpoint) => endpoint,
                Err(error) => return (deployments, false, Some(error)),
            };
        let response: DeploymentsResponse = match vercel_json(cli, &endpoint, scope).await {
            Ok(response) => response,
            Err(error) => return (deployments, false, Some(error)),
        };
        deployments.extend(response.deployments);
        let next_cursor = match pagination_cursor(response.pagination) {
            Ok(cursor) => cursor,
            Err(error) => return (deployments, false, Some(error)),
        };
        if next_cursor.is_some() && next_cursor == cursor {
            return (deployments, false, Some(CliError::InvalidOutput));
        }
        cursor = next_cursor;
        if cursor.is_none() {
            return (deployments, false, None);
        }
    }
    (deployments, true, None)
}

fn deployment_endpoint(
    project_id: &str,
    limit: usize,
    cursor: Option<&str>,
    state: Option<&str>,
) -> Result<String, CliError> {
    validate_vercel_id(project_id).map_err(|_| CliError::InvalidOutput)?;
    if let Some(cursor) = cursor {
        validate_timestamp_cursor(cursor)?;
    }
    if state.is_some_and(|value| !ACTIVE_STATES.contains(&value)) {
        return Err(CliError::InvalidOutput);
    }
    let mut endpoint = format!("/v7/deployments?projectId={project_id}&limit={limit}");
    if let Some(cursor) = cursor {
        endpoint.push_str("&until=");
        endpoint.push_str(cursor);
    }
    if let Some(state) = state {
        endpoint.push_str("&state=");
        endpoint.push_str(state);
    }
    Ok(endpoint)
}

fn validate_timestamp_cursor(value: &str) -> Result<(), CliError> {
    if !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<i64>().is_ok_and(|timestamp| timestamp > 0)
    {
        Ok(())
    } else {
        Err(CliError::InvalidOutput)
    }
}

fn validate_vercel_target(target: &Target) -> Result<(&str, Option<&str>), CliError> {
    if target.provider != Provider::Vercel {
        return Err(CliError::InvalidOutput);
    }
    let project_id = target
        .metadata
        .get("projectId")
        .ok_or(CliError::InvalidOutput)?;
    let scope = target
        .metadata
        .get("scopeSlug")
        .map(String::as_str)
        .ok_or(CliError::InvalidOutput)?;
    validate_vercel_id(project_id).map_err(|_| CliError::InvalidOutput)?;
    validate_scope(scope).map_err(|_| CliError::InvalidOutput)?;
    if let Some(team_id) = target.metadata.get("teamId") {
        validate_vercel_id(team_id).map_err(|_| CliError::InvalidOutput)?;
    }
    Ok((project_id, Some(scope)))
}

fn target_issue(target: &Target, message: String, kind: IssueKind) -> Issue {
    Issue {
        kind,
        provider: Provider::Vercel,
        connection_id: Some(target.connection_id.clone()),
        message,
        target_id: Some(target.id.clone()),
    }
}

pub async fn collect(
    cli: Option<&CliInvocation>,
    targets: &[Target],
    history_hours: u32,
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
                provider: Provider::Vercel,
                connection_id: Some(connection_id.into()),
                message: "Vercel CLI is unavailable. Cached activity is shown when available."
                    .into(),
                target_id: None,
            }],
        );
    };
    let cli = super::api::ApiClient::vercel(cli);
    let identity = match verified_user(&cli).await {
        Ok(identity) => Some(identity),
        Err(error) => {
            return (
                Vec::new(),
                vec![Issue {
                    kind: error.issue_kind(),
                    provider: Provider::Vercel,
                    connection_id: Some(connection_id.into()),
                    message: error.user_message("Vercel"),
                    target_id: None,
                }],
            )
        }
    };
    let cutoff = Utc::now() - Duration::hours(i64::from(history_hours));
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    for target in targets {
        let verified_identity = identity.as_ref().map(|user| VerifiedIdentity {
            uid: Some(user.id.as_str()),
            username: Some(user.username.as_str()),
        });
        match collect_project(&cli, target, cutoff, verified_identity).await {
            Ok((mut target_activities, truncated)) => {
                activities.append(&mut target_activities);
                if truncated {
                    issues.push(Issue {
            kind: IssueKind::Other,
                        provider: Provider::Vercel,
                        connection_id: Some(connection_id.into()),
                        message: format!(
                            "Vercel history for {} reached the {MAX_DEPLOYMENT_PAGES}-page safety limit. Recent results are partial.",
                            target.name
                        ),
                        target_id: Some(target.id.clone()),
                    });
                }
            }
            Err(error) => issues.push(Issue {
                kind: error.issue_kind(),
                provider: Provider::Vercel,
                connection_id: Some(connection_id.into()),
                message: error.user_message("Vercel"),
                target_id: Some(target.id.clone()),
            }),
        }
    }
    (activities, issues)
}

async fn collect_project(
    cli: &super::api::ApiClient,
    target: &Target,
    cutoff: DateTime<Utc>,
    identity: Option<VerifiedIdentity<'_>>,
) -> Result<(Vec<Activity>, bool), CliError> {
    let (project_id, scope) = validate_vercel_target(target)?;
    let mut activities = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_DEPLOYMENT_PAGES {
        let endpoint = deployment_endpoint(project_id, PAGE_SIZE, cursor.as_deref(), None)?;
        let response: DeploymentsResponse = vercel_json(cli, &endpoint, scope).await?;
        for deployment in response.deployments {
            let terminal_time = deployment
                .ready
                .or(deployment.updated_at)
                .and_then(timestamp);
            if !is_terminal(&deployment.state)
                || terminal_time.is_none_or(|finished| finished >= cutoff)
            {
                activities.push(map_deployment(
                    deployment,
                    target,
                    identity.unwrap_or(VerifiedIdentity {
                        uid: None,
                        username: None,
                    }),
                )?);
            }
        }
        let next_cursor = pagination_cursor(response.pagination)?;
        if next_cursor.is_some() && next_cursor == cursor {
            return Err(CliError::InvalidOutput);
        }
        cursor = next_cursor;
        if cursor.is_none() {
            return Ok((activities, false));
        }
    }
    Ok((activities, cursor.is_some()))
}

async fn current_user(cli: &super::api::ApiClient) -> Result<VercelUser, CliError> {
    vercel_json::<UserEnvelope>(cli, "/v2/user", None)
        .await
        .map(|response| response.user)
}

pub(crate) async fn vercel_cli_identity(cli: &CliInvocation) -> Result<(String, String), CliError> {
    let user = current_user(&super::api::ApiClient::vercel(cli)).await?;
    Ok((user.id, user.username))
}

async fn verified_user(cli: &super::api::ApiClient) -> Result<VercelUser, CliError> {
    let user = current_user(cli).await?;
    if identity_matches(cli, &user.id) {
        Ok(user)
    } else {
        Err(CliError::AccountMismatch)
    }
}

async fn vercel_json<T: for<'de> Deserialize<'de>>(
    cli: &super::api::ApiClient,
    endpoint: &str,
    scope: Option<&str>,
) -> Result<T, CliError> {
    cli.vercel_get(endpoint, scope).await
}

fn validate_scope(value: &str) -> Result<(), ()> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        Ok(())
    } else {
        Err(())
    }
}

fn pagination_cursor(pagination: Pagination) -> Result<Option<String>, CliError> {
    let Some(value) = pagination.next else {
        return Ok(None);
    };
    let cursor = match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Number(value) => value.to_string(),
        _ => return Err(CliError::InvalidOutput),
    };
    validate_timestamp_cursor(&cursor)?;
    Ok(Some(cursor))
}

fn map_deployment(
    deployment: Deployment,
    target: &Target,
    identity: VerifiedIdentity<'_>,
) -> Result<Activity, CliError> {
    validate_vercel_id(&deployment.uid).map_err(|_| CliError::InvalidOutput)?;
    let created = timestamp(deployment.created).ok_or(CliError::InvalidOutput)?;
    let ready = optional_timestamp(deployment.ready)?;
    let updated_at = optional_timestamp(deployment.updated_at)?;
    if ready.is_some_and(|value| value < created) || updated_at.is_some_and(|value| value < created)
    {
        return Err(CliError::InvalidOutput);
    }
    let terminal_time = ready.or(updated_at);
    let actor = deployment
        .creator
        .as_ref()
        .and_then(|creator| creator.username.clone().or_else(|| creator.uid.clone()));
    let ownership = deployment_ownership(deployment.creator.as_ref(), identity);
    let branch = deployment_meta_text(
        &deployment.meta,
        &[
            "githubCommitRef",
            "gitlabCommitRef",
            "bitbucketCommitRef",
            "gitCommitRef",
        ],
    );
    let title = deployment_meta_text(
        &deployment.meta,
        &[
            "githubCommitMessage",
            "gitlabCommitMessage",
            "bitbucketCommitMessage",
            "gitCommitMessage",
        ],
    )
    .unwrap_or_else(|| deployment.name.clone());
    let url = deployment
        .inspector_url
        .filter(|url| is_trusted_vercel_url(url))
        .unwrap_or_else(|| deployment_dashboard_url(target, &deployment.uid));
    let status = vercel_status(&deployment.state)?;
    let finished_at = is_terminal(&deployment.state)
        .then(|| terminal_time.map(|value| value.to_rfc3339()))
        .flatten();
    Ok(Activity {
        id: format!("vercel:{}", deployment.uid),
        provider: Provider::Vercel,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: super::execution_name(Some(&deployment.name)),
        kind: deployment.target.unwrap_or_else(|| "preview".into()),
        title,
        project: target.name.clone(),
        branch,
        url,
        status,
        raw_status: deployment.state,
        actor,
        ownership,
        started_at: created.to_rfc3339(),
        finished_at,
        updated_at: terminal_time.unwrap_or(created).to_rfc3339(),
    })
}

fn deployment_meta_text(
    meta: &HashMap<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        meta.get(*key)
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.lines().map(str::trim).find(|line| !line.is_empty()))
            .map(str::to_owned)
    })
}

fn deployment_ownership(creator: Option<&Creator>, identity: VerifiedIdentity<'_>) -> Ownership {
    let Some(creator) = creator else {
        return Ownership::Unknown;
    };
    if let (Some(expected), Some(actual)) = (identity.uid, creator.uid.as_deref()) {
        return if expected == actual {
            Ownership::Mine
        } else {
            Ownership::Other
        };
    }
    if let (Some(expected), Some(actual)) = (identity.username, creator.username.as_deref()) {
        return if expected == actual {
            Ownership::Mine
        } else {
            Ownership::Other
        };
    }
    Ownership::Unknown
}

fn is_trusted_vercel_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("vercel.com")
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

fn deployment_dashboard_url(target: &Target, deployment_id: &str) -> String {
    let encode =
        |value: &str| url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>();
    let scope = target
        .metadata
        .get("scopeSlug")
        .map(String::as_str)
        .unwrap_or("dashboard");
    format!(
        "https://vercel.com/{}/{}/{}",
        encode(scope),
        encode(&target.name),
        encode(deployment_id)
    )
}

fn timestamp(milliseconds: i64) -> Option<DateTime<Utc>> {
    (milliseconds > 0)
        .then(|| Utc.timestamp_millis_opt(milliseconds).single())
        .flatten()
}

fn optional_timestamp(milliseconds: Option<i64>) -> Result<Option<DateTime<Utc>>, CliError> {
    milliseconds
        .map(|value| timestamp(value).ok_or(CliError::InvalidOutput))
        .transpose()
}

fn is_terminal(state: &str) -> bool {
    matches!(state, "READY" | "ERROR" | "CANCELED" | "BLOCKED")
}

fn vercel_status(state: &str) -> Result<ActivityStatus, CliError> {
    let mapped = match state {
        "QUEUED" | "INITIALIZING" => ActivityStatus::Queued,
        "BUILDING" => ActivityStatus::Running,
        "BLOCKED" => ActivityStatus::Skipped,
        "READY" => ActivityStatus::Success,
        "ERROR" => ActivityStatus::Failed,
        "CANCELED" => ActivityStatus::Canceled,
        _ => return Err(CliError::InvalidOutput),
    };
    Ok(mapped)
}

fn validate_vercel_id(value: &str) -> Result<(), ()> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(test)]
#[path = "vercel_tests.rs"]
mod tests;
