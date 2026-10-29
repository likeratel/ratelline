use super::{
    cluster_name, target_id, SharedProfiles, AMPLIFY_BRANCH, AWS_PROVIDER, CLOUD_FORMATION_STACK,
    ECS_SERVICE, MAX_DISCOVERED_TARGETS,
};
use crate::models::{
    ActivityStatus, Connection, ConnectionStatus, Discovery, Issue, Ownership, Target,
};
use aws_config::{profile::ProfileFileCredentialsProvider, BehaviorVersion};
use aws_types::{region::Region, SdkConfig};
use chrono::{DateTime, SecondsFormat, Utc};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
};

pub(super) fn cloudformation_target(
    connection_id: &str,
    profile: &str,
    region: &str,
    stack: &str,
    stack_id: Option<&str>,
) -> Target {
    Target {
        id: target_id(CLOUD_FORMATION_STACK, profile, region, stack),
        provider: AWS_PROVIDER,
        connection_id: connection_id.into(),
        name: format!("CloudFormation: {stack}"),
        description: Some(format!("CloudFormation stack in {region}")),
        metadata: [
            ("kind".into(), CLOUD_FORMATION_STACK.into()),
            ("profile".into(), profile.into()),
            ("region".into(), region.into()),
            ("stack".into(), stack.into()),
        ]
        .into_iter()
        .chain(stack_id.map(|id| ("stack_id".into(), id.into())))
        .collect(),
    }
}

pub(super) fn ecs_target(
    connection_id: &str,
    profile: &str,
    region: &str,
    cluster: &str,
    service: &str,
    service_name: &str,
) -> Target {
    Target {
        id: target_id(ECS_SERVICE, profile, region, service),
        provider: AWS_PROVIDER,
        connection_id: connection_id.into(),
        name: format!("ECS: {}/{}", cluster_name(cluster), service_name),
        description: Some(format!("ECS service in {region}")),
        metadata: HashMap::from([
            ("kind".into(), ECS_SERVICE.into()),
            ("profile".into(), profile.into()),
            ("region".into(), region.into()),
            ("cluster".into(), cluster.into()),
            ("service".into(), service.into()),
        ]),
    }
}

pub(super) fn amplify_target(
    connection_id: &str,
    profile: &str,
    region: &str,
    app_id: &str,
    app_name: &str,
    branch: &str,
) -> Target {
    Target {
        id: target_id(
            AMPLIFY_BRANCH,
            profile,
            region,
            &format!("{app_id}:{branch}"),
        ),
        provider: AWS_PROVIDER,
        connection_id: connection_id.into(),
        name: format!("Amplify: {app_name}/{branch}"),
        description: Some(format!("Amplify Hosting branch in {region}")),
        metadata: HashMap::from([
            ("kind".into(), AMPLIFY_BRANCH.into()),
            ("profile".into(), profile.into()),
            ("region".into(), region.into()),
            ("app_id".into(), app_id.into()),
            ("app_name".into(), app_name.into()),
            ("branch".into(), branch.into()),
        ]),
    }
}

pub(super) fn push_target(targets: &mut Vec<Target>, target: Target, issues: &mut Vec<Issue>) {
    if targets.len() < MAX_DISCOVERED_TARGETS {
        targets.push(target);
    } else if !issues
        .iter()
        .any(|issue| issue.message.contains("target limit"))
    {
        issues.push(partial_issue(
            &target.connection_id,
            "AWS discovery reached its target limit; more resources may be available.",
        ));
    }
}

#[derive(Clone, Copy)]
pub(super) struct AwsTargetMetadata<'a> {
    pub(super) kind: &'a str,
    pub(super) profile: &'a str,
    pub(super) region: &'a str,
    pub(super) stack: Option<&'a str>,
    pub(super) stack_id: Option<&'a str>,
    pub(super) cluster: Option<&'a str>,
    pub(super) service: Option<&'a str>,
    pub(super) app_id: Option<&'a str>,
    pub(super) app_name: Option<&'a str>,
    pub(super) branch: Option<&'a str>,
}

