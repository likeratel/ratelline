use std::collections::HashMap;

use crate::{
    cli::{CliError, CliInvocation},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery,
        Issue, Ownership, Provider, Target,
    },
};

use super::{failed_connection, missing_cli};
use api::*;
use auth::{access_token, ensure_default_gcloud_auth, projects, MAX_PROJECTS};

mod api;
mod auth;
pub use auth::identity;

const MAX_LIST_PAGES: usize = 50;
const MAX_ACTIVE_PAGES: usize = 2;
const PAGE_SIZE: &str = "100";
const HOSTING: &str = "hosting";
const APP_HOSTING: &str = "appHosting";
pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    if account.provider != Provider::Firebase {
        return failed_discovery(account, None, CliError::InvalidOutput);
    }
    let Some(cli) = cli else {
        return discovery_from_connection(missing_cli(account, "Google Cloud"));
    };
    let version = cli.version().await;
    let api = match FirebaseApi::from_cli(cli).await {
        Ok(api) => api,
        Err(message) => {
            return discovery_from_connection(Connection::disconnected(
                &account.id,
                account.provider,
                ConnectionStatus::Error,
                message,
            ))
        }
    };
    let projects = match projects(cli, &api.identity).await {
        Ok(projects) => projects,
        Err(error) => return failed_discovery(account, version, error),
    };
    let connection = connected(account, version, &api.identity);
    let mut targets = Vec::new();
    let mut issues = Vec::new();
    if projects.len() == MAX_PROJECTS {
        issues.push(issue(
            account,
            None,
            "Firebase project discovery is limited to 25 active Google Cloud projects.",
        ));
    }
    for project in projects {
        if !valid_project(&project) {
            issues.push(issue(
                account,
                None,
                "Google Cloud returned a project with invalid metadata; it was skipped.",
            ));
            continue;
        }
        discover_hosting_sites(&api, account, &project, &mut targets, &mut issues).await;
        discover_app_hosting(&api, account, &project, &mut targets, &mut issues).await;
    }
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    if account.provider != Provider::Firebase {
        return failed_connection(account, None, "Firebase", CliError::InvalidOutput);
    }
    let Some(cli) = cli else {
        return missing_cli(account, "Google Cloud");
    };
    let version = cli.version().await;
    match FirebaseApi::from_cli(cli).await {
        Ok(api) => connected(account, version, &api.identity),
        Err(message) => Connection::disconnected(
            &account.id,
            account.provider,
            ConnectionStatus::Error,
            message,
        ),
    }
}

