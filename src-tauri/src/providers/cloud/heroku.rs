use std::{collections::HashMap, time::Duration};

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
    parse_timestamp, provider_url, terminal_time, valid_slug, validate_target, PAGE_SIZE,
};

const PROVIDER: Provider = Provider::Heroku;
const LABEL: &str = "Heroku";

#[derive(Deserialize)]
struct App {
    name: String,
    #[serde(default)]
    archived_at: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    id: String,
    version: u64,
    description: String,
    status: String,
    created_at: String,
    updated_at: String,
    user: Option<ReleaseUser>,
}

#[derive(Deserialize)]
struct ReleaseUser {
    email: String,
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let Some(cli) = cli else {
        return missing_cli(account, PROVIDER, LABEL);
    };
    let connection = connection(Some(cli), account).await;
    if connection.status != ConnectionStatus::Connected {
        return discovery_from_connection(connection);
    }
    let apps: Vec<App> = match cli.json(&["apps", "--json"]).await {
        Ok(apps) => apps,
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
    let targets = apps
        .into_iter()
        .filter(|app| app.archived_at.is_none())
        .filter_map(|app| match app_target(app, &account.id) {
            Ok(target) => Some(target),
            Err(_) => {
                issues.push(issue(
                    PROVIDER,
                    &account.id,
                    None,
                    "Heroku returned an app with an invalid name; it was skipped.",
                ));
                None
            }
        })
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
    match cli.run(&["auth:whoami"], Duration::from_secs(10)).await {
        Ok(output) if !output.trim().is_empty() && !output.contains('\0') => Connection {
            kind: None,
            connection_id: account.id.clone(),
            provider: PROVIDER,
            status: ConnectionStatus::Connected,
            account: Some(output.trim().to_owned()),
            message: Some(
                "Heroku release history includes code, config variable, and add-on changes; it is not a build-only feed."
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
    if cursor.is_some() {
        return Err(CliError::InvalidOutput);
    }
    let app = metadata(target, "app")?;
    if !valid_slug(app) {
        return Err(CliError::InvalidOutput);
    }
    let releases: Vec<Release> = cli.json(&release_args(app)).await?;
    map_releases(releases, target, app)
}

fn release_args(app: &str) -> [&str; 6] {
    ["releases", "--app", app, "--json", "--num", "20"]
}

fn app_target(app: App, connection_id: &str) -> Result<Target, CliError> {
    if !valid_slug(&app.name) {
        return Err(CliError::InvalidOutput);
    }
    Ok(Target {
        id: format!("heroku:{}", app.name),
        provider: PROVIDER,
        connection_id: connection_id.into(),
        name: app.name.clone(),
        description: Some("Heroku app releases".into()),
        metadata: HashMap::from([("app".into(), app.name)]),
    })
}

fn map_releases(
    releases: Vec<Release>,
    target: &Target,
    app: &str,
) -> Result<ActivityPage, CliError> {
    let truncated = releases.len() >= PAGE_SIZE;
    let mut issues = Vec::new();
    let activities = releases
        .into_iter()
        .take(PAGE_SIZE)
        .map(|release| map_release(release, target, app, &mut issues))
        .collect::<Result<Vec<_>, _>>()?;
    if truncated {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            "Heroku CLI returned the requested 20-release window and does not expose a continuation cursor; older releases may exist.",
        ));
    }
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

fn map_release(
    release: Release,
    target: &Target,
    app: &str,
    issues: &mut Vec<crate::models::Issue>,
) -> Result<Activity, CliError> {
    if !valid_slug(app)
        || release.id.trim().is_empty()
        || release.version == 0
        || release.description.trim().is_empty()
    {
        return Err(CliError::InvalidOutput);
    }
    let started_at = parse_timestamp(&release.created_at)?;
    let updated_at = parse_timestamp(&release.updated_at)?;
    let status = heroku_status(&release.status);
    if status == ActivityStatus::Unknown {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            format!(
                "Heroku release v{} has an unrecognized status '{}'.",
                release.version, release.status
            ),
        ));
    }
    let actor = release
        .user
        .map(|user| user.email)
        .filter(|email| !email.trim().is_empty());
    Ok(Activity {
        id: format!("heroku:{}:{}", app, release.id),
        provider: PROVIDER,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: "release".into(),
        title: format!("v{} {}", release.version, release.description),
        project: app.into(),
        branch: None,
        url: provider_url("dashboard.heroku.com", &["apps", app, "activity"])?,
        status,
        raw_status: release.status,
        actor,
        ownership: Ownership::Unknown,
        started_at,
        finished_at: terminal_time(status, &updated_at),
        updated_at,
    })
}

fn heroku_status(status: &str) -> ActivityStatus {
    match status {
        "pending" => ActivityStatus::Running,
        "succeeded" => ActivityStatus::Success,
        "failed" => ActivityStatus::Failed,
        "expired" => ActivityStatus::Canceled,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "heroku:example-app".into(),
            provider: PROVIDER,
            connection_id: "heroku:local".into(),
            name: "example-app".into(),
            description: None,
            metadata: HashMap::from([("app".into(), "example-app".into())]),
        }
    }

    #[test]
    fn parses_release_fixture_and_maps_platform_status() {
        let fixture = r#"[{
          "id":"00000000-0000-0000-0000-000000000001", "version":7,
          "description":"Deploy example commit", "status":"succeeded",
          "created_at":"2026-09-28T10:00:00Z", "updated_at":"2026-09-28T10:01:00Z",
          "user":{"email":"person@example.invalid"}
        }]"#;
        let releases: Vec<Release> = serde_json::from_str(fixture).expect("fixture");
        let page = map_releases(releases, &target(), "example-app").expect("page");
        assert_eq!(page.activities[0].status, ActivityStatus::Success);
        assert_eq!(page.activities[0].ownership, Ownership::Unknown);
        assert!(page.activities[0]
            .url
            .starts_with("https://dashboard.heroku.com/"));
    }

    #[test]
    fn request_is_scoped_and_continuation_is_rejected() {
        assert_eq!(
            release_args("example-app"),
            ["releases", "--app", "example-app", "--json", "--num", "20"]
        );
        assert_eq!(heroku_status("pending"), ActivityStatus::Running);
        assert_eq!(heroku_status("unexpected"), ActivityStatus::Unknown);
    }
}
