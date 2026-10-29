//! Reads which gh accounts are signed in to github.com. The active account is
//! reported for display only; ratelline never switches it.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cli::{classify_failure, is_safe_github_username, CliError, CliInvocation};

const STATUS_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ACCOUNTS: usize = 32;
const JSON_FILTER: &str = r#"(.hosts["github.com"] // []) | map({login, state, active})"#;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GithubCliAccountState {
    Ok,
    SignedOut,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubCliAccount {
    pub login: String,
    pub active: bool,
    pub state: GithubCliAccountState,
}

#[derive(Deserialize)]
struct JsonAccount {
    login: String,
    state: Option<String>,
    active: Option<bool>,
}

pub(crate) async fn list(cli: &CliInvocation) -> Result<Vec<GithubCliAccount>, CliError> {
    let (_, stdout, stderr) = cli
        .capture(
            &[
                "auth",
                "status",
                "--hostname",
                "github.com",
                "--json",
                "hosts",
                "--jq",
                JSON_FILTER,
            ],
            STATUS_TIMEOUT,
        )
        .await?;
    if let Ok(accounts) = serde_json::from_slice::<Vec<JsonAccount>>(&stdout) {
        return validated(accounts.into_iter().map(from_json).collect());
    }
    // With no account at all, gh reports it on stderr and exits 1.
    if signed_in_nowhere(&String::from_utf8_lossy(&stderr)) {
        return Ok(Vec::new());
    }
    // gh before 2.67 has no `auth status --json`; read its text report instead.
    if classify_failure(&stderr) != CliError::UnsupportedVersion {
        return Err(failure(&stderr));
    }
    let (success, stdout, stderr) = cli
        .capture(
            &["auth", "status", "--hostname", "github.com"],
            STATUS_TIMEOUT,
        )
        .await?;
    let text = [stdout, stderr].concat();
    let text = String::from_utf8(text).map_err(|_| CliError::InvalidOutput)?;
    let accounts = parse_text(&text);
    if accounts.is_empty() {
        if signed_in_nowhere(&text) {
            return Ok(Vec::new());
        }
        return Err(if success {
            CliError::InvalidOutput
        } else {
            failure(text.as_bytes())
        });
    }
    validated(accounts)
}

/// `You are not logged into any GitHub hosts.` means an empty account list,
/// unlike other sign-in failures.
fn signed_in_nowhere(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("not logged into any") || lower.contains("not logged in to any")
}

fn failure(stderr: &[u8]) -> CliError {
    match classify_failure(stderr) {
        CliError::Failed => CliError::InvalidOutput,
        error => error,
    }
}

fn from_json(account: JsonAccount) -> GithubCliAccount {
    let state = match account.state.as_deref() {
        Some("success") => GithubCliAccountState::Ok,
        Some("timeout") => GithubCliAccountState::Error,
        _ => GithubCliAccountState::SignedOut,
    };
    GithubCliAccount {
        login: account.login,
        active: account.active.unwrap_or(false),
        state,
    }
}

fn validated(accounts: Vec<GithubCliAccount>) -> Result<Vec<GithubCliAccount>, CliError> {
    if accounts.len() > MAX_ACCOUNTS
        || accounts
            .iter()
            .any(|account| !is_safe_github_username(&account.login))
    {
        return Err(CliError::InvalidOutput);
    }
    let mut unique: Vec<GithubCliAccount> = Vec::new();
    for account in accounts {
        match unique
            .iter()
            .position(|item| item.login.eq_ignore_ascii_case(&account.login))
        {
            Some(index) if account.active => unique[index].active = true,
            Some(_) => {}
            None => unique.push(account),
        }
    }
    Ok(unique)
}