pub async fn page_api(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, String> {
    validate_target(target)?;
    let api = FirebaseApi::from_cli(cli).await?;
    let project = target.metadata["projectId"].as_str();
    match target.metadata["kind"].as_str() {
        HOSTING => hosting_page(&api, target, project, cursor).await,
        APP_HOSTING => app_hosting_page(&api, target, project, cursor).await,
        _ => Err("The Firebase target kind is invalid.".into()),
    }
}

pub async fn active_api(cli: &CliInvocation, target: &Target) -> Result<ActivityPage, String> {
    let deadline = active_deadline(tokio::time::Instant::now());
    validate_target(target)?;
    if target.metadata["kind"] == HOSTING {
        return Ok(ActivityPage::default());
    }
    if target.metadata["kind"] != APP_HOSTING {
        return Err("The Firebase target kind is invalid.".into());
    }

    let api = tokio::time::timeout_at(deadline, FirebaseApi::from_cli(cli))
        .await
        .map_err(|_| {
            "Firebase App Hosting active scan timed out during gcloud authentication.".to_owned()
        })??;
    let project = target.metadata["projectId"].as_str();
    let backend = target.metadata["backendId"].as_str();
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    let mut token: Option<String> = None;
    for page in 0..MAX_ACTIVE_PAGES {
        let request = fetch_rollouts(&api, target, project, token.as_deref());
        let result = match tokio::time::timeout_at(deadline, request).await {
            Ok(result) => result,
            Err(_) => {
                Err("Firebase App Hosting active scan reached the monitoring deadline.".to_owned())
            }
        };
        let response = match result {
            Ok(response) => response,
            Err(message) if page > 0 => {
                issues.push(target_issue(
                    target,
                    format!(
                        "Firebase App Hosting active scan stopped after a partial result: {message}"
                    ),
                ));
                break;
            }
            Err(message) => return Err(message),
        };
        issues.extend(response.unreachable.into_iter().map(|location| {
            target_issue(
                target,
                format!(
                    "Firebase App Hosting could not reach location {location}. Active results are partial."
                ),
            )
        }));
        for rollout in response.rollouts {
            match map_rollout(rollout, target, project, backend) {
                Ok(activity) if is_active(&activity) => activities.push(activity),
                Ok(_) => {}
                Err(message) => issues.push(target_issue(
                    target,
                    format!("{message} The invalid rollout was skipped."),
                )),
            }
        }
        token = (!response.next_page_token.is_empty()).then_some(response.next_page_token);
        if token.is_none() {
            break;
        }
        if page + 1 == MAX_ACTIVE_PAGES {
            issues.push(target_issue(
                target,
                "Firebase App Hosting active scan reached ratelline's 200-rollout safety limit. Active results are partial.",
            ));
        }
    }
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
) -> Result<ActivityPage, CliError> {
    page_api(cli, target, cursor)
        .await
        .map_err(|_| CliError::InvalidOutput)
}

async fn discover_hosting_sites(
    api: &FirebaseApi,
    account: &Account,
    project: &Project,
    targets: &mut Vec<Target>,
    issues: &mut Vec<Issue>,
) {
    let mut token = String::new();
    for page in 0..MAX_LIST_PAGES {
        let mut query = vec![("pageSize", PAGE_SIZE)];
        if !token.is_empty() {
            query.push(("pageToken", token.as_str()));
        }
        let path = ["v1beta1", "projects", project.project_id.as_str(), "sites"];
        let response: SitesResponse =
            match api.hosting_get(&project.project_id, &path, &query).await {
                Ok(response) => response,
                Err(message) => {
                    issues.push(issue(
                        account,
                        None,
                        format!("{}: {message}", project.project_id),
                    ));
                    return;
                }
            };
        for site in response.sites {
            match hosting_target(account, project, site) {
                Ok(target) => targets.push(target),
                Err(message) => issues.push(issue(account, None, message)),
            }
        }
        token = response.next_page_token;
        if token.is_empty() {
            return;
        }
        if page + 1 == MAX_LIST_PAGES {
            issues.push(issue(
                account,
                None,
                format!(
                    "Firebase Hosting site discovery for {} reached ratelline's pagination limit. Results are partial.",
                    project.project_id
                ),
            ));
        }
    }
}

async fn discover_app_hosting(
    api: &FirebaseApi,
    account: &Account,
    project: &Project,
    targets: &mut Vec<Target>,
    issues: &mut Vec<Issue>,
) {
    let locations = match list_locations(api, &project.project_id).await {
        Ok(locations) => locations,
        Err(message) => {
            issues.push(issue(
                account,
                None,
                format!("{}: {message}", project.project_id),
            ));
            return;
        }
    };
    for location in locations {
        if !valid_resource_id(&location.location_id)
            || location.name
                != format!(
                    "projects/{}/locations/{}",
                    project.project_id, location.location_id
                )
        {
            issues.push(issue(
                account,
                None,
                "Firebase App Hosting returned an invalid location; it was skipped.",
            ));
            continue;
        }
        let mut token = String::new();
        for page in 0..MAX_LIST_PAGES {
            let mut query = vec![("pageSize", PAGE_SIZE)];
            if !token.is_empty() {
                query.push(("pageToken", token.as_str()));
            }
            let path = [
                "v1beta",
                "projects",
                project.project_id.as_str(),
                "locations",
                location.location_id.as_str(),
                "backends",
            ];
            let response: BackendsResponse = match api
                .app_hosting_get(&project.project_id, &path, &query)
                .await
            {
                Ok(response) => response,
                Err(message) => {
                    issues.push(issue(
                        account,
                        None,
                        format!(
                            "{} · {}: {message}",
                            project.project_id, location.location_id
                        ),
                    ));
                    break;
                }
            };
            for unreachable in &response.unreachable {
                issues.push(issue(
                    account,
                    None,
                    format!("Firebase App Hosting could not reach location {unreachable}. Results are partial."),
                ));
            }
            for backend in response.backends {
                match app_hosting_target(account, project, &location.location_id, backend) {
                    Ok(target) => targets.push(target),
                    Err(message) => issues.push(issue(account, None, message)),
                }
            }
            token = response.next_page_token;
            if token.is_empty() {
                break;
            }
            if page + 1 == MAX_LIST_PAGES {
                issues.push(issue(
                    account,
                    None,
                    format!(
                        "Firebase App Hosting backend discovery for {} reached ratelline's pagination limit. Results are partial.",
                        project.project_id
                    ),
                ));
            }
        }
    }
}

async fn list_locations(api: &FirebaseApi, project: &str) -> Result<Vec<Location>, String> {
    let mut locations = Vec::new();
    let mut token = String::new();
    for page in 0..MAX_LIST_PAGES {
        let mut query = vec![("pageSize", PAGE_SIZE)];
        if !token.is_empty() {
            query.push(("pageToken", token.as_str()));
        }
        let path = ["v1beta", "projects", project, "locations"];
        let response: LocationsResponse = api.app_hosting_get(project, &path, &query).await?;
        locations.extend(response.locations);
        token = response.next_page_token;
        if token.is_empty() {
            return Ok(locations);
        }
        if page + 1 == MAX_LIST_PAGES {
            return Err(
                "Firebase App Hosting location discovery reached ratelline's pagination limit."
                    .into(),
            );
        }
    }
    Ok(locations)
}

fn hosting_target(account: &Account, project: &Project, site: Site) -> Result<Target, String> {
    let site_id = resource_tail(&site.name, "sites")?;
    if !valid_resource_id(site_id) || !valid_https_url(&site.default_url) {
        return Err(
            "Firebase Hosting returned a site with invalid metadata; it was skipped.".into(),
        );
    }
    Ok(Target {
        id: format!("{}:hosting:{site_id}", project.project_id),
        provider: Provider::Firebase,
        connection_id: account.id.clone(),
        name: site_id.into(),
        description: Some(format!("{} · {}", project.name, site.default_url)),
        metadata: HashMap::from([
            ("kind".into(), HOSTING.into()),
            ("projectId".into(), project.project_id.clone()),
            ("siteId".into(), site_id.into()),
        ]),
    })
}

fn app_hosting_target(
    account: &Account,
    project: &Project,
    location: &str,
    backend: Backend,
) -> Result<Target, String> {
    let backend_id = resource_tail(&backend.name, "backends")?;
    if !valid_resource_id(backend_id)
        || !valid_resource_id(location)
        || (!backend.uri.is_empty() && !valid_https_url(&backend.uri))
        || (!backend.display_name.is_empty() && !valid_text(&backend.display_name, 512))
    {
        return Err(
            "Firebase App Hosting returned a backend with invalid metadata; it was skipped.".into(),
        );
    }
    let name = if backend.display_name.trim().is_empty() {
        backend_id.to_owned()
    } else {
        backend.display_name
    };
    Ok(Target {
        id: format!("{}:apphosting:{location}:{backend_id}", project.project_id),
        provider: Provider::Firebase,
        connection_id: account.id.clone(),
        name,
        description: Some(format!("{} · {location}", project.name)),
        metadata: HashMap::from([
            ("kind".into(), APP_HOSTING.into()),
            ("projectId".into(), project.project_id.clone()),
            ("location".into(), location.into()),
            ("backendId".into(), backend_id.into()),
        ]),
    })
}

async fn hosting_page(
    api: &FirebaseApi,
    target: &Target,
    project: &str,
    cursor: Option<&str>,
) -> Result<ActivityPage, String> {
    let site = target.metadata["siteId"].as_str();
    let token = parse_cursor(cursor, HOSTING)?;
    let mut query = vec![("pageSize", PAGE_SIZE)];
    if let Some(token) = token.as_deref() {
        query.push(("pageToken", token));
    }
    let path = ["v1beta1", "sites", site, "releases"];
    let response: ReleasesResponse = api.hosting_get(project, &path, &query).await?;
    let mut activities = Vec::new();
    let mut issues = Vec::new();
    for release in response.releases {
        match map_release(release, target, project, site, &api.identity) {
            Ok(activity) => activities.push(activity),
            Err(message) => issues.push(target_issue(
                target,
                format!("{message} The invalid release was skipped."),
            )),
        }
    }
    Ok(ActivityPage {
        activities,
        next_cursor: make_cursor(HOSTING, response.next_page_token),
        issues,
    })
}

async fn app_hosting_page(
    api: &FirebaseApi,
    target: &Target,
    project: &str,
    cursor: Option<&str>,
) -> Result<ActivityPage, String> {
    let backend = target.metadata["backendId"].as_str();
    let token = parse_cursor(cursor, APP_HOSTING)?;
    let response = fetch_rollouts(api, target, project, token.as_deref()).await?;
    let mut activities = Vec::new();
    let mut issues = response
        .unreachable
        .into_iter()
        .map(|location| {
            target_issue(
                target,
                format!(
                    "Firebase App Hosting could not reach location {location}. Results are partial."
                ),
            )
        })
        .collect::<Vec<_>>();
    for rollout in response.rollouts {
        match map_rollout(rollout, target, project, backend) {
            Ok(activity) => activities.push(activity),
            Err(message) => issues.push(target_issue(
                target,
                format!("{message} The invalid rollout was skipped."),
            )),
        }
    }
    Ok(ActivityPage {
        activities,
        next_cursor: make_cursor(APP_HOSTING, response.next_page_token),
        issues,
    })
}

async fn fetch_rollouts(
    api: &FirebaseApi,
    target: &Target,
    project: &str,
    token: Option<&str>,
) -> Result<RolloutsResponse, String> {
    let location = target.metadata["location"].as_str();
    let backend = target.metadata["backendId"].as_str();
    let mut query = vec![("pageSize", PAGE_SIZE), ("orderBy", "createTime desc")];
    if let Some(token) = token {
        query.push(("pageToken", token));
    }
    let path = [
        "v1beta",
        "projects",
        project,
        "locations",
        location,
        "backends",
        backend,
        "rollouts",
    ];
    api.app_hosting_get(project, &path, &query).await
}

fn map_release(
    release: Release,
    target: &Target,
    project: &str,
    site: &str,
    identity: &str,
) -> Result<Activity, String> {
    let id = resource_tail(&release.name, "releases")?;
    if !valid_resource_id(id)
        || !valid_text(&release.release_type, 128)
        || (!release.message.is_empty() && !valid_text(&release.message, 512))
    {
        return Err("Firebase Hosting returned an invalid release.".into());
    }
    let actor = release
        .release_user
        .map(|user| user.email)
        .filter(|email| valid_identity(email));
    let ownership = match actor.as_deref() {
        Some(actual) if actual == identity => Ownership::Mine,
        Some(_) => Ownership::Other,
        None => Ownership::Unknown,
    };
    let version = release.version.filter(|version| {
        (version.name.is_empty() || valid_text(&version.name, 1024))
            && (version.status.is_empty() || valid_text(&version.status, 128))
    });
    let raw_status = version
        .as_ref()
        .map(|version| {
            if version.status.is_empty() {
                release.release_type.clone()
            } else {
                version.status.clone()
            }
        })
        .unwrap_or_else(|| release.release_type.clone());
    let timestamp = release.release_time.to_rfc3339();
    Ok(Activity {
        id: format!("firebase-hosting:{}:{id}", target.id),
        provider: Provider::Firebase,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        kind: HOSTING.into(),
        title: if release.message.trim().is_empty() {
            format!("Release {site}")
        } else {
            release.message
        },
        workflow_name: None,
        project: target.name.clone(),
        branch: None,
        url: hosting_console_url(project, site),
        status: ActivityStatus::Success,
        raw_status,
        actor,
        ownership,
        started_at: timestamp.clone(),
        finished_at: Some(timestamp.clone()),
        updated_at: timestamp,
    })
}

fn map_rollout(
    rollout: Rollout,
    target: &Target,
    project: &str,
    backend: &str,
) -> Result<Activity, String> {
    let id = resource_tail(&rollout.name, "rollouts")?;
    if !valid_resource_id(id)
        || !valid_text(&rollout.state, 128)
        || (!rollout.display_name.is_empty() && !valid_text(&rollout.display_name, 512))
        || (!rollout.build.is_empty() && !valid_text(&rollout.build, 1024))
    {
        return Err("Firebase App Hosting returned an invalid rollout.".into());
    }
    let status = rollout_status(&rollout.state);
    let terminal = matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    );
    Ok(Activity {
        id: format!("firebase-app-hosting:{}:{id}", target.id),
        provider: Provider::Firebase,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        kind: APP_HOSTING.into(),
        title: if rollout.display_name.trim().is_empty() {
            format!("Rollout {backend}")
        } else {
            rollout.display_name
        },
        workflow_name: None,
        project: target.name.clone(),
        branch: None,
        url: app_hosting_console_url(project),
        status,
        raw_status: rollout.state,
        actor: None,
        ownership: Ownership::Unknown,
        started_at: rollout.create_time.to_rfc3339(),
        finished_at: terminal.then(|| rollout.update_time.to_rfc3339()),
        updated_at: rollout.update_time.to_rfc3339(),
    })
}