pub(super) fn target_metadata(target: &Target) -> Result<AwsTargetMetadata<'_>, String> {
    let kind = metadata_value(target, "kind")?;
    let profile = metadata_value(target, "profile")?;
    let region = metadata_value(target, "region")?;
    if !is_safe_region(region) {
        return Err("AWS target region is invalid.".into());
    }
    let stack = optional_metadata_value(target, "stack")?;
    let stack_id = optional_metadata_value(target, "stack_id")?;
    let cluster = optional_metadata_value(target, "cluster")?;
    let service = optional_metadata_value(target, "service")?;
    let app_id = optional_metadata_value(target, "app_id")?;
    let app_name = optional_metadata_value(target, "app_name")?;
    let branch = optional_metadata_value(target, "branch")?;
    match kind {
        CLOUD_FORMATION_STACK if stack.is_none() => {
            return Err("CloudFormation target is missing valid stack metadata.".into())
        }
        ECS_SERVICE if cluster.is_none() || service.is_none() => {
            return Err("ECS target is missing valid cluster or service metadata.".into())
        }
        AMPLIFY_BRANCH if app_id.is_none() || app_name.is_none() || branch.is_none() => {
            return Err("Amplify target is missing valid app or branch metadata.".into())
        }
        _ => {}
    }
    Ok(AwsTargetMetadata {
        kind,
        profile,
        region,
        stack,
        stack_id,
        cluster,
        service,
        app_id,
        app_name,
        branch,
    })
}

fn optional_metadata_value<'a>(target: &'a Target, key: &str) -> Result<Option<&'a str>, String> {
    match target.metadata.get(key) {
        None => Ok(None),
        Some(value) if !value.trim().is_empty() && !value.contains('\0') && value.len() <= 512 => {
            Ok(Some(value.as_str()))
        }
        Some(_) => Err(format!("AWS target has invalid {key} metadata.")),
    }
}

pub(super) fn metadata_value<'a>(target: &'a Target, key: &str) -> Result<&'a str, String> {
    target
        .metadata
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty() && !value.contains('\0'))
        .ok_or_else(|| format!("AWS target is missing valid {key} metadata."))
}

pub(super) async fn load_config(profile: &str, region: &str) -> SdkConfig {
    let credentials = ProfileFileCredentialsProvider::builder()
        .profile_name(profile)
        .build();
    aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(region.to_owned()))
        .credentials_provider(credentials)
        .load()
        .await
}

pub(super) fn read_shared_profiles() -> Result<SharedProfiles, String> {
    let (config_path, credentials_path) = shared_profile_paths()
        .ok_or_else(|| "AWS shared configuration location could not be determined.".to_string())?;
    let mut profiles = SharedProfiles::default();

    read_profile_file(&config_path, true, &mut profiles)?;
    read_profile_file(&credentials_path, false, &mut profiles)?;
    Ok(profiles)
}

pub(super) fn shared_profile_paths() -> Option<(PathBuf, PathBuf)> {
    let aws_directory = env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".aws"))
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".aws")));
    let config_path = env::var_os("AWS_CONFIG_FILE")
        .map(PathBuf::from)
        .or_else(|| {
            aws_directory
                .as_ref()
                .map(|directory| directory.join("config"))
        });
    let credentials_path = env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .map(PathBuf::from)
        .or_else(|| aws_directory.map(|directory| directory.join("credentials")));
    config_path.zip(credentials_path)
}

pub(super) fn read_profile_file(
    path: &PathBuf,
    is_config: bool,
    profiles: &mut SharedProfiles,
) -> Result<(), String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return Err(
                "AWS shared configuration could not be read. Check file permissions.".into(),
            )
        }
    };
    let mut current_profile = None;
    for line in contents.lines() {
        if let Some(name) = profile_section(line, is_config) {
            profiles.names.insert(name.clone());
            current_profile = Some(name);
            continue;
        }
        if is_config && line.trim_start().starts_with("region") {
            let Some(profile) = current_profile.as_ref() else {
                continue;
            };
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "region" && is_safe_region(value.trim()) {
                profiles
                    .regions
                    .insert(profile.clone(), value.trim().to_owned());
            }
        }
    }
    Ok(())
}

pub(super) fn profile_section(line: &str, is_config: bool) -> Option<String> {
    let section = line.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    if section == "default" {
        return Some("default".into());
    }
    if is_config {
        return section
            .strip_prefix("profile ")
            .map(str::trim)
            .filter(|name| is_safe_profile(name))
            .map(str::to_owned);
    }
    is_safe_profile(section).then(|| section.to_owned())
}

