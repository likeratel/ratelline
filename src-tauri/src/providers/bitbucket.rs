use std::{collections::HashMap, time::Duration};

use futures::StreamExt;
use reqwest::{redirect::Policy, Client, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize};

use crate::{
    credentials::{self, BitbucketCredential},
    models::{
        Account, Activity, ActivityPage, ActivityStatus, Connection, ConnectionStatus, Discovery,
        Issue, Ownership, Provider, Target,
    },
};

#[path = "bitbucket_pagination.rs"]
mod pagination;
use pagination::{parse_cursor, validated_next_page, validated_pipeline_next_page};

const API_ROOT: &str = "https://api.bitbucket.org/2.0/";
const RESPONSE_CAP: usize = 2 * 1024 * 1024;
const RECENT_PAGE_SIZE: u32 = 20;
const ACTIVE_PAGE_SIZE: u32 = 50;
const DISCOVERY_PAGE_SIZE: u32 = 100;
const MAX_WORKSPACE_PAGES: u32 = 10;
const MAX_REPOSITORY_PAGES: u32 = 50;
const MAX_REPOSITORIES: usize = 1_000;

#[derive(Clone, Deserialize)]
pub struct User {
    pub uuid: String,
    pub account_id: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct Page<T> {
    values: Vec<T>,
    next: Option<String>,
}

#[derive(Deserialize)]
struct WorkspaceAccess {
    workspace: Workspace,
}

#[derive(Deserialize)]
struct Workspace {
    slug: String,
}

#[derive(Deserialize)]
struct Repository {
    uuid: String,
    full_name: String,
    name: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct Pipeline {
    uuid: String,
    build_number: u64,
    creator: Option<Actor>,
    state: PipelineState,
    target: Option<PipelineTarget>,
    created_on: String,
    completed_on: Option<String>,
    links: Option<PipelineLinks>,
}

#[derive(Deserialize)]
struct PipelineState {
    name: String,
    result: Option<PipelineResult>,
    stage: Option<PipelineStage>,
}

#[derive(Deserialize)]
struct PipelineStage {
    name: String,
}

#[derive(Deserialize)]
struct Actor {
    uuid: Option<String>,
    account_id: Option<String>,
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct PipelineResult {
    name: String,
}

#[derive(Deserialize)]
struct PipelineTarget {
    ref_name: Option<String>,
    selector: Option<PipelineSelector>,
    commit: Option<PipelineCommit>,
}

#[derive(Deserialize)]
struct PipelineSelector {
    #[serde(rename = "type")]
    kind: Option<String>,
    pattern: Option<String>,
}

#[derive(Deserialize)]
struct PipelineCommit {
    message: Option<String>,
}

#[derive(Deserialize)]
struct PipelineLinks {
    html: Option<Link>,
}

#[derive(Deserialize)]
struct Link {
    href: String,
}

struct Api {
    client: Client,
    credential: BitbucketCredential,
}

impl Api {
    fn new(credential: BitbucketCredential) -> Result<Self, String> {
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "The Bitbucket HTTP client could not be initialized.".to_owned())?;
        Ok(Self { client, credential })
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T, String> {
        validate_api_url(&url)?;
        let response = self
            .client
            .get(url)
            .basic_auth(&self.credential.email, Some(&self.credential.token))
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| "Bitbucket could not be reached.".to_owned())?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_message(status));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| "Bitbucket returned an incomplete response.".to_owned())?;
            if bytes.len().saturating_add(chunk.len()) > RESPONSE_CAP {
                return Err("Bitbucket returned a response larger than 2 MiB.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "Bitbucket returned an invalid response.".to_owned())
    }
}

pub async fn authenticate(email: String, token: String) -> Result<BitbucketCredential, String> {
    validate_email_and_token(&email, &token)?;
    let provisional = BitbucketCredential {
        email,
        token,
        uuid: String::new(),
        account_id: String::new(),
        display_name: String::new(),
    };
    let api = Api::new(provisional)?;
    let user: User = api.get(api_url(&["user"], &[])?).await?;
    validate_user(&user)?;
    Ok(BitbucketCredential {
        uuid: user.uuid,
        account_id: user.account_id,
        display_name: user.display_name,
        ..api.credential
    })
}

pub async fn connection(account: &Account) -> Connection {
    match authenticated_api(&account.id).await {
        Ok((api, _)) => match api.get::<User>(api_url(&["user"], &[]).unwrap()).await {
            Ok(user) if validate_user(&user).is_ok() && same_identity(&user, &api.credential) => {
                Connection {
                    kind: None,
                    connection_id: account.id.clone(),
                    provider: Provider::Bitbucket,
                    status: ConnectionStatus::Connected,
                    account: Some(user.display_name),
                    message: None,
                    cli_version: None,
                }
            }
            Ok(_) => disconnected(
                account,
                "The Bitbucket API token identity no longer matches this connection.",
            ),
            Err(message) => disconnected(account, message),
        },
        Err(message) => disconnected(account, message),
    }
}

pub async fn discover(account: &Account) -> Discovery {
    let (api, credential) = match authenticated_api(&account.id).await {
        Ok(value) => value,
        Err(message) => return failed_discovery(account, message),
    };
    match api.get::<User>(api_url(&["user"], &[]).unwrap()).await {
        Ok(user) if validate_user(&user).is_ok() && same_identity(&user, &credential) => {}
        Ok(_) => {
            return failed_discovery(
                account,
                "The Bitbucket API token identity no longer matches this connection.".into(),
            )
        }
        Err(message) => return failed_discovery(account, message),
    }
    let mut issues = Vec::new();
    let workspaces = match collect_workspaces(&api, &account.id, &mut issues).await {
        Ok(value) => value,
        Err(message) => return failed_discovery(account, message),
    };
    let mut targets = Vec::new();
    let mut pages = 0_u32;
    for workspace in workspaces {
        if targets.len() >= MAX_REPOSITORIES || pages >= MAX_REPOSITORY_PAGES {
            issues.push(issue(&account.id, None, "Bitbucket repository discovery reached its 1,000-repository or 50-page limit. Results are partial."));
            break;
        }
        let result = collect_repositories(&api, &account.id, &workspace.slug, &mut pages).await;
        match result {
            Ok((repositories, truncated)) => {
                for repository in repositories {
                    if targets.len() >= MAX_REPOSITORIES {
                        issues.push(issue(&account.id, None, "Bitbucket repository discovery reached its 1,000-repository limit. Results are partial."));
                        break;
                    }
                    match repository_target(repository, &workspace.slug, &account.id) {
                        Ok(target) => targets.push(target),
                        Err(message) => issues.push(issue(&account.id, None, &message)),
                    }
                }
                if truncated {
                    issues.push(issue(&account.id, None, "Bitbucket repository discovery reached its 50-page limit. Results are partial."));
                    break;
                }
            }
            Err(message) => issues.push(issue(
                &account.id,
                None,
                &format!("Workspace {}: {message}", workspace.slug),
            )),
        }
    }
    Discovery {
        connection: Connection {
            kind: None,
            connection_id: account.id.clone(),
            provider: Provider::Bitbucket,
            status: ConnectionStatus::Connected,
            account: Some(credential.display_name),
            message: None,
            cli_version: None,
        },
        targets,
        aws_profiles: Vec::new(),
        issues,
    }
}

pub async fn page(target: &Target, cursor: Option<&str>) -> Result<ActivityPage, String> {
    let page = parse_cursor(cursor)?;
    pipeline_page(target, page, RECENT_PAGE_SIZE).await
}

pub async fn active(target: &Target) -> Result<ActivityPage, String> {
    validate_target(target)?;
    let (api, credential) = authenticated_api(&target.connection_id).await?;
    let futures = ["BUILDING", "PENDING", "PAUSED", "HALTED"]
        .map(|status| active_status_page(&api, target, status));
    let results = futures::future::join_all(futures).await;
    let mut pipelines = HashMap::new();
    let mut issues = Vec::new();
    let mut successes = 0;
    for (status, result) in ["BUILDING", "PENDING", "PAUSED", "HALTED"]
        .into_iter()
        .zip(results)
    {
        match result {
            Ok((rows, truncated)) => {
                successes += 1;
                for row in rows {
                    pipelines.insert(row.uuid.clone(), row);
                }
                if truncated {
                    issues.push(issue(&target.connection_id, Some(target.id.clone()), &format!("Bitbucket returned more than 50 {status} pipelines. Active results are partial.")));
                }
            }
            Err(message) => issues.push(issue(
                &target.connection_id,
                Some(target.id.clone()),
                &format!("Bitbucket {status} pipeline lookup failed: {message}"),
            )),
        }
    }
    if successes == 0 {
        return Err("All Bitbucket active pipeline lookups failed.".into());
    }
    let activities = pipelines
        .into_values()
        .map(|pipeline| pipeline_activity(pipeline, target, &credential))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ActivityPage {
        activities,
        next_cursor: None,
        issues,
    })
}

async fn pipeline_page(
    target: &Target,
    page_number: u32,
    page_size: u32,
) -> Result<ActivityPage, String> {
    validate_target(target)?;
    let (api, credential) = authenticated_api(&target.connection_id).await?;
    let workspace = target.metadata.get("workspace").unwrap();
    let repository = target.metadata.get("repoSlug").unwrap();
    let page_value = page_number.to_string();
    let size_value = page_size.to_string();
    let query = vec![
        ("page", page_value.as_str()),
        ("pagelen", size_value.as_str()),
        ("sort", "-created_on"),
    ];
    let url = api_url(
        &["repositories", workspace, repository, "pipelines"],
        &query,
    )?;
    let response: Page<Pipeline> = api.get(url).await?;
    let next_cursor = validated_pipeline_next_page(response.next.as_deref(), target)?;
    let activities = response
        .values
        .into_iter()
        .map(|pipeline| pipeline_activity(pipeline, target, &credential))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ActivityPage {
        activities,
        next_cursor,
        issues: Vec::new(),
    })
}