fn validate_target(target: &Target) -> Result<(), String> {
    target.validate()?;
    if target.provider != Provider::Firebase {
        return Err("The selected target is not a Firebase resource.".into());
    }
    for key in ["kind", "projectId"] {
        metadata(target, key)?;
    }
    match target.metadata["kind"].as_str() {
        HOSTING => {
            metadata(target, "siteId")?;
        }
        APP_HOSTING => {
            metadata(target, "location")?;
            metadata(target, "backendId")?;
        }
        _ => return Err("The Firebase target kind is invalid.".into()),
    }
    Ok(())
}

fn metadata<'a>(target: &'a Target, key: &str) -> Result<&'a str, String> {
    let value = target
        .metadata
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("The Firebase target has no {key}."))?;
    if valid_resource_id(value) {
        Ok(value)
    } else {
        Err("The Firebase target metadata is invalid.".into())
    }
}

fn parse_cursor(cursor: Option<&str>, kind: &str) -> Result<Option<String>, String> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let token = cursor
        .strip_prefix(kind)
        .and_then(|value| value.strip_prefix(':'))
        .filter(|value| {
            !value.is_empty() && value.len() <= 2048 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "The Firebase history cursor is invalid.".to_owned())?;
    Ok(Some(token.to_owned()))
}

fn make_cursor(kind: &str, token: String) -> Option<String> {
    (!token.is_empty()).then(|| format!("{kind}:{token}"))
}