pub(super) fn selected_profile(profile: Option<String>) -> Result<Option<String>, String> {
    profile
        .map(|value| {
            is_safe_profile(&value)
                .then_some(value)
                .ok_or_else(|| "AWS profile is invalid.".into())
        })
        .transpose()
}

pub(super) fn selected_region(
    region: Option<String>,
    configured: Option<&String>,
) -> Result<String, String> {
    let selected = region.or_else(|| configured.cloned());
    selected
        .filter(|value| is_safe_region(value))
        .ok_or_else(|| {
            "Select an AWS region or configure a region for the selected profile.".into()
        })
}

pub(super) fn is_safe_profile(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.contains('\0')
}

pub(super) fn is_safe_region(value: &str) -> bool {
    (3..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn disconnected_discovery(
    connection_id: &str,
    aws_profiles: Vec<String>,
    status: ConnectionStatus,
    message: impl Into<String>,
) -> Discovery {
    let message = message.into();
    Discovery {
        connection: Connection::disconnected(connection_id, AWS_PROVIDER, status, message.clone()),
        targets: Vec::new(),
        aws_profiles,
        issues: vec![partial_issue(connection_id, message)],
    }
}

pub(super) fn partial_issue(connection_id: &str, message: impl Into<String>) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider: AWS_PROVIDER,
        connection_id: Some(connection_id.into()),
        message: message.into(),
        target_id: None,
    }
}

pub(super) fn target_issue(target: &Target, message: impl Into<String>) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider: AWS_PROVIDER,
        connection_id: Some(target.connection_id.clone()),
        message: message.into(),
        target_id: Some(target.id.clone()),
    }
}

pub(super) fn friendly_aws_error(operation: &str, error: &str) -> String {
    let normalized = error.to_ascii_lowercase();
    if ["expired", "token", "credential", "sso", "not loaded"]
        .iter()
        .any(|term| normalized.contains(term))
    {
        return format!("AWS {operation} could not authenticate the selected profile. Sign in again or refresh its credentials.");
    }
    if normalized.contains("accessdenied") || normalized.contains("access denied") {
        return format!("AWS {operation} was denied. Verify read access for the selected profile.");
    }
    format!("AWS {operation} failed. Check the selected profile, region, network connection, and read permissions.")
}

pub(super) fn connection_status_for_error(error: &str) -> ConnectionStatus {
    let normalized = error.to_ascii_lowercase();
    if ["expired", "token", "credential", "sso", "not loaded"]
        .iter()
        .any(|term| normalized.contains(term))
    {
        ConnectionStatus::SignedOut
    } else {
        ConnectionStatus::Error
    }
}
pub(super) fn normalize_cloudformation_status(raw_status: &str) -> ActivityStatus {
    if raw_status.ends_with("_IN_PROGRESS") {
        ActivityStatus::Running
    } else if raw_status.ends_with("_FAILED") || raw_status.contains("ROLLBACK") {
        ActivityStatus::Failed
    } else if raw_status.ends_with("_COMPLETE") {
        ActivityStatus::Success
    } else if raw_status.ends_with("_SKIPPED") {
        ActivityStatus::Skipped
    } else {
        ActivityStatus::Unknown
    }
}

pub(super) fn normalize_ecs_status(raw_status: &str) -> ActivityStatus {
    match raw_status {
        "IN_PROGRESS" => ActivityStatus::Running,
        "COMPLETED" => ActivityStatus::Success,
        "FAILED" => ActivityStatus::Failed,
        _ => ActivityStatus::Unknown,
    }
}

pub(super) fn normalize_amplify_status(raw_status: &str) -> ActivityStatus {
    match raw_status {
        "CREATED" | "PENDING" => ActivityStatus::Queued,
        "PROVISIONING" | "RUNNING" | "CANCELLING" => ActivityStatus::Running,
        "SUCCEED" => ActivityStatus::Success,
        "FAILED" => ActivityStatus::Failed,
        "CANCELLED" => ActivityStatus::Canceled,
        _ => ActivityStatus::Unknown,
    }
}

