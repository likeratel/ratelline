use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use crate::models::{Connection, ConnectionStatus, Discovery, Issue, Provider, Target};

mod amplify;
mod collection;
mod support;

#[cfg(test)]
mod tests;

use support::{
    cloudformation_target, connection_status_for_error, disconnected_discovery, ecs_target,
    friendly_aws_error, load_config, partial_issue, push_target, read_shared_profiles,
    selected_profile, selected_region,
};

pub use collection::{collect, page};

const AWS_PROVIDER: Provider = Provider::Aws;
const CALL_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_DISCOVERY_PAGES: usize = 10;
const MAX_DISCOVERED_TARGETS: usize = 500;
const MAX_COLLECT_TARGETS: usize = 100;
const ECS_DESCRIBE_BATCH_SIZE: usize = 10;

const CLOUD_FORMATION_STACK: &str = "cloudformation_stack";
const ECS_SERVICE: &str = "ecs_service";
const AMPLIFY_BRANCH: &str = "amplify_branch";

#[derive(Default)]
struct SharedProfiles {
    names: BTreeSet<String>,
    regions: BTreeMap<String, String>,
}

pub fn profile_names() -> Result<Vec<String>, String> {
    Ok(read_shared_profiles()?.names.into_iter().collect())
}

/// Checks one explicit profile's identity without listing AWS resources.
pub async fn connection(
    profile: Option<String>,
    region: Option<String>,
    connection_id: &str,
) -> Connection {
    let profiles = match read_shared_profiles() {
        Ok(profiles) => profiles,
        Err(message) => {
            return Connection::disconnected(
                connection_id,
                AWS_PROVIDER,
                ConnectionStatus::Error,
                message,
            )
        }
    };
    let Some(profile) = (match selected_profile(profile) {
        Ok(profile) => profile,
        Err(message) => {
            return Connection::disconnected(
                connection_id,
                AWS_PROVIDER,
                ConnectionStatus::Unconfigured,
                message,
            )
        }
    }) else {
        return Connection::disconnected(
            connection_id,
            AWS_PROVIDER,
            ConnectionStatus::Unconfigured,
            "Select an AWS profile to check its connection.",
        );
    };
    if !profiles.names.contains(&profile) {
        return Connection::disconnected(
            connection_id,
            AWS_PROVIDER,
            ConnectionStatus::Unconfigured,
            "The selected AWS profile was not found in the shared AWS configuration.",
        );
    }
    let region = match selected_region(region, profiles.regions.get(&profile)) {
        Ok(region) => region,
        Err(message) => {
            return Connection::disconnected(
                connection_id,
                AWS_PROVIDER,
                ConnectionStatus::Unconfigured,
                message,
            )
        }
    };
    let config = load_config(&profile, &region).await;
    collection::seed_clients(&profile, &region, &config);
    checked_connection(&config, connection_id).await
}

async fn checked_connection(config: &aws_types::SdkConfig, connection_id: &str) -> Connection {
    let client = aws_sdk_sts::Client::new(config);
    match tokio::time::timeout(CALL_TIMEOUT, client.get_caller_identity().send()).await {
        Ok(Ok(identity)) => {
            let account = identity.account().unwrap_or("Unknown account");
            let arn = identity.arn().unwrap_or("Unknown identity");
            Connection {
                kind: None,
                connection_id: connection_id.into(),
                provider: AWS_PROVIDER,
                status: ConnectionStatus::Connected,
                account: Some(format!("{account} ({arn})")),
                message: None,
                cli_version: None,
            }
        }
        Ok(Err(error)) => Connection::disconnected(
            connection_id,
            AWS_PROVIDER,
            connection_status_for_error(&error.to_string()),
            friendly_aws_error("identity check", &error.to_string()),
        ),
        Err(_) => Connection::disconnected(
            connection_id,
            AWS_PROVIDER,
            ConnectionStatus::Error,
            "AWS identity check timed out. Check the selected profile, network connection, and region.",
        ),
    }
}

