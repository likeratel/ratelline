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

const PROVIDER: Provider = Provider::Fly;
const LABEL: &str = "Fly.io";

#[derive(Deserialize)]
struct App {
    #[serde(alias = "Name")]
    name: String,
    #[serde(default, alias = "Organization")]
    organization: Option<AppOrganization>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AppOrganization {
    Label(String),
    Details {
        #[serde(default, alias = "Slug")]
        slug: Option<String>,
        #[serde(default, alias = "Name")]
        name: Option<String>,
    },
}

impl AppOrganization {
    fn label(self) -> Option<String> {
        match self {
            Self::Label(value) => (!value.trim().is_empty()).then_some(value),
            Self::Details { slug, name } => slug
                .filter(|value| !value.trim().is_empty())
                .or_else(|| name.filter(|value| !value.trim().is_empty())),
        }
    }
}

#[derive(Deserialize)]
struct Release {
    #[serde(alias = "ID")]
    id: String,
    #[serde(alias = "Version")]
    version: u64,
    #[serde(alias = "Status")]
    status: String,
    #[serde(alias = "Description")]
    description: String,
    #[serde(alias = "CreatedAt", alias = "createdAt")]
    created_at: String,
    #[serde(default, alias = "User")]
    user: Option<ReleaseUser>,
}

#[derive(Deserialize)]
struct ReleaseUser {
    #[serde(default, alias = "Email")]
    email: Option<String>,
    #[serde(default, alias = "Name")]
    name: Option<String>,
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let Some(cli) = cli else {
        return missing_cli(account, PROVIDER, LABEL);
    };
    let connection = connection(Some(cli), account).await;
    if connection.status != ConnectionStatus::Connected {
        return discovery_from_connection(connection);
    }
    let apps: Vec<App> = match cli.json(&["apps", "list", "--json"]).await {
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
        .filter_map(|app| match app_target(app, &account.id) {
            Ok(target) => Some(target),
            Err(_) => {
                issues.push(issue(
                    PROVIDER,
                    &account.id,
                    None,
                    "Fly.io returned an app with an invalid name; it was skipped.",
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
    match cli
        .run(&["auth", "whoami"], Duration::from_secs(10))
        .await
    {
        Ok(output) if !output.trim().is_empty() && !output.contains('\0') => Connection {
            kind: None,
            connection_id: account.id.clone(),
            provider: PROVIDER,
            status: ConnectionStatus::Connected,
            account: Some(output.trim().to_owned()),
            message: Some(
                "Fly.io release history is shown directly; app or machine running state is not treated as deployment progress."
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

fn release_args(app: &str) -> [&str; 4] {
    ["releases", "--app", app, "--json"]
}

fn app_target(app: App, connection_id: &str) -> Result<Target, CliError> {
    if !valid_slug(&app.name) {
        return Err(CliError::InvalidOutput);
    }
    Ok(Target {
        id: format!("fly:{}", app.name),
        provider: PROVIDER,
        connection_id: connection_id.into(),
        name: app.name.clone(),
        description: app
            .organization
            .and_then(AppOrganization::label)
            .map(|organization| format!("Fly.io organization: {organization}")),
        metadata: HashMap::from([("app".into(), app.name)]),
    })
}

fn map_releases(
    releases: Vec<Release>,
    target: &Target,
    app: &str,
) -> Result<ActivityPage, CliError> {
    let truncated = releases.len() > PAGE_SIZE;
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
            "Fly.io CLI fetches a fixed 25-release window and exposes no continuation cursor; ratelline shows the latest 20.",
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
    if release.id.trim().is_empty() || release.version == 0 || release.description.trim().is_empty()
    {
        return Err(CliError::InvalidOutput);
    }
    let timestamp = parse_timestamp(&release.created_at)?;
    let status = fly_status(&release.status);
    if status == ActivityStatus::Unknown {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            format!(
                "Fly.io release {} has an unrecognized status '{}'.",
                release.version, release.status
            ),
        ));
    }
    let actor = release.user.and_then(|user| {
        user.email
            .filter(|value| !value.trim().is_empty())
            .or_else(|| user.name.filter(|value| !value.trim().is_empty()))
    });
    Ok(Activity {
        id: format!("fly:{}:{}", app, release.id),
        provider: PROVIDER,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: "release".into(),
        title: format!("v{} {}", release.version, release.description),
        project: app.into(),
        branch: None,
        url: provider_url("fly.io", &["apps", app, "monitoring"])?,
        status,
        raw_status: release.status,
        actor,
        ownership: Ownership::Unknown,
        started_at: timestamp.clone(),
        finished_at: terminal_time(status, &timestamp),
        updated_at: timestamp,
    })
}

fn fly_status(status: &str) -> ActivityStatus {
    match status.to_ascii_lowercase().as_str() {
        "pending" | "running" => ActivityStatus::Running,
        "complete" | "completed" | "succeeded" | "success" => ActivityStatus::Success,
        "failed" => ActivityStatus::Failed,
        "cancelled" | "canceled" => ActivityStatus::Canceled,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            id: "fly:example-app".into(),
            provider: PROVIDER,
            connection_id: "fly:local".into(),
            name: "example-app".into(),
            description: None,
            metadata: HashMap::from([("app".into(), "example-app".into())]),
        }
    }

    #[test]
    fn parses_current_go_json_field_names() {
        let fixture = r#"[{
          "ID":"rel_123", "Version":12, "Status":"complete",
          "Description":"Deploy image", "CreatedAt":"2026-09-28T10:00:00Z",
          "User":{"Email":"person@example.invalid"}
        }]"#;
        let releases: Vec<Release> = serde_json::from_str(fixture).expect("fixture");
        let page = map_releases(releases, &target(), "example-app").expect("page");
        assert_eq!(page.activities[0].status, ActivityStatus::Success);
        assert_eq!(page.activities[0].ownership, Ownership::Unknown);
        assert!(page.activities[0].url.starts_with("https://fly.io/"));
    }

    #[test]
    fn parses_current_apps_json_with_nested_organization() {
        let apps: Vec<App> = serde_json::from_str(
            r#"[{"ID":"example-app","Name":"example-app","Organization":{"ID":"org-1","Name":"Example Org","Slug":"example-org"}}]"#,
        )
        .expect("fixture");
        let target =
            app_target(apps.into_iter().next().expect("app"), "fly:local").expect("target");
        assert_eq!(
            target.description.as_deref(),
            Some("Fly.io organization: example-org")
        );
    }

    #[test]
    fn request_is_app_scoped_and_machine_state_is_unused() {
        assert_eq!(
            release_args("example-app"),
            ["releases", "--app", "example-app", "--json"]
        );
        assert_eq!(fly_status("running"), ActivityStatus::Running);
        assert_eq!(fly_status("mystery"), ActivityStatus::Unknown);
    }
}