fn rollout_status(value: &str) -> ActivityStatus {
    match value {
        "QUEUED" | "PENDING_BUILD" => ActivityStatus::Queued,
        "PROGRESSING" => ActivityStatus::Running,
        "PAUSED" => ActivityStatus::Waiting,
        "SUCCEEDED" => ActivityStatus::Success,
        "FAILED" => ActivityStatus::Failed,
        "CANCELLED" => ActivityStatus::Canceled,
        "SKIPPED" => ActivityStatus::Skipped,
        _ => ActivityStatus::Unknown,
    }
}

fn is_active(activity: &Activity) -> bool {
    matches!(
        activity.status,
        ActivityStatus::Queued | ActivityStatus::Running | ActivityStatus::Waiting
    )
}

fn connected(account: &Account, version: Option<String>, identity: &str) -> Connection {
    Connection {
        kind: None,
        connection_id: account.id.clone(),
        provider: Provider::Firebase,
        status: ConnectionStatus::Connected,
        account: Some(identity.into()),
        message: None,
        cli_version: version,
    }
}

fn failed_discovery(account: &Account, version: Option<String>, error: CliError) -> Discovery {
    discovery_from_connection(failed_connection(account, version, "Firebase", error))
}

fn discovery_from_connection(connection: Connection) -> Discovery {
    Discovery {
        connection,
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: Vec::new(),
    }
}