async fn active_status_page(
    api: &Api,
    target: &Target,
    status: &str,
) -> Result<(Vec<Pipeline>, bool), String> {
    let workspace = target.metadata.get("workspace").unwrap();
    let repository = target.metadata.get("repoSlug").unwrap();
    let size = ACTIVE_PAGE_SIZE.to_string();
    let url = api_url(
        &["repositories", workspace, repository, "pipelines"],
        &[
            ("pagelen", &size),
            ("sort", "-created_on"),
            ("status", status),
        ],
    )?;
    let response: Page<Pipeline> = api.get(url).await?;
    let truncated = validated_pipeline_next_page(response.next.as_deref(), target)?.is_some();
    Ok((response.values, truncated))
}

fn pipeline_activity(
    pipeline: Pipeline,
    target: &Target,
    credential: &BitbucketCredential,
) -> Result<Activity, String> {
    let status = pipeline_status(&pipeline.state);
    let actor = pipeline
        .creator
        .as_ref()
        .and_then(|user| user.display_name.clone());
    let ownership = pipeline
        .creator
        .as_ref()
        .map_or(Ownership::Unknown, |creator| {
            let uuid = creator
                .uuid
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let account_id = creator
                .account_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if uuid == Some(credential.uuid.as_str())
                || account_id == Some(credential.account_id.as_str())
            {
                Ownership::Mine
            } else if uuid.is_some() || account_id.is_some() {
                Ownership::Other
            } else {
                Ownership::Unknown
            }
        });
    let url = pipeline_web_url(&pipeline, target)?;
    let title = pipeline
        .target
        .as_ref()
        .and_then(|target| target.commit.as_ref())
        .and_then(|commit| commit.message.as_deref())
        .and_then(|message| message.lines().next())
        .filter(|message| !message.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Pipeline #{}", pipeline.build_number));
    let started_at = parse_timestamp(&pipeline.created_on)?;
    let finished_at = pipeline
        .completed_on
        .as_deref()
        .map(parse_timestamp)
        .transpose()?;
    let updated_at = finished_at.clone().unwrap_or_else(|| started_at.clone());
    Ok(Activity {
        id: pipeline.uuid,
        provider: Provider::Bitbucket,
        connection_id: target.connection_id.clone(),
        target_id: target.id.clone(),
        workflow_name: pipeline
            .target
            .as_ref()
            .and_then(|target| target.selector.as_ref())
            .filter(|selector| selector.kind.as_deref() == Some("custom"))
            .and_then(|selector| super::execution_name(selector.pattern.as_deref())),
        kind: "pipeline".into(),
        title,
        project: target.name.clone(),
        branch: pipeline.target.and_then(|target| target.ref_name),
        url,
        status,
        raw_status: raw_pipeline_status(&pipeline.state),
        actor,
        ownership,
        started_at,
        finished_at,
        updated_at,
    })
}

fn pipeline_status(state: &PipelineState) -> ActivityStatus {
    match (
        state.name.as_str(),
        state.result.as_ref().map(|result| result.name.as_str()),
    ) {
        ("PENDING", _) => ActivityStatus::Queued,
        ("IN_PROGRESS", _)
            if state
                .stage
                .as_ref()
                .is_some_and(|stage| matches!(stage.name.as_str(), "PAUSED" | "HALTED")) =>
        {
            ActivityStatus::Waiting
        }
        ("IN_PROGRESS", _) => ActivityStatus::Running,
        ("COMPLETED", Some("SUCCESSFUL")) => ActivityStatus::Success,
        ("COMPLETED", Some("FAILED" | "ERROR")) => ActivityStatus::Failed,
        ("COMPLETED", Some("STOPPED" | "EXPIRED" | "SUPERSEDED")) => ActivityStatus::Canceled,
        _ => ActivityStatus::Unknown,
    }
}

fn raw_pipeline_status(state: &PipelineState) -> String {
    if let Some(result) = &state.result {
        format!("{}:{}", state.name, result.name)
    } else if let Some(stage) = &state.stage {
        format!("{}:{}", state.name, stage.name)
    } else {
        state.name.clone()
    }
}

async fn authenticated_api(connection_id: &str) -> Result<(Api, BitbucketCredential), String> {
    let credential = credentials::get_bitbucket(connection_id).await?;
    let api = Api::new(credential.clone())?;
    Ok((api, credential))
}

async fn collect_workspaces(
    api: &Api,
    connection_id: &str,
    issues: &mut Vec<Issue>,
) -> Result<Vec<Workspace>, String> {
    let mut result = Vec::new();
    for page_number in 1..=MAX_WORKSPACE_PAGES {
        let page = page_number.to_string();
        let size = DISCOVERY_PAGE_SIZE.to_string();
        let url = api_url(
            &["user", "workspaces"],
            &[("page", &page), ("pagelen", &size)],
        )?;
        let response: Page<WorkspaceAccess> = api.get(url).await?;
        result.extend(response.values.into_iter().map(|access| access.workspace));
        let next = validated_next_page(response.next.as_deref(), &["user", "workspaces"])?;
        if next.is_none() {
            return Ok(result);
        }
    }
    issues.push(issue(
        connection_id,
        None,
        "Bitbucket workspace discovery reached its 10-page limit. Results are partial.",
    ));
    Ok(result)
}

async fn collect_repositories(
    api: &Api,
    _connection_id: &str,
    workspace: &str,
    page_count: &mut u32,
) -> Result<(Vec<Repository>, bool), String> {
    validate_segment(workspace)?;
    let mut result = Vec::new();
    let mut page_number = 1_u32;
    loop {
        if *page_count >= MAX_REPOSITORY_PAGES {
            return Ok((result, true));
        }
        *page_count += 1;
        let page = page_number.to_string();
        let size = DISCOVERY_PAGE_SIZE.to_string();
        let url = api_url(
            &["repositories", workspace],
            &[("page", &page), ("pagelen", &size), ("role", "member")],
        )?;
        let response: Page<Repository> = api.get(url).await?;
        result.extend(response.values);
        match validated_next_page(response.next.as_deref(), &["repositories", workspace])? {
            Some(next) => {
                page_number = next
                    .parse()
                    .map_err(|_| "Bitbucket returned an invalid numeric page.".to_owned())?
            }
            None => return Ok((result, false)),
        }
    }
}

fn repository_target(
    repository: Repository,
    workspace: &str,
    connection_id: &str,
) -> Result<Target, String> {
    validate_segment(workspace)?;
    let (_, slug) = repository
        .full_name
        .split_once('/')
        .ok_or_else(|| "Bitbucket returned a repository without a valid full name.".to_owned())?;
    validate_segment(slug)?;
    let metadata = HashMap::from([
        ("workspace".into(), workspace.into()),
        ("repoSlug".into(), slug.into()),
        ("uuid".into(), repository.uuid),
    ]);
    Ok(Target {
        id: repository.full_name,
        provider: Provider::Bitbucket,
        connection_id: connection_id.into(),
        name: repository.name,
        description: repository.description,
        metadata,
    })
}

fn validate_target(target: &Target) -> Result<(), String> {
    target.validate()?;
    if target.provider != Provider::Bitbucket || !target.connection_id.starts_with("bitbucket:") {
        return Err("The selected target is not a Bitbucket repository.".into());
    }
    validate_segment(
        target
            .metadata
            .get("workspace")
            .ok_or_else(|| "The Bitbucket target has no workspace.".to_owned())?,
    )?;
    validate_segment(
        target
            .metadata
            .get("repoSlug")
            .ok_or_else(|| "The Bitbucket target has no repository slug.".to_owned())?,
    )
}

fn api_url(path: &[&str], query: &[(&str, &str)]) -> Result<Url, String> {
    let mut url =
        Url::parse(API_ROOT).map_err(|_| "The Bitbucket API URL is invalid.".to_owned())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "The Bitbucket API URL is invalid.".to_owned())?;
        segments.pop_if_empty();
        for segment in path {
            validate_segment(segment)?;
            segments.push(segment);
        }
    }
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    Ok(url)
}