/// Discovers read-only CloudFormation, ECS, and Amplify targets for one explicit profile.
pub async fn discover(
    profile: Option<String>,
    region: Option<String>,
    connection_id: &str,
) -> Result<Discovery, String> {
    let profiles = read_shared_profiles()?;
    let aws_profiles = profiles.names.into_iter().collect::<Vec<_>>();
    let Some(selected_profile) = selected_profile(profile)? else {
        return Ok(disconnected_discovery(
            connection_id,
            aws_profiles,
            ConnectionStatus::Unconfigured,
            "Select an AWS profile to discover resources.",
        ));
    };

    if !aws_profiles.iter().any(|name| name == &selected_profile) {
        return Ok(disconnected_discovery(
            connection_id,
            aws_profiles,
            ConnectionStatus::Unconfigured,
            "The selected AWS profile was not found in the shared AWS configuration.",
        ));
    }

    let selected_region = match selected_region(region, profiles.regions.get(&selected_profile)) {
        Ok(value) => value,
        Err(message) => {
            return Ok(disconnected_discovery(
                connection_id,
                aws_profiles,
                ConnectionStatus::Unconfigured,
                message,
            ));
        }
    };

    let config = load_config(&selected_profile, &selected_region).await;
    collection::seed_clients(&selected_profile, &selected_region, &config);
    let connection = checked_connection(&config, connection_id).await;
    if connection.status != ConnectionStatus::Connected {
        let message = connection
            .message
            .clone()
            .unwrap_or_else(|| "AWS identity check failed.".into());
        return Ok(Discovery {
            connection,
            targets: Vec::new(),
            aws_profiles,
            issues: vec![partial_issue(connection_id, message)],
        });
    }

    let mut targets = Vec::new();
    let mut issues = Vec::new();
    let cloudformation = aws_sdk_cloudformation::Client::new(&config);
    discover_stacks(
        &cloudformation,
        connection_id,
        &selected_profile,
        &selected_region,
        &mut targets,
        &mut issues,
    )
    .await;

    if targets.len() < MAX_DISCOVERED_TARGETS {
        let ecs = aws_sdk_ecs::Client::new(&config);
        discover_ecs_services(
            &ecs,
            connection_id,
            &selected_profile,
            &selected_region,
            &mut targets,
            &mut issues,
        )
        .await;
    } else {
        issues.push(partial_issue(
            connection_id,
            "AWS discovery reached its target limit; ECS and Amplify resources were not listed.",
        ));
    }

    if targets.len() < MAX_DISCOVERED_TARGETS {
        let amplify = aws_sdk_amplify::Client::new(&config);
        amplify::discover_apps(
            &amplify,
            connection_id,
            &selected_profile,
            &selected_region,
            &mut targets,
            &mut issues,
        )
        .await;
    } else if !issues
        .iter()
        .any(|issue| issue.message.contains("Amplify resources were not listed"))
    {
        issues.push(partial_issue(
            connection_id,
            "AWS discovery reached its target limit; Amplify resources were not listed.",
        ));
    }

    Ok(Discovery {
        connection,
        targets,
        aws_profiles,
        issues,
    })
}

async fn discover_stacks(
    client: &aws_sdk_cloudformation::Client,
    connection_id: &str,
    profile: &str,
    region: &str,
    targets: &mut Vec<Target>,
    issues: &mut Vec<Issue>,
) {
    let mut next_token = None;

    for page in 0..MAX_DISCOVERY_PAGES {
        let mut request = client.list_stacks();
        if let Some(token) = next_token.take() {
            request = request.next_token(token);
        }

        let output = match tokio::time::timeout(CALL_TIMEOUT, request.send()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                issues.push(partial_issue(
                    connection_id,
                    friendly_aws_error("CloudFormation stack listing", &error.to_string()),
                ));
                return;
            }
            Err(_) => {
                issues.push(partial_issue(
                    connection_id,
                    "CloudFormation stack listing timed out.",
                ));
                return;
            }
        };

        for stack in output.stack_summaries() {
            if stack
                .stack_status()
                .is_some_and(|status| status.as_str() == "DELETE_COMPLETE")
            {
                continue;
            }
            let Some(name) = stack.stack_name() else {
                continue;
            };
            push_target(
                targets,
                cloudformation_target(connection_id, profile, region, name, stack.stack_id()),
                issues,
            );
            if targets.len() >= MAX_DISCOVERED_TARGETS {
                issues.push(partial_issue(
                    connection_id,
                    "AWS discovery reached its target limit; more CloudFormation stacks may be available.",
                ));
                return;
            }
        }

        next_token = output.next_token().map(str::to_owned);
        if next_token.is_none() {
            return;
        }
        if page + 1 == MAX_DISCOVERY_PAGES {
            issues.push(partial_issue(
                connection_id,
                "CloudFormation stack listing reached its page limit; more stacks may be available.",
            ));
        }
    }
}