fn issue(account: &Account, target_id: Option<&str>, message: impl Into<String>) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider: Provider::Firebase,
        connection_id: Some(account.id.clone()),
        target_id: target_id.map(str::to_owned),
        message: message.into(),
    }
}

fn target_issue(target: &Target, message: impl Into<String>) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider: Provider::Firebase,
        connection_id: Some(target.connection_id.clone()),
        target_id: Some(target.id.clone()),
        message: message.into(),
    }
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 512
        && value.contains('@')
        && !value.chars().any(char::is_control)
        && !value.chars().any(char::is_whitespace)
}

fn valid_project(project: &Project) -> bool {
    valid_resource_id(&project.project_id)
        && valid_text(&project.name, 512)
        && project.lifecycle_state == "ACTIVE"
}

fn valid_resource_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.contains('\0')
}

fn valid_https_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn resource_tail<'a>(name: &'a str, collection: &str) -> Result<&'a str, String> {
    let parts = name.split('/').collect::<Vec<_>>();
    parts
        .windows(2)
        .rev()
        .find_map(|pair| (pair[0] == collection).then_some(pair[1]))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Firebase returned an invalid resource name.".to_owned())
}

fn hosting_console_url(project: &str, site: &str) -> String {
    format!("https://console.firebase.google.com/project/{project}/hosting/sites/{site}")
}

fn app_hosting_console_url(project: &str) -> String {
    format!("https://console.firebase.google.com/project/{project}/apphosting")
}

#[cfg(test)]
mod tests;
