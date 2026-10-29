use super::{aws, bitbucket, cloud, github, github_deployments, hosting, mobile, vercel};
use crate::{
    cli::CliInvocation,
    models::{
        Account, ActivityPage, Connection, Discovery, IssueKind, Provider, QueryError, Target,
    },
};

pub fn local_tools() -> &'static [(Provider, &'static [&'static str])] {
    &[
        (Provider::Railway, &["railway"]),
        (Provider::Netlify, &["netlify"]),
        (Provider::Render, &["render"]),
        (Provider::Heroku, &["heroku"]),
        (Provider::Fly, &["fly", "flyctl"]),
        (Provider::Gitlab, &["glab"]),
        (Provider::Digitalocean, &["doctl"]),
        (Provider::Gcloud, &["gcloud"]),
        (Provider::Azure, &["az"]),
        (Provider::Eas, &["eas"]),
        (Provider::Firebase, &["gcloud"]),
        (Provider::Supabase, &["supabase"]),
    ]
}

pub fn supported(provider: Provider) -> bool {
    !matches!(provider, Provider::Fastlane | Provider::Supabase)
}

pub fn limitation(provider: Provider) -> &'static str {
    match provider {
        Provider::Eas => "Register an EAS project with its UUID, owner and slug to monitor builds and published updates.",
        Provider::Firebase => "Connect a Google Cloud CLI account to monitor Firebase Hosting releases and App Hosting rollouts.",
        Provider::CloudflarePages | Provider::CloudflareWorkers => "Connect a scoped Cloudflare API token to monitor this service.",
        Provider::Fastlane => "fastlane has no central deployment history. Monitor the GitHub Actions or GitLab pipeline that runs your lane.",
        Provider::Supabase => "Function ACTIVE status is not a deployment result. A reliable deployment history is not available through the local CLI.",
        _ => "This provider requires its installed CLI and an existing authenticated session.",
    }
}

pub async fn connection(
    cli: Option<&CliInvocation>,
    account: &Account,
    target: Option<&Target>,
) -> Connection {
    provider_connection(cli, account, target).await.normalized()
}

async fn provider_connection(
    cli: Option<&CliInvocation>,
    account: &Account,
    target: Option<&Target>,
) -> Connection {
    match account.provider {
        Provider::Github => github::connection(cli, &account.id).await,
        Provider::GithubDeployments => {
            let result = github::connection(cli, &account.id).await;
            Connection {
                provider: Provider::GithubDeployments,
                ..result
            }
        }
        Provider::Vercel => vercel::connection(cli, &account.id).await,
        Provider::Aws => {
            aws::connection(
                account.profile.clone(),
                target.and_then(|t| t.metadata.get("region")).cloned(),
                &account.id,
            )
            .await
        }
        Provider::Bitbucket => bitbucket::connection(account).await,
        Provider::Railway
        | Provider::Netlify
        | Provider::Render
        | Provider::CloudflarePages
        | Provider::CloudflareWorkers => hosting::connection(cli, account).await,
        Provider::Heroku
        | Provider::Fly
        | Provider::Gitlab
        | Provider::Digitalocean
        | Provider::Gcloud
        | Provider::Azure => cloud::connection(cli, account).await,
        _ => mobile::connection(cli, account).await,
    }
}

pub async fn discover(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    let found = provider_discovery(cli, account).await;
    Discovery {
        connection: found.connection.normalized(),
        ..found
    }
}

async fn provider_discovery(cli: Option<&CliInvocation>, account: &Account) -> Discovery {
    match account.provider {
        Provider::Github => github::discover(cli, &account.id).await,
        Provider::Vercel => vercel::discover(cli, &account.id).await,
        Provider::Aws => aws::discover(account.profile.clone(), None, &account.id)
            .await
            .unwrap_or_else(|message| Discovery {
                connection: Connection::disconnected(
                    &account.id,
                    account.provider,
                    crate::models::ConnectionStatus::Error,
                    message.clone(),
                ),
                targets: Vec::new(),
                aws_profiles: Vec::new(),
                issues: vec![crate::models::Issue {
                    kind: crate::models::IssueKind::Other,
                    provider: account.provider,
                    connection_id: Some(account.id.clone()),
                    target_id: None,
                    message,
                }],
            }),
        Provider::GithubDeployments => github_deployments::discover(cli, account).await,
        Provider::Bitbucket => bitbucket::discover(account).await,
        Provider::Railway
        | Provider::Netlify
        | Provider::Render
        | Provider::CloudflarePages
        | Provider::CloudflareWorkers => hosting::discover(cli, account).await,
        Provider::Heroku
        | Provider::Fly
        | Provider::Gitlab
        | Provider::Digitalocean
        | Provider::Gcloud
        | Provider::Azure => cloud::discover(cli, account).await,
        _ => mobile::discover(cli, account).await,
    }
}

