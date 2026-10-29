use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    cli::{CliError, CliInvocation},
    models::{Account, ActivityPage, Connection, ConnectionStatus, Discovery, Provider, Target},
};

use url::Url;

use crate::models::{Activity, ActivityStatus, Ownership};

use super::support::{
    capped_targets, discovery_from_connection, failed_connection, issue, metadata, missing_cli,
    valid_argument, validate_target,
};

use super::support::{parse_timestamp, terminal_time, PAGE_SIZE};

const PROVIDER: Provider = Provider::Azure;
const LABEL: &str = "Azure";
const MAX_HISTORY_HOURS: i64 = 168;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Subscription {
    id: String,
    name: String,
    state: String,
    #[serde(default)]
    user: Option<AzureUser>,
    #[serde(default)]
    environment_name: Option<String>,
}

#[derive(Deserialize)]
struct AzureUser {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebApp {
    id: String,
    name: String,
    resource_group: String,
    #[serde(default)]
    default_host_name: Option<String>,
}

#[derive(Deserialize)]
struct Deployment {
    id: String,
    status: i32,
    #[serde(default, alias = "statusText")]
    status_text: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default, alias = "authorEmail")]
    author_email: Option<String>,
    #[serde(default)]
    deployer: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(alias = "receivedTime")]
    received_time: String,
    #[serde(default, alias = "startTime")]
    start_time: Option<String>,
    #[serde(default, alias = "endTime")]
    end_time: Option<String>,
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let Some(cli) = cli else {
        return missing_cli(account, PROVIDER, LABEL);
    };
    let connection = connection(Some(cli), account).await;
    if connection.status != ConnectionStatus::Connected {
        return discovery_from_connection(connection);
    }
    let subscriptions: Vec<Subscription> = match cli
        .json(&["account", "list", "--output", "json", "--only-show-errors"])
        .await
    {
        Ok(subscriptions) => subscriptions,
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
    for subscription in subscriptions
        .into_iter()
        .filter(|subscription| subscription.state.eq_ignore_ascii_case("Enabled"))
    {
        if Uuid::parse_str(&subscription.id).is_err() || subscription.name.trim().is_empty() {
            issues.push(issue(
                PROVIDER,
                &account.id,
                None,
                "Azure returned a subscription with invalid metadata; it was skipped.",
            ));
            continue;
        }
        let args = webapp_args(&subscription.id);
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        match cli.json::<Vec<WebApp>>(&refs).await {
            Ok(apps) => {
                for app in apps {
                    match app_target(&subscription, app, &account.id) {
                        Ok(target) => targets.push(target),
                        Err(_) => issues.push(issue(
                            PROVIDER,
                            &account.id,
                            None,
                            format!(
                                "Azure returned an invalid App Service target in subscription {}; it was skipped.",
                                subscription.name
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
                    "Azure App Service discovery failed for subscription {}: {}",
                    subscription.name,
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
        .json::<Subscription>(&["account", "show", "--output", "json", "--only-show-errors"])
        .await
    {
        Ok(subscription)
            if subscription
                .environment_name
                .as_deref()
                .is_some_and(|environment| environment != "AzureCloud") =>
        {
            Connection {
                kind: None,
                connection_id: account.id.clone(),
                provider: PROVIDER,
                status: ConnectionStatus::Error,
                account: subscription.user.map(|user| user.name),
                message: Some(
                    "Azure sovereign clouds are not supported yet because ratelline only validates portal.azure.com links."
                        .into(),
                ),
                cli_version: version,
            }
        }
        Ok(subscription) => match subscription.user {
            Some(user) if !user.name.trim().is_empty() => Connection {
                kind: None,
                connection_id: account.id.clone(),
                provider: PROVIDER,
                status: ConnectionStatus::Connected,
                account: Some(user.name),
                message: Some(
                    "Azure App Service deployment history uses the existing CLI session. This command has no pagination; reads have a timeout and output-size limit."
                        .into(),
                ),
                cli_version: version,
            },
            _ => failed_connection(
                account,
                PROVIDER,
                LABEL,
                version,
                CliError::InvalidOutput,
            ),
        },
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
    let subscription = metadata(target, "subscription")?;
    let resource_group = metadata(target, "resource_group")?;
    let app = metadata(target, "app")?;
    let resource_id = metadata(target, "resource_id")?;
    if Uuid::parse_str(subscription).is_err()
        || !valid_argument(resource_group)
        || !valid_argument(app)
        || !valid_resource_id(resource_id, subscription, resource_group, app)
    {
        return Err(CliError::InvalidOutput);
    }
    let args = deployment_args(subscription, resource_group, app);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let deployments: Vec<Deployment> = cli.json(&refs).await?;
    map_deployments(deployments, target, app, resource_id, Utc::now())
}

fn webapp_args(subscription: &str) -> Vec<String> {
    vec![
        "webapp".into(),
        "list".into(),
        format!("--subscription={subscription}"),
        "--output=json".into(),
        "--only-show-errors".into(),
    ]
}

fn app_target(
    subscription: &Subscription,
    app: WebApp,
    connection_id: &str,
) -> Result<Target, CliError> {
    if !valid_argument(&app.name)
        || !valid_argument(&app.resource_group)
        || !valid_resource_id(&app.id, &subscription.id, &app.resource_group, &app.name)
    {
        return Err(CliError::InvalidOutput);
    }
    Ok(Target {
        id: format!("azure:{}", app.id.to_ascii_lowercase()),
        provider: PROVIDER,
        connection_id: connection_id.into(),
        name: app.name.clone(),
        description: app
            .default_host_name
            .map(|host| format!("{} ({host})", subscription.name)),
        metadata: HashMap::from([
            ("subscription".into(), subscription.id.clone()),
            ("resource_group".into(), app.resource_group),
            ("app".into(), app.name),
            ("resource_id".into(), app.id),
        ]),
    })
}

fn deployment_args(subscription: &str, group: &str, app: &str) -> Vec<String> {
    vec![
        "webapp".into(),
        "log".into(),
        "deployment".into(),
        "list".into(),
        format!("--subscription={subscription}"),
        format!("--resource-group={group}"),
        format!("--name={app}"),
        "--output=json".into(),
        "--only-show-errors".into(),
    ]
}

fn map_deployments(
    deployments: Vec<Deployment>,
    target: &Target,
    app: &str,
    resource_id: &str,
    now: DateTime<Utc>,
) -> Result<ActivityPage, CliError> {
    let mut issues = Vec::new();
    let mut activities = deployments
        .into_iter()
        .map(|deployment| map_deployment(deployment, target, app, resource_id, &mut issues))
        .collect::<Result<Vec<_>, _>>()?;
    activities.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let cutoff = now - Duration::hours(MAX_HISTORY_HOURS);
    let relevant_history_omitted = activities.get(PAGE_SIZE..).is_some_and(|omitted| {
        omitted
            .iter()
            .any(|activity| may_affect_history(activity, &cutoff))
    });
    activities.truncate(PAGE_SIZE);
    if relevant_history_omitted {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            "Azure CLI returned more than 20 Kudu deployments but exposes no continuation flag for this command; older deployments are omitted.",
        ));
    }
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

fn may_affect_history(activity: &Activity, cutoff: &DateTime<Utc>) -> bool {
    let terminal = matches!(
        activity.status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    );
    if !terminal {
        return true;
    }
    !activity
        .finished_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|finished_at| finished_at.timestamp() < cutoff.timestamp())
}

fn map_deployment(
    deployment: Deployment,
    target: &Target,
    app: &str,
    resource_id: &str,
    issues: &mut Vec<crate::models::Issue>,
) -> Result<Activity, CliError> {
    if !valid_argument(&deployment.id) {
        return Err(CliError::InvalidOutput);
    }
    let received_at = parse_timestamp(&deployment.received_time)?;
    let started_at = deployment
        .start_time
        .as_deref()
        .map(parse_timestamp)
        .transpose()?
        .unwrap_or_else(|| received_at.clone());
    let updated_at = deployment
        .end_time
        .as_deref()
        .map(parse_timestamp)
        .transpose()?
        .unwrap_or_else(|| started_at.clone());
    let status = deployment_status(deployment.status);
    if status == ActivityStatus::Unknown {
        issues.push(issue(
            PROVIDER,
            &target.connection_id,
            Some(&target.id),
            format!(
                "Azure deployment {} has an unrecognized Kudu status code {}.",
                deployment.id, deployment.status
            ),
        ));
    }
    let raw_status = deployment
        .status_text
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| deployment.status.to_string());
    let title = deployment
        .message
        .filter(|value| !value.trim().is_empty())
        .or(deployment.deployer.filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| "App Service deployment".into());
    let actor = deployment
        .author
        .filter(|value| valid_actor(value))
        .or_else(|| deployment.author_email.filter(|value| valid_actor(value)));
    Ok(Activity {
        id: format!("azure:{}:{}", app, deployment.id),
        provider: PROVIDER,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: None,
        kind: "app-service-deployment".into(),
        title,
        project: app.into(),
        branch: None,
        url: portal_url(resource_id)?,
        status,
        raw_status,
        actor,
        ownership: Ownership::Unknown,
        started_at,
        finished_at: terminal_time(status, &updated_at),
        updated_at,
    })
}

fn valid_resource_id(resource_id: &str, subscription: &str, group: &str, app: &str) -> bool {
    valid_argument(resource_id)
        && resource_id.eq_ignore_ascii_case(&format!(
            "/subscriptions/{subscription}/resourceGroups/{group}/providers/Microsoft.Web/sites/{app}"
        ))
}

fn portal_url(resource_id: &str) -> Result<String, CliError> {
    if !valid_argument(resource_id) || !resource_id.starts_with('/') {
        return Err(CliError::InvalidOutput);
    }
    let mut url = Url::parse("https://portal.azure.com/").map_err(|_| CliError::InvalidOutput)?;
    url.set_fragment(Some(&format!("resource{resource_id}/deploymentcenter")));
    Ok(url.into())
}

fn valid_actor(value: &str) -> bool {
    !value.trim().is_empty() && !value.eq_ignore_ascii_case("N/A")
}

fn deployment_status(status: i32) -> ActivityStatus {
    match status {
        0 => ActivityStatus::Queued,
        1 | 2 => ActivityStatus::Running,
        3 => ActivityStatus::Failed,
        4 => ActivityStatus::Success,
        _ => ActivityStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        let subscription = "00000000-0000-4000-8000-000000000001";
        let resource_id = format!(
            "/subscriptions/{subscription}/resourceGroups/example-rg/providers/Microsoft.Web/sites/example-app"
        );
        Target {
            id: format!("azure:{}", resource_id.to_ascii_lowercase()),
            provider: PROVIDER,
            connection_id: "azure:local".into(),
            name: "example-app".into(),
            description: None,
            metadata: HashMap::from([
                ("subscription".into(), subscription.into()),
                ("resource_group".into(), "example-rg".into()),
                ("app".into(), "example-app".into()),
                ("resource_id".into(), resource_id),
            ]),
        }
    }

    #[test]
    fn parses_kudu_deployment_fixture_and_status_code() {
        let fixture = r#"[{
          "id":"deployment-123", "status":3, "status_text":"failed",
          "author":"Example User", "deployer":"ZipDeploy", "message":"Deploy archive",
          "received_time":"2026-09-28T10:00:00Z", "start_time":"2026-09-28T10:00:05Z",
          "end_time":"2026-09-28T10:02:00Z"
        }]"#;
        let deployments: Vec<Deployment> = serde_json::from_str(fixture).expect("fixture");
        let resource_id = target().metadata["resource_id"].clone();
        let page = map_deployments(
            deployments,
            &target(),
            "example-app",
            &resource_id,
            DateTime::parse_from_rfc3339("2026-10-06T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .expect("page");
        assert_eq!(page.activities[0].status, ActivityStatus::Failed);
        assert_eq!(page.activities[0].ownership, Ownership::Unknown);
        assert!(page.activities[0]
            .url
            .starts_with("https://portal.azure.com/"));
    }

    #[test]
    fn read_request_is_scoped_and_unknown_status_is_preserved() {
        let args = deployment_args("subscription", "group", "app");
        assert!(args.contains(&"--subscription=subscription".into()));
        assert!(args.contains(&"--resource-group=group".into()));
        assert!(args.contains(&"--name=app".into()));
        assert_eq!(deployment_status(2), ActivityStatus::Running);
        assert_eq!(deployment_status(99), ActivityStatus::Unknown);
    }

    #[test]
    fn truncation_only_reports_history_that_may_still_matter() {
        let now = DateTime::parse_from_rfc3339("2026-10-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let resource_id = target().metadata["resource_id"].clone();
        let old_completed = deployments(
            PAGE_SIZE + 1,
            4,
            "2026-09-28T10:00:00Z",
            "2026-09-28T10:03:00Z",
        );
        let healthy =
            map_deployments(old_completed, &target(), "example-app", &resource_id, now).unwrap();
        assert_eq!(healthy.activities.len(), PAGE_SIZE);
        assert!(healthy.issues.is_empty());

        let recent_completed = deployments(
            PAGE_SIZE + 1,
            4,
            "2026-10-05T10:00:00Z",
            "2026-10-05T10:03:00Z",
        );
        let partial = map_deployments(
            recent_completed,
            &target(),
            "example-app",
            &resource_id,
            now,
        )
        .unwrap();
        assert_eq!(partial.issues.len(), 1);

        let mut old_active =
            deployments(PAGE_SIZE, 4, "2026-10-05T10:00:00Z", "2026-10-05T10:03:00Z");
        old_active.extend(deployments(
            1,
            2,
            "2026-09-20T10:00:00Z",
            "2026-09-20T10:03:00Z",
        ));
        let partial =
            map_deployments(old_active, &target(), "example-app", &resource_id, now).unwrap();
        assert_eq!(partial.issues.len(), 1);
    }

    fn deployments(
        count: usize,
        status: i32,
        started_at: &str,
        finished_at: &str,
    ) -> Vec<Deployment> {
        (0..count)
            .map(|index| Deployment {
                id: format!("deployment-{index}"),
                status,
                status_text: None,
                author: None,
                author_email: None,
                deployer: None,
                message: None,
                received_time: started_at.into(),
                start_time: Some(started_at.into()),
                end_time: Some(finished_at.into()),
            })
            .collect()
    }
}
