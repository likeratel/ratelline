//! Checks what one gh account can read, using that account's own token.

use futures::{stream, StreamExt};
use serde::de::IgnoredAny;

use crate::cli::{CliError, CliInvocation};

const PROBE_CONCURRENCY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetAccess {
    Accessible,
    Inaccessible,
    Unknown(CliError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AccountProbe {
    /// The GitHub user ID and login returned by `GET /user`.
    pub identity: Result<(String, String), CliError>,
    pub targets: Vec<(String, TargetAccess)>,
}

/// Reads `GET /user`, then `GET /repos/{owner}/{repo}` for each repository.
/// Repositories are not checked when the identity cannot be read, nor when it
/// differs from `expected_identity` unless `probe_other_identity` is set. Only
/// a diagnosis the user opened sets it, to show which other user has access.
pub(crate) async fn probe(
    cli: &CliInvocation,
    repositories: &[String],
    expected_identity: Option<&str>,
    probe_other_identity: bool,
) -> AccountProbe {
    let identity = super::github::github_cli_identity(cli).await;
    let other_identity = identity
        .as_ref()
        .is_ok_and(|(id, _)| expected_identity.is_some_and(|expected| expected != id.as_str()));
    if identity.is_err() || other_identity && !probe_other_identity {
        return AccountProbe {
            identity,
            targets: Vec::new(),
        };
    }
    let api = super::api::ApiClient::github(cli);
    let targets = stream::iter(repositories.iter().cloned())
        .map(|repository| {
            let api = api.clone();
            async move {
                let access = repository_access(&api, &repository).await;
                (repository, access)
            }
        })
        .buffered(PROBE_CONCURRENCY)
        .collect()
        .await;
    AccountProbe { identity, targets }
}

async fn repository_access(api: &super::api::ApiClient, repository: &str) -> TargetAccess {
    if super::github::validate_repository_id(repository).is_err() {
        return TargetAccess::Unknown(CliError::InvalidOutput);
    }
    match api
        .github_get::<IgnoredAny>(&format!("repos/{repository}"))
        .await
    {
        Ok(_) => TargetAccess::Accessible,
        Err(CliError::NoAccess) => TargetAccess::Inaccessible,
        Err(error) => TargetAccess::Unknown(error),
    }
}