fn validate_api_url(url: &Url) -> Result<(), String> {
    if url.scheme() != "https"
        || url.host_str() != Some("api.bitbucket.org")
        || url.port().is_some()
        || url.username() != ""
        || url.password().is_some()
        || !url.path().starts_with("/2.0/")
    {
        return Err("Bitbucket returned an unsafe API pagination URL.".into());
    }
    Ok(())
}

fn validate_segment(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 512
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_control() || character == '/')
    {
        Err("Bitbucket returned an invalid resource identifier.".into())
    } else {
        Ok(())
    }
}

fn validate_email_and_token(email: &str, token: &str) -> Result<(), String> {
    if email.trim() != email
        || email.len() > 320
        || !email.contains('@')
        || email.chars().any(char::is_control)
    {
        return Err("Enter a valid Atlassian account email address.".into());
    }
    if token.trim() != token
        || token.is_empty()
        || token.len() > 4_096
        || token.chars().any(char::is_control)
    {
        return Err("Enter a valid Bitbucket API token.".into());
    }
    Ok(())
}

fn validate_user(user: &User) -> Result<(), String> {
    if user.uuid.is_empty()
        || user.account_id.is_empty()
        || user.display_name.trim().is_empty()
        || user.display_name.len() > 512
    {
        Err("Bitbucket returned an invalid user identity.".into())
    } else {
        Ok(())
    }
}