pub fn has_active_scan(provider: Provider) -> bool {
    matches!(
        provider,
        Provider::Github
            | Provider::Vercel
            | Provider::Bitbucket
            | Provider::Eas
            | Provider::CloudflarePages
            | Provider::CloudflareWorkers
            | Provider::Firebase
    )
}

pub async fn active(
    cli: Option<&CliInvocation>,
    target: &Target,
    identity: Option<&str>,
) -> Result<ActivityPage, QueryError> {
    if target.provider == Provider::Bitbucket {
        return Ok(bitbucket::active(target).await?);
    }
    if matches!(
        target.provider,
        Provider::CloudflarePages | Provider::CloudflareWorkers
    ) {
        return Ok(hosting::active_api(target).await?);
    }
    let cli = cli.ok_or_else(|| {
        QueryError::new(
            IssueKind::MissingCli,
            format!(
                "{} CLI is not installed or this account must be reconnected.",
                target.provider.as_str()
            ),
        )
    })?;
    if target.provider == Provider::Firebase {
        return Ok(mobile::firebase::active_api(cli, target).await?);
    }
    match target.provider {
        Provider::Eas => mobile::eas::active(cli, target).await,
        Provider::Github => github::active(cli, target, identity).await,
        Provider::Vercel => vercel::active(cli, target, identity).await,
        _ => return Ok(ActivityPage::default()),
    }
    .map_err(|error| error.query_error(target.provider.as_str()))
}

pub async fn page(
    cli: Option<&CliInvocation>,
    target: &Target,
    cursor: Option<&str>,
    identity: Option<&str>,
    hours: u32,
) -> Result<ActivityPage, QueryError> {
    target.validate()?;
    if cursor.is_some_and(|value| {
        value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control)
    }) {
        return Err("Invalid history cursor.".into());
    }
    if target.provider == Provider::Aws {
        return Ok(aws::page(target, cursor, hours).await?);
    }
    if target.provider == Provider::Bitbucket {
        return Ok(bitbucket::page(target, cursor).await?);
    }
    if matches!(
        target.provider,
        Provider::CloudflarePages | Provider::CloudflareWorkers
    ) {
        return Ok(hosting::page_api(target, cursor).await?);
    }
    if target.provider == Provider::Firebase {
        let cli = cli.ok_or_else(|| {
            QueryError::new(
                IssueKind::MissingCli,
                "Google Cloud CLI is not installed. Sign in with gcloud auth login.",
            )
        })?;
        return Ok(mobile::firebase::page_api(cli, target, cursor).await?);
    }
    let api_result = match target.provider {
        Provider::Github => Some(match cli {
            Some(cli) => github::page(cli, target, cursor, identity).await,
            None => Err(crate::cli::CliError::Unavailable),
        }),
        Provider::Vercel => Some(match cli {
            Some(cli) => vercel::page(cli, target, cursor, identity).await,
            None => Err(crate::cli::CliError::Unavailable),
        }),
        Provider::GithubDeployments => Some(match cli {
            Some(cli) => github_deployments::page(cli, target, cursor, identity).await,
            None => Err(crate::cli::CliError::Unavailable),
        }),
        _ => None,
    };
    if let Some(result) = api_result {
        return result.map_err(|error| error.query_error(target.provider.as_str()));
    }
    let cli = cli.ok_or_else(|| {
        QueryError::new(
            IssueKind::MissingCli,
            format!(
                "{} CLI is not installed. Connect its local CLI session first.",
                target.provider.as_str()
            ),
        )
    })?;
    match target.provider {
        Provider::Railway
        | Provider::Netlify
        | Provider::Render
        | Provider::CloudflarePages
        | Provider::CloudflareWorkers => hosting::page(cli, target, cursor, identity).await,
        Provider::Heroku
        | Provider::Fly
        | Provider::Gitlab
        | Provider::Digitalocean
        | Provider::Gcloud
        | Provider::Azure => cloud::page(cli, target, cursor).await,
        _ => mobile::page(cli, target, cursor).await,
    }
    .map_err(|error| error.query_error(target.provider.as_str()))
}
