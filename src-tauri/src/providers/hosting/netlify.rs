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

const LABEL: &str = "Netlify";
const PAGE_SIZE: usize = 20;
const SITE_PAGE_SIZE: usize = 100;
const MAX_SITE_PAGES: u32 = 5;

#[derive(Clone, Deserialize)]
struct NetlifyUser {
    id: String,
    full_name: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
struct SiteRow {
    id: String,
    name: String,
    account_name: Option<String>,
    build_settings: Option<BuildSettings>,
}

#[derive(Deserialize)]
struct BuildSettings {
    repo_url: Option<String>,
    repo_branch: Option<String>,
}

#[derive(Deserialize)]
struct DeployRow {
    id: String,
    user_id: Option<String>,
    state: String,
    name: Option<String>,
    branch: Option<String>,
    skipped: Option<bool>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    published_at: Option<DateTime<Utc>>,
    title: Option<String>,
    context: Option<String>,
}

pub async fn connection(cli: Option<&CliInvocation>, account: &Account) -> Connection {
    let Some(cli) = cli else {
        return support::missing_cli(account, LABEL, "netlify");
    };
    let version = cli.version().await;
    match current_user(cli).await {
        Ok(user) => connected(&account.id, version, &user),
        Err(error) => support::failed_connection(account, LABEL, error, version),
    }
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let Some(cli) = cli else {
        return support::discovery_from_connection(support::missing_cli(account, LABEL, "netlify"));
    };
    let version = cli.version().await;
    let user = match current_user(cli).await {
        Ok(user) => user,
        Err(error) => return support::failed_discovery(account, LABEL, error, version),
    };
    let connection = connected(&account.id, version, &user);
    let (sites, truncated) = match discover_sites(cli).await {
        Ok(result) => result,
        Err(error) => {
            return support::failed_discovery(account, LABEL, error, connection.cli_version);
        }
    };

    let mut targets = Vec::new();
    let mut issues = Vec::new();
    for site in sites {
        match site_target(site, &account.id) {
            Ok(target) => targets.push(target),
            Err(_) => issues.push(support::issue(
                Provider::Netlify,
                &account.id,
                None,
                "Netlify returned a project without a valid ID or name.",
            )),
        }
    }
    if truncated {
        issues.push(support::issue(
            Provider::Netlify,
            &account.id,
            None,
            format!(
                "Netlify project discovery reached the {MAX_SITE_PAGES}-page safety limit. Some projects may be missing."
            ),
        ));
    }
    Discovery {
        connection,
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

fn connected(connection_id: &str, version: Option<String>, user: &NetlifyUser) -> Connection {
    Connection {
        connection_id: connection_id.to_owned(),
        provider: Provider::Netlify,
        status: ConnectionStatus::Connected,
        account: Some(user.id.clone()),
        message: None,
        cli_version: version,
    }
}

async fn current_user(cli: &CliInvocation) -> Result<NetlifyUser, CliError> {
    let user: NetlifyUser = cli.json(&["api", "getCurrentUser"]).await?;
    support::validate_id(&user.id)?;
    if user
        .full_name
        .as_deref()
        .is_some_and(|value| support::validate_text(value, 512).is_err())
        || user
            .email
            .as_deref()
            .is_some_and(|value| support::validate_text(value, 512).is_err())
    {
        return Err(CliError::InvalidOutput);
    }
    Ok(user)
}

async fn discover_sites(cli: &CliInvocation) -> Result<(Vec<SiteRow>, bool), CliError> {
    let mut sites = Vec::new();
    let mut full_last_page = false;
    for page in 1..=MAX_SITE_PAGES {
        let data = site_request_data(page);
        let batch: Vec<SiteRow> = cli.json(&["api", "listSites", "--data", &data]).await?;
        full_last_page = batch.len() == SITE_PAGE_SIZE;
        sites.extend(batch);
        if !full_last_page {
            break;
        }
    }
    Ok((sites, full_last_page))
}

fn site_request_data(page: u32) -> String {
    format!(r#"{{"filter":"all","page":{page},"per_page":{SITE_PAGE_SIZE}}}"#)
}

fn site_target(site: SiteRow, connection_id: &str) -> Result<Target, CliError> {
    support::validate_id(&site.id)?;
    support::validate_text(&site.name, 256)?;
    let description = [
        site.account_name.as_deref(),
        site.build_settings
            .as_ref()
            .and_then(|settings| settings.repo_url.as_deref()),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty() && value.len() <= 512 && !value.contains('\0'))
    .collect::<Vec<_>>()
    .join(" · ");
    let mut metadata = HashMap::from([("siteId".into(), site.id.clone())]);
    if let Some(branch) = site
        .build_settings
        .and_then(|settings| settings.repo_branch)
        .filter(|branch| !branch.trim().is_empty() && branch.len() <= 512 && !branch.contains('\0'))
    {
        metadata.insert("defaultBranch".into(), branch);
    }
    Ok(Target {
        id: site.id,
        provider: Provider::Netlify,
        connection_id: connection_id.to_owned(),
        name: site.name,
        description: (!description.is_empty()).then_some(description),
        metadata,
    })
}

pub async fn page(
    cli: &CliInvocation,
    target: &Target,
    cursor: Option<&str>,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    if let Some(identity) = identity {
        support::validate_id(identity)?;
    }
    let page_number = parse_cursor(cursor)?;
    let site_id = support::metadata(target, "siteId")?;
    let data = deploy_request_data(site_id, page_number);
    let rows: Vec<DeployRow> = cli
        .json(&["api", "listSiteDeploys", "--data", &data])
        .await?;
    deploy_page(rows, target, page_number, identity)
}

fn parse_cursor(cursor: Option<&str>) -> Result<u32, CliError> {
    match cursor {
        None => Ok(1),
        Some(value)
            if !value.is_empty() && value.chars().all(|character| character.is_ascii_digit()) =>
        {
            value
                .parse::<u32>()
                .ok()
                .filter(|page| (2..=10_000).contains(page))
                .ok_or(CliError::InvalidOutput)
        }
        Some(_) => Err(CliError::InvalidOutput),
    }
}

fn deploy_request_data(site_id: &str, page: u32) -> String {
    format!(r#"{{"site_id":"{site_id}","page":{page},"per_page":{PAGE_SIZE}}}"#)
}

fn deploy_page(
    rows: Vec<DeployRow>,
    target: &Target,
    page_number: u32,
    identity: Option<&str>,
) -> Result<ActivityPage, CliError> {
    let full_page = rows.len() == PAGE_SIZE;
    let activities = rows
        .into_iter()
        .map(|row| map_deploy(row, target, identity))
        .collect::<Result<Vec<_>, _>>()?;
    let reached_limit = full_page && page_number == 10_000;
    Ok(ActivityPage {
        activities,
        next_cursor: (full_page && !reached_limit).then(|| (page_number + 1).to_string()),
        issues: reached_limit
            .then(|| {
                support::issue(
                    Provider::Netlify,
                    &target.connection_id,
                    Some(&target.id),
                    "Netlify history reached ratelline's 10,000-page cursor safety limit.",
                )
            })
            .into_iter()
            .collect(),
    })
}

fn map_deploy(
    row: DeployRow,
    target: &Target,
    identity: Option<&str>,
) -> Result<Activity, CliError> {
    support::validate_id(&row.id)?;
    support::validate_text(&row.state, 128)?;
    for value in [
        row.title.as_deref(),
        row.name.as_deref(),
        row.branch.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        support::validate_text(value, 512)?;
    }
    if let Some(actor) = row.user_id.as_deref() {
        support::validate_id(actor)?;
    }
    let actor = row.user_id.clone();
    let ownership = match (row.user_id.as_deref(), identity) {
        (Some(actor), Some(current)) if actor == current => Ownership::Mine,
        (Some(_), Some(_)) => Ownership::Other,
        _ => Ownership::Unknown,
    };
    let status = if row.skipped == Some(true) {
        ActivityStatus::Skipped
    } else {
        netlify_status(&row.state)
    };
    let terminal = matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    );
    let finished_at = terminal.then(|| row.published_at.unwrap_or(row.updated_at).to_rfc3339());
    Ok(Activity {
        id: format!("netlify:{}", row.id),
        provider: Provider::Netlify,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: row.context.unwrap_or_else(|| "deploy".into()),
        title: row
            .title
            .or(row.name)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("{} deploy", target.name)),
        project: target.name.clone(),
        branch: row
            .branch
            .or_else(|| target.metadata.get("defaultBranch").cloned()),
        url: format!(
            "https://app.netlify.com/sites/{}/deploys/{}",
            support::path_segment(&target.name),
            support::path_segment(&row.id)
        ),
        status,
        raw_status: row.state,
        actor,
        ownership,
        started_at: row.created_at.to_rfc3339(),
        finished_at,
        updated_at: row.updated_at.to_rfc3339(),
    })
}

fn netlify_status(state: &str) -> ActivityStatus {
    match state {
        "new" | "enqueued" | "preparing" => ActivityStatus::Queued,
        "pending_review" | "accepted" | "uploaded" | "prepared" | "processed" => {
            ActivityStatus::Waiting
        }
        "building" | "uploading" | "processing" | "retrying" => ActivityStatus::Running,
        "ready" => ActivityStatus::Success,
        "error" | "rejected" => ActivityStatus::Failed,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "site-1".into(),
            provider: Provider::Netlify,
            connection_id: "netlify:local".into(),
            name: "demo-site".into(),
            description: None,
            metadata: HashMap::from([
                ("siteId".into(), "site-1".into()),
                ("currentUserId".into(), "user-1".into()),
            ]),
        }
    }

    #[test]
    fn builds_bounded_api_requests_without_shell_interpolation() {
        assert_eq!(
            site_request_data(2),
            r#"{"filter":"all","page":2,"per_page":100}"#
        );
        assert_eq!(
            deploy_request_data("site-1", 3),
            r#"{"site_id":"site-1","page":3,"per_page":20}"#
        );
        assert_eq!(parse_cursor(Some("3")), Ok(3));
        assert_eq!(parse_cursor(Some("--help")), Err(CliError::InvalidOutput));
    }

    #[test]
    fn parses_and_maps_synthetic_deploy_response() {
        let rows: Vec<DeployRow> = serde_json::from_str(
            r#"[{"id":"deploy-1","site_id":"site-1","user_id":"user-1","state":"ready","name":"demo-site","branch":"main","created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:06:05Z","published_at":"2026-01-02T03:06:00Z","title":"Ship demo","context":"production"}]"#,
        )
        .unwrap();
        let page = deploy_page(rows, &target(), 1, Some("user-1")).unwrap();
        assert_eq!(page.activities[0].status, ActivityStatus::Success);
        assert_eq!(page.activities[0].ownership, Ownership::Mine);
        assert!(page.activities[0]
            .url
            .starts_with("https://app.netlify.com/"));
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn same_display_name_does_not_share_connection_identity() {
        let first: NetlifyUser =
            serde_json::from_str(r#"{"id":"user-1","full_name":"Demo User"}"#).unwrap();
        let second: NetlifyUser =
            serde_json::from_str(r#"{"id":"user-2","full_name":"Demo User"}"#).unwrap();
        let first_connection = connected("netlify:local", None, &first);
        let second_connection = connected("netlify:local", None, &second);
        assert_eq!(first_connection.account.as_deref(), Some("user-1"));
        assert_eq!(second_connection.account.as_deref(), Some("user-2"));
        assert_ne!(first_connection.account, second_connection.account);
    }

    #[test]
    fn spoofed_metadata_cannot_grant_mine_ownership() {
        // The target contains currentUserId=user-1, matching this deploy's actor.
        for (identity, expected) in [
            (Some("user-2"), Ownership::Other),
            (None, Ownership::Unknown),
            (Some("user-1"), Ownership::Mine),
        ] {
            let rows: Vec<DeployRow> = serde_json::from_str(
                r#"[{"id":"deploy-1","user_id":"user-1","state":"ready","created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:06:05Z"}]"#,
            ).unwrap();
            let page = deploy_page(rows, &target(), 1, identity).unwrap();
            assert_eq!(page.activities[0].ownership, expected);
        }
    }

    #[test]
    fn malformed_dates_fail_at_the_response_boundary() {
        let result = serde_json::from_str::<Vec<DeployRow>>(
            r#"[{"id":"deploy-1","state":"ready","created_at":"yesterday","updated_at":"2026-01-02T03:06:05Z"}]"#,
        );
        assert!(result.is_err());
    }
}