fn same_identity(user: &User, credential: &BitbucketCredential) -> bool {
    user.uuid == credential.uuid && user.account_id == credential.account_id
}

fn pipeline_web_url(pipeline: &Pipeline, target: &Target) -> Result<String, String> {
    let html = pipeline
        .links
        .as_ref()
        .and_then(|links| links.html.as_ref());
    if let Some(link) = html {
        return validate_pipeline_url(&link.href);
    }
    validate_target(target)?;
    let mut url = Url::parse("https://bitbucket.org")
        .map_err(|_| "The Bitbucket website URL is invalid.".to_owned())?;
    url.path_segments_mut()
        .map_err(|_| "The Bitbucket website URL is invalid.".to_owned())?
        .extend([
            target.metadata["workspace"].as_str(),
            target.metadata["repoSlug"].as_str(),
            "pipelines",
            "results",
            &pipeline.build_number.to_string(),
        ]);
    validate_pipeline_url(url.as_str())
}

fn validate_pipeline_url(value: &str) -> Result<String, String> {
    let url =
        Url::parse(value).map_err(|_| "Bitbucket returned an invalid pipeline URL.".to_owned())?;
    if url.scheme() != "https"
        || url.host_str() != Some("bitbucket.org")
        || url.port().is_some()
        || url.username() != ""
        || url.password().is_some()
    {
        return Err("Bitbucket returned an unsafe pipeline URL.".into());
    }
    Ok(url.into())
}

