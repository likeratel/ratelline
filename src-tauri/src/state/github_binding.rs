//! Keeps every GitHub connection bound to one gh account. A connection without
//! a selected account is diagnosed against each signed-in account and bound
//! automatically only when exactly one account can read all of its repositories.

use std::time::Instant;

use serde::Serialize;

use super::*;
use crate::{
    cli::CliError,
    github_cli_accounts::{GithubCliAccount, GithubCliAccountState},
    models::IssueKind,
    providers::github_access::{self, TargetAccess},
};

/// A determinate result is not re-probed sooner than this unless the user asks.
const DETERMINATE_RETRY: Duration = Duration::from_secs(15 * 60);
/// Retry delays after inconclusive or failed attempts: 1, 2, 4, then 15 minutes.
const FAILURE_BACKOFF: [Duration; 4] = [
    Duration::from_secs(60),
    Duration::from_secs(2 * 60),
    Duration::from_secs(4 * 60),
    DETERMINATE_RETRY,
];
/// A user-requested refresh retries early, but never within this window of the
/// previous attempt, so repeated refreshes cannot defeat the backoff.
const MANUAL_RETRY_FLOOR: Duration = Duration::from_secs(30);
/// Attempt key for listing gh accounts; never a valid connection ID.
const ACCOUNT_LIST_ATTEMPT: &str = "";
const MAX_CONNECTION_ID_LEN: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAccountAccess {
    pub login: String,
    pub active: bool,
    pub state: GithubCliAccountState,
    /// True only when the account is signed in, matches the connection's saved
    /// identity and could read every selected repository.
    pub can_access_all: bool,
    pub inaccessible_targets: Vec<String>,
    /// Set when a check could not finish, so the result is not conclusive.
    pub check_failed: Option<IssueKind>,
    pub identity_mismatch: bool,
    /// Another connection of the same service already uses this account.
    pub used_by_connection_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PinDecision {
    Pin(String),
    NoCandidate,
    Ambiguous,
    InUse,
    Undetermined,
}

#[derive(Clone, Debug)]
pub(super) struct PinAttempt {
    fingerprint: String,
    at: Instant,
    retry_after: Duration,
    failures: usize,
}

impl PinAttempt {
    fn blocks(&self, fingerprint: &str) -> bool {
        self.fingerprint == fingerprint && self.at.elapsed() < self.retry_after
    }
}

fn failed_attempt(
    previous: Option<&PinAttempt>,
    fingerprint: String,
    rate_limited: bool,
) -> PinAttempt {
    let failures = previous
        .filter(|attempt| attempt.fingerprint == fingerprint)
        .map_or(0, |attempt| attempt.failures)
        + 1;
    let backoff = FAILURE_BACKOFF[(failures - 1).min(FAILURE_BACKOFF.len() - 1)];
    PinAttempt {
        fingerprint,
        at: Instant::now(),
        retry_after: if rate_limited {
            backoff.max(DETERMINATE_RETRY)
        } else {
            backoff
        },
        failures,
    }
}

/// Exactly one conclusive, unused account that can read everything is pinned.
/// Any inconclusive check blocks pinning because it could hide a second match.
pub(crate) fn pin_decision(results: &[GithubAccountAccess]) -> PinDecision {
    if results.iter().any(|result| result.check_failed.is_some()) {
        return PinDecision::Undetermined;
    }
    let accessible: Vec<_> = results.iter().filter(|item| item.can_access_all).collect();
    match accessible.as_slice() {
        [] => PinDecision::NoCandidate,
        [only] if only.used_by_connection_id.is_some() => PinDecision::InUse,
        [only] => PinDecision::Pin(only.login.clone()),
        _ => PinDecision::Ambiguous,
    }
}

fn validate_connection_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_CONNECTION_ID_LEN
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, ':' | '-'))
    {
        return Err("The connection ID is invalid.".into());
    }
    Ok(())
}

fn is_github_provider(provider: Provider) -> bool {
    matches!(provider, Provider::Github | Provider::GithubDeployments)
}

fn repositories_for(settings: &Settings, account: &Account) -> Vec<String> {
    let mut repositories: Vec<String> = settings
        .targets
        .iter()
        .filter(|target| target.connection_id == account.id && target.provider == account.provider)
        .map(|target| target.id.clone())
        .collect();
    repositories.sort();
    repositories.dedup();
    repositories
}