async fn discover_ecs_services(
    client: &aws_sdk_ecs::Client,
    connection_id: &str,
    profile: &str,
    region: &str,
    targets: &mut Vec<Target>,
    issues: &mut Vec<Issue>,
) {
    let mut cluster_token = None;

    for cluster_page in 0..MAX_DISCOVERY_PAGES {
        let mut request = client.list_clusters();
        if let Some(token) = cluster_token.take() {
            request = request.next_token(token);
        }
        let output = match tokio::time::timeout(CALL_TIMEOUT, request.send()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                issues.push(partial_issue(
                    connection_id,
                    friendly_aws_error("ECS cluster listing", &error.to_string()),
                ));
                return;
            }
            Err(_) => {
                issues.push(partial_issue(
                    connection_id,
                    "ECS cluster listing timed out.",
                ));
                return;
            }
        };

        for cluster in output.cluster_arns() {
            discover_cluster_services(
                client,
                connection_id,
                profile,
                region,
                cluster,
                targets,
                issues,
            )
            .await;
            if targets.len() >= MAX_DISCOVERED_TARGETS {
                issues.push(partial_issue(
                    connection_id,
                    "AWS discovery reached its target limit; more ECS services may be available.",
                ));
                return;
            }
        }

        cluster_token = output.next_token().map(str::to_owned);
        if cluster_token.is_none() {
            return;
        }
        if cluster_page + 1 == MAX_DISCOVERY_PAGES {
            issues.push(partial_issue(
                connection_id,
                "ECS cluster listing reached its page limit; more clusters may be available.",
            ));
        }
    }
}

async fn discover_cluster_services(
    client: &aws_sdk_ecs::Client,
    connection_id: &str,
    profile: &str,
    region: &str,
    cluster: &str,
    targets: &mut Vec<Target>,
    issues: &mut Vec<Issue>,
) {
    let mut next_token = None;

    for page in 0..MAX_DISCOVERY_PAGES {
        let mut request = client.list_services().cluster(cluster);
        if let Some(token) = next_token.take() {
            request = request.next_token(token);
        }
        let output = match tokio::time::timeout(CALL_TIMEOUT, request.send()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                issues.push(partial_issue(
                    connection_id,
                    friendly_aws_error("ECS service listing", &error.to_string()),
                ));
                return;
            }
            Err(_) => {
                issues.push(partial_issue(
                    connection_id,
                    "ECS service listing timed out.",
                ));
                return;
            }
        };

        for service_arns in output.service_arns().chunks(ECS_DESCRIBE_BATCH_SIZE) {
            let request = service_arns.iter().cloned().fold(
                client.describe_services().cluster(cluster),
                |request, service| request.services(service),
            );
            let described = match tokio::time::timeout(CALL_TIMEOUT, request.send()).await {
                Ok(Ok(output)) => output,
                Ok(Err(error)) => {
                    issues.push(partial_issue(
                        connection_id,
                        friendly_aws_error("ECS service description", &error.to_string()),
                    ));
                    continue;
                }
                Err(_) => {
                    issues.push(partial_issue(
                        connection_id,
                        "ECS service description timed out.",
                    ));
                    continue;
                }
            };
            if !described.failures().is_empty() {
                issues.push(partial_issue(
                    connection_id,
                    "ECS did not describe every listed service.",
                ));
            }
            for service in described.services() {
                let Some(name) = service.service_name() else {
                    continue;
                };
                let service_id = service.service_arn().unwrap_or(name);
                push_target(
                    targets,
                    ecs_target(connection_id, profile, region, cluster, service_id, name),
                    issues,
                );
                if targets.len() >= MAX_DISCOVERED_TARGETS {
                    issues.push(partial_issue(
                        connection_id,
                        "AWS discovery reached its target limit; more ECS services may be available.",
                    ));
                    return;
                }
            }
        }

        next_token = output.next_token().map(str::to_owned);
        if next_token.is_none() {
            return;
        }
        if page + 1 == MAX_DISCOVERY_PAGES {
            issues.push(partial_issue(
                connection_id,
                "ECS service listing reached its page limit; more services may be available.",
            ));
        }
    }
}

fn cluster_name(cluster: &str) -> &str {
    cluster.rsplit('/').next().unwrap_or(cluster)
}

fn service_name(service: &str) -> &str {
    service.rsplit('/').next().unwrap_or(service)
}

fn target_id(kind: &str, profile: &str, region: &str, resource: &str) -> String {
    activity_id(&[kind, profile, region, resource])
}

fn activity_id(parts: &[&str]) -> String {
    let hash = parts.iter().fold(0xcbf29ce484222325_u64, |state, part| {
        part.as_bytes()
            .iter()
            .chain(std::iter::once(&0xff))
            .fold(state, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            })
    });
    format!("aws-{hash:016x}")
}