fn parse_timestamp(value: &str) -> Result<String, String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.to_rfc3339())
        .map_err(|_| "Bitbucket returned an invalid pipeline timestamp.".to_owned())
}

fn status_message(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => "Bitbucket rejected the email or API token.".into(),
        StatusCode::FORBIDDEN => {
            "Bitbucket denied this request. Check the API token scopes.".into()
        }
        StatusCode::TOO_MANY_REQUESTS => {
            "Bitbucket rate-limited this request. Try again later.".into()
        }
        _ => format!("Bitbucket returned HTTP {}.", status.as_u16()),
    }
}

fn disconnected(account: &Account, message: impl Into<String>) -> Connection {
    Connection::disconnected(
        &account.id,
        Provider::Bitbucket,
        ConnectionStatus::Error,
        message,
    )
}

fn issue(connection_id: &str, target_id: Option<String>, message: &str) -> Issue {
    Issue {
        kind: crate::models::IssueKind::Other,
        provider: Provider::Bitbucket,
        connection_id: Some(connection_id.into()),
        target_id,
        message: message.into(),
    }
}

fn failed_discovery(account: &Account, message: String) -> Discovery {
    Discovery {
        connection: disconnected(account, message.clone()),
        targets: Vec::new(),
        aws_profiles: Vec::new(),
        issues: vec![issue(&account.id, None, &message)],
    }
}

#[cfg(test)]
#[path = "bitbucket_tests.rs"]
mod tests;