fn fingerprint(account: &Account, repositories: &[String]) -> String {
    format!(
        "{}|{}",
        account.profile.as_deref().unwrap_or_default(),
        repositories.join(",")
    )
}

fn used_by(
    accounts: &[Account],
    account: &Account,
    login: &str,
    identity: Option<&str>,
) -> Option<String> {
    accounts
        .iter()
        .find(|other| {
            other.id != account.id
                && other.provider == account.provider
                && (other
                    .cli_username
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(login))
                    || identity.is_some_and(|id| other.profile.as_deref() == Some(id)))
        })
        .map(|other| other.id.clone())
}

fn summarize(
    account: &Account,
    accounts: &[Account],
    listed: &GithubCliAccount,
    probe: Option<github_access::AccountProbe>,
) -> GithubAccountAccess {
    let unchecked = GithubAccountAccess {
        login: listed.login.clone(),
        active: listed.active,
        state: listed.state,
        can_access_all: false,
        inaccessible_targets: Vec::new(),
        check_failed: None,
        identity_mismatch: false,
        used_by_connection_id: used_by(accounts, account, &listed.login, None),
    };
    let Some(probe) = probe else {
        return GithubAccountAccess {
            check_failed: (listed.state == GithubCliAccountState::Error)
                .then_some(IssueKind::TimedOut),
            ..unchecked
        };
    };
    let (identity, login) = match probe.identity {
        Ok(identity) => identity,
        Err(CliError::SignedOut) => {
            return GithubAccountAccess {
                state: GithubCliAccountState::SignedOut,
                ..unchecked
            }
        }
        Err(error) => {
            return GithubAccountAccess {
                state: GithubCliAccountState::Error,
                check_failed: Some(error.issue_kind()),
                ..unchecked
            }
        }
    };
    let identity_mismatch = !login.eq_ignore_ascii_case(&listed.login)
        || account
            .profile
            .as_deref()
            .is_some_and(|expected| expected != identity);
    let inaccessible_targets = probe
        .targets
        .iter()
        .filter(|(_, access)| *access == TargetAccess::Inaccessible)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let check_failed = probe.targets.iter().find_map(|(_, access)| match access {
        TargetAccess::Unknown(error) => Some(error.issue_kind()),
        _ => None,
    });
    GithubAccountAccess {
        can_access_all: !identity_mismatch
            && inaccessible_targets.is_empty()
            && check_failed.is_none(),
        inaccessible_targets,
        check_failed,
        identity_mismatch,
        used_by_connection_id: used_by(accounts, account, &listed.login, Some(&identity)),
        ..unchecked
    }
}

impl AppState {
    pub async fn github_account_states(&self) -> Result<Vec<GithubCliAccount>, String> {
        self.tools
            .github_account_states()
            .await
            .map_err(|error| error.user_message("GitHub"))
    }

    pub async fn diagnose_github_connection(
        &self,
        connection_id: &str,
    ) -> Result<Vec<GithubAccountAccess>, String> {
        validate_connection_id(connection_id)?;
        let settings = self.snapshot().await.settings;
        let account = settings
            .accounts
            .iter()
            .find(|account| account.id == connection_id && is_github_provider(account.provider))
            .cloned()
            .ok_or_else(|| "The selected GitHub connection does not exist.".to_owned())?;
        let listed = self.github_account_states().await?;
        // The user opened this diagnosis, so accounts of other GitHub users are
        // checked too; choosing one still requires confirming the change.
        Ok(self
            .diagnose_accounts(&settings, &account, &listed, true)
            .await)
    }