pub(super) fn record_latest_stack_operation(
    seen_operations: &mut HashSet<String>,
    operation_id: &str,
) -> bool {
    seen_operations.insert(operation_id.to_owned())
}

pub(super) fn ownership_from_observed_actor(_actor: Option<&str>) -> Ownership {
    Ownership::Unknown
}

pub(super) fn terminal_time(status: ActivityStatus, time: &str) -> Option<String> {
    matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    )
    .then(|| time.to_owned())
}

pub(super) fn rfc3339(milliseconds: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(milliseconds)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub(super) fn should_collect_ecs_deployment(
    status: ActivityStatus,
    updated_at: &str,
    cutoff: DateTime<Utc>,
) -> bool {
    status == ActivityStatus::Running
        || DateTime::parse_from_rfc3339(updated_at)
            .map(|value| value.with_timezone(&Utc) >= cutoff)
            .unwrap_or(false)
}

pub(super) fn is_selected_stack_event(
    selected_stack: &str,
    selected_stack_id: Option<&str>,
    event_stack_name: Option<&str>,
    logical_resource_id: Option<&str>,
    physical_resource_id: Option<&str>,
) -> bool {
    match selected_stack_id {
        Some(stack_id) => physical_resource_id == Some(stack_id),
        None => {
            physical_resource_id == Some(selected_stack)
                || logical_resource_id == Some(selected_stack)
                || (physical_resource_id.is_none() && event_stack_name == Some(selected_stack))
        }
    }
}

pub(super) fn cloudformation_operation_id(
    stack: &str,
    operation_id: Option<&str>,
    client_request_token: Option<&str>,
) -> String {
    operation_id
        .or(client_request_token)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("root-state:{stack}"))
}

pub(super) fn should_collect_cloudformation_operation(
    status: ActivityStatus,
    timestamp: &str,
    cutoff: DateTime<Utc>,
) -> bool {
    !matches!(
        status,
        ActivityStatus::Success
            | ActivityStatus::Failed
            | ActivityStatus::Canceled
            | ActivityStatus::Skipped
    ) || is_within_history(timestamp, cutoff)
}

pub(super) fn is_within_history(timestamp: &str, cutoff: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|value| value.with_timezone(&Utc) >= cutoff)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_old_running_rollouts_but_excludes_old_terminal_deployments() {
        let cutoff = DateTime::parse_from_rfc3339("2026-09-29T00:00:00Z")
            .expect("cutoff")
            .with_timezone(&Utc);
        let old = "2026-09-01T00:00:00.000Z";
        assert!(should_collect_ecs_deployment(
            ActivityStatus::Running,
            old,
            cutoff
        ));
        assert!(!should_collect_ecs_deployment(
            ActivityStatus::Success,
            old,
            cutoff
        ));
    }

    #[test]
    fn maps_only_the_selected_root_stack_and_uses_a_stable_operation_key() {
        let root = aws_sdk_cloudformation::types::StackEvent::builder()
            .stack_name("parent")
            .logical_resource_id("parent")
            .physical_resource_id("arn:aws:cloudformation:us-east-1:123:stack/parent/id")
            .resource_type("AWS::CloudFormation::Stack")
            .build();
        let nested = aws_sdk_cloudformation::types::StackEvent::builder()
            .stack_name("parent")
            .logical_resource_id("NestedStack")
            .physical_resource_id("arn:aws:cloudformation:us-east-1:123:stack/nested/id")
            .resource_type("AWS::CloudFormation::Stack")
            .build();
        let selected_id = "arn:aws:cloudformation:us-east-1:123:stack/parent/id";

        assert!(is_selected_stack_event(
            "parent",
            Some(selected_id),
            root.stack_name(),
            root.logical_resource_id(),
            root.physical_resource_id(),
        ));
        assert!(!is_selected_stack_event(
            "parent",
            Some(selected_id),
            nested.stack_name(),
            nested.logical_resource_id(),
            nested.physical_resource_id(),
        ));
        assert_eq!(
            cloudformation_operation_id("parent", Some("operation-42"), None),
            "operation-42"
        );
        assert_eq!(
            cloudformation_operation_id("parent", None, None),
            "root-state:parent"
        );
    }
}