/// Parses `gh auth status` text such as
/// `✓ Logged in to github.com account octo (keyring)` followed by
/// `- Active account: true`. Token lines are never read.
pub(crate) fn parse_text(text: &str) -> Vec<GithubCliAccount> {
    const MARKERS: [(&str, GithubCliAccountState); 4] = [
        (
            "failed to log in to github.com account ",
            GithubCliAccountState::SignedOut,
        ),
        (
            "timeout trying to log in to github.com account ",
            GithubCliAccountState::Error,
        ),
        (
            "logged in to github.com account ",
            GithubCliAccountState::Ok,
        ),
        ("logged in to github.com as ", GithubCliAccountState::Ok),
    ];
    let mut accounts: Vec<GithubCliAccount> = Vec::new();
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower
            .trim_start_matches([' ', '-'])
            .starts_with("active account: true")
        {
            if let Some(last) = accounts.last_mut() {
                last.active = true;
            }
            continue;
        }
        let Some((offset, marker, state)) = MARKERS
            .iter()
            .find_map(|(marker, state)| lower.find(marker).map(|offset| (offset, *marker, *state)))
        else {
            continue;
        };
        let rest = &line[offset + marker.len()..];
        let login: String = rest
            .chars()
            .take_while(|character| !character.is_whitespace())
            .collect();
        accounts.push(GithubCliAccount {
            login,
            active: false,
            state,
        });
    }
    accounts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_report_lists_accounts_states_and_active_flag_without_tokens() {
        let text = "github.com\n  \u{2713} Logged in to github.com account orbit-developer (keyring)\n  - Active account: false\n  - Token: gho_************\n\n  X Failed to log in to github.com account orbit-work (keyring)\n  - Active account: true\n  - The token in keyring is invalid.\n  X Timeout trying to log in to github.com account orbit-slow (keyring)\n";
        assert_eq!(
            parse_text(text),
            vec![
                GithubCliAccount {
                    login: "orbit-developer".into(),
                    active: false,
                    state: GithubCliAccountState::Ok,
                },
                GithubCliAccount {
                    login: "orbit-work".into(),
                    active: true,
                    state: GithubCliAccountState::SignedOut,
                },
                GithubCliAccount {
                    login: "orbit-slow".into(),
                    active: false,
                    state: GithubCliAccountState::Error,
                },
            ]
        );
        assert_eq!(
            parse_text("  \u{2713} Logged in to github.com as legacy-user (oauth_token)"),
            vec![GithubCliAccount {
                login: "legacy-user".into(),
                active: false,
                state: GithubCliAccountState::Ok,
            }]
        );
    }

    #[test]
    fn unsafe_or_excessive_logins_are_rejected() {
        let unsafe_login = vec![GithubCliAccount {
            login: "bad;login".into(),
            active: false,
            state: GithubCliAccountState::Ok,
        }];
        assert_eq!(validated(unsafe_login), Err(CliError::InvalidOutput));
        let duplicate = vec![
            GithubCliAccount {
                login: "orbit-developer".into(),
                active: false,
                state: GithubCliAccountState::Ok,
            },
            GithubCliAccount {
                login: "orbit-developer".into(),
                active: true,
                state: GithubCliAccountState::Ok,
            },
        ];
        let unique = validated(duplicate).unwrap();
        assert_eq!(unique.len(), 1);
        assert!(unique[0].active);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn older_gh_without_json_status_falls_back_to_the_text_report() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
case "$*" in
  *--json*) printf 'unknown flag: --json\n' >&2; exit 1;;
esac
printf 'github.com\n  \342\234\223 Logged in to github.com account orbit-developer (keyring)\n  - Active account: true\n'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cli = CliInvocation::for_test(script);
        let accounts = list(&cli).await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].login, "orbit-developer");
        assert!(accounts[0].active);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn no_signed_in_account_is_an_empty_list_but_other_sign_in_errors_fail() {
        use std::os::unix::fs::PermissionsExt;
        for (stderr, expected) in [
            (
                "You are not logged into any GitHub hosts. To log in, run: gh auth login",
                Ok(Vec::new()),
            ),
            ("authentication required", Err(CliError::SignedOut)),
        ] {
            let root = tempfile::tempdir().unwrap();
            let script = root.path().join("gh");
            std::fs::write(
                &script,
                format!("#!/bin/sh\nprintf '%s\\n' '{stderr}' >&2\nexit 1\n"),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(
                list(&CliInvocation::for_test(script)).await,
                expected,
                "{stderr}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn json_status_is_read_even_when_one_account_fails() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
printf '[{"login":"orbit-developer","state":"success","active":false},{"login":"orbit-work","state":"error","active":true}]'
exit 1
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let accounts = list(&CliInvocation::for_test(script)).await.unwrap();
        assert_eq!(
            accounts
                .iter()
                .map(|account| (account.login.as_str(), account.state, account.active))
                .collect::<Vec<_>>(),
            vec![
                ("orbit-developer", GithubCliAccountState::Ok, false),
                ("orbit-work", GithubCliAccountState::SignedOut, true),
            ]
        );
    }
}