    async fn diagnose_accounts(
        &self,
        settings: &Settings,
        account: &Account,
        listed: &[GithubCliAccount],
        probe_other_identities: bool,
    ) -> Vec<GithubAccountAccess> {
        let repositories = repositories_for(settings, account);
        let probes = listed.iter().map(|item| {
            let repositories = &repositories;
            async move {
                if item.state != GithubCliAccountState::Ok {
                    return None;
                }
                let candidate = Account {
                    cli_username: Some(item.login.clone()),
                    ..account.clone()
                };
                match self.tools.invocation_for(&candidate) {
                    Ok(Some(cli)) => Some(
                        github_access::probe(
                            &cli,
                            repositories,
                            account.profile.as_deref(),
                            probe_other_identities,
                        )
                        .await,
                    ),
                    Ok(None) => Some(github_access::AccountProbe {
                        identity: Err(CliError::Unavailable),
                        targets: Vec::new(),
                    }),
                    Err(_) => Some(github_access::AccountProbe {
                        identity: Err(CliError::InvalidOutput),
                        targets: Vec::new(),
                    }),
                }
            }
        });
        let probes = futures::future::join_all(probes).await;
        listed
            .iter()
            .zip(probes)
            .map(|(item, probe)| summarize(account, &settings.accounts, item, probe))
            .collect()
    }

    /// Lets a user-requested refresh retry right away, except for attempts made
    /// within the last 30 seconds.
    pub async fn forget_github_pin_attempts(&self) {
        self.github_pin_attempts
            .lock()
            .await
            .retain(|_, attempt| attempt.at.elapsed() < MANUAL_RETRY_FLOOR);
    }

    /// Binds GitHub connections that have no gh account when the choice is
    /// unambiguous. It changes only ratelline settings, never gh's active account,
    /// and never reads repositories with an account of another GitHub user.
    pub(crate) async fn auto_pin_github_connections<P: SnapshotPublisher>(&self, publisher: &P) {
        let settings = self.snapshot().await.settings;
        let pending: Vec<(Account, String)> = {
            let attempts = self.github_pin_attempts.lock().await;
            if attempts
                .get(ACCOUNT_LIST_ATTEMPT)
                .is_some_and(|attempt| attempt.blocks(""))
            {
                return;
            }
            settings
                .accounts
                .iter()
                .filter(|account| {
                    is_github_provider(account.provider) && account.cli_username.is_none()
                })
                .map(|account| {
                    let repositories = repositories_for(&settings, account);
                    (account.clone(), fingerprint(account, &repositories))
                })
                .filter(|(account, fingerprint)| {
                    attempts
                        .get(&account.id)
                        .is_none_or(|attempt| !attempt.blocks(fingerprint))
                })
                .collect()
        };
        if pending.is_empty() {
            return;
        }
        let listed = match self.tools.github_account_states().await {
            Ok(listed) => {
                self.github_pin_attempts
                    .lock()
                    .await
                    .remove(ACCOUNT_LIST_ATTEMPT);
                listed
            }
            Err(error) => {
                let mut attempts = self.github_pin_attempts.lock().await;
                let attempt = failed_attempt(
                    attempts.get(ACCOUNT_LIST_ATTEMPT),
                    String::new(),
                    error == CliError::RateLimited,
                );
                attempts.insert(ACCOUNT_LIST_ATTEMPT.to_owned(), attempt);
                return;
            }
        };
        for (account, fingerprint) in pending {
            // Re-read settings so a connection pinned earlier in this loop is
            // treated as used by the following connections.
            let settings = self.snapshot().await.settings;
            let still_unbound = settings.accounts.iter().any(|current| {
                current.id == account.id
                    && current.cli_username.is_none()
                    && current.profile == account.profile
            });
            if !still_unbound {
                continue;
            }
            let results = self
                .diagnose_accounts(&settings, &account, &listed, false)
                .await;
            let rate_limited = results
                .iter()
                .any(|result| result.check_failed == Some(IssueKind::RateLimited));
            let determinate = match pin_decision(&results) {
                PinDecision::Pin(login) => self
                    .connect_github_cli_account(
                        account.provider,
                        &account.id,
                        &login,
                        false,
                        publisher,
                    )
                    .await
                    .is_ok(),
                PinDecision::Undetermined => false,
                PinDecision::NoCandidate | PinDecision::Ambiguous | PinDecision::InUse => true,
            };
            let mut attempts = self.github_pin_attempts.lock().await;
            let attempt = if determinate {
                PinAttempt {
                    fingerprint,
                    at: Instant::now(),
                    retry_after: DETERMINATE_RETRY,
                    failures: 0,
                }
            } else {
                failed_attempt(attempts.get(&account.id), fingerprint, rate_limited)
            };
            attempts.insert(account.id.clone(), attempt);
        }
    }
}

#[cfg(test)]
#[path = "github_binding_tests.rs"]
mod tests;
