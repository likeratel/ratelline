use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, BufReader},
    process::Child,
};

use crate::{
    cli::{login_timeout, CliInvocation},
    models::{LoginState, LoginStatus, Provider},
    state::AppState,
};

const MAX_LOGIN_OUTPUT_BYTES: usize = 1024 * 1024;

pub async fn begin(
    app: &AppHandle,
    provider: Provider,
    connection_id: String,
    profile: Option<String>,
) -> Result<LoginStatus, String> {
    validate_provider(provider)?;
    let state = app.state::<AppState>();
    let status = LoginStatus {
        connection_id: connection_id.clone(),
        provider,
        state: LoginState::Waiting,
        message: format!(
            "Waiting for {} sign-in to complete.",
            provider_name(provider)
        ),
        url: None,
        code: None,
    };
    let (account, status, should_start) = state
        .register_login_attempt(&connection_id, provider, status)
        .await?;
    let profile = match login_profile(&account, profile) {
        Ok(profile) => profile,
        Err(error) => {
            if should_start {
                state.login_statuses.lock().await.remove(&connection_id);
            }
            return Err(error);
        }
    };
    if !should_start {
        return Ok(status);
    }
    let invocation = match state.tools.invocation_for(&account) {
        Ok(Some(invocation)) => invocation,
        Ok(None) => {
            state.login_statuses.lock().await.remove(&connection_id);
            return Err(format!("{} CLI was not found.", provider_name(provider)));
        }
        Err(error) => {
            state.login_statuses.lock().await.remove(&connection_id);
            return Err(error);
        }
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut final_status = run_login(
            app_handle.clone(),
            provider,
            connection_id.clone(),
            profile,
            invocation,
        )
        .await;
        let state = app_handle.state::<AppState>();
        if provider == Provider::Vercel && final_status.state == LoginState::Complete {
            if let Err(message) = state
                .finalize_vercel_cli_account(&connection_id, &app_handle)
                .await
            {
                final_status = login_error(provider, &connection_id, &message);
            }
        }
        state
            .publish_login_status_if_account_exists(&connection_id, provider, final_status)
            .await;
    });
    Ok(status)
}

pub async fn get(
    app: &AppHandle,
    provider: Provider,
    connection_id: String,
) -> Result<LoginStatus, String> {
    let state = app.state::<AppState>();
    state.account(&connection_id, provider).await?;
    let status = state
        .login_statuses
        .lock()
        .await
        .get(&connection_id)
        .cloned()
        .unwrap_or(LoginStatus {
            connection_id,
            provider,
            state: LoginState::Error,
            message: "No sign-in attempt is active.".into(),
            url: None,
            code: None,
        });
    Ok(status)
}

async fn run_login(
    app: AppHandle,
    provider: Provider,
    connection_id: String,
    profile: Option<String>,
    invocation: CliInvocation,
) -> LoginStatus {
    let _permit = match invocation.acquire_process().await {
        Ok(permit) => permit,
        Err(_) => {
            return login_error(
                provider,
                &connection_id,
                "The sign-in process could not be started.",
            )
        }
    };
    let profile_arg = profile.unwrap_or_default();
    let args: Vec<&str> = match provider {
        Provider::Vercel => vec!["login", "--non-interactive"],
        Provider::Aws => vec![
            "sso",
            "login",
            "--profile",
            profile_arg.as_str(),
            "--no-browser",
            "--use-device-code",
        ],
        _ => return login_error(provider, &connection_id, "Sign in with this provider's official CLI, then use Connect and discover. No credentials are stored by ratelline."),
    };
    let mut child = match invocation.login_command(&args).spawn() {
        Ok(child) => child,
        Err(_) => {
            return login_error(
                provider,
                &connection_id,
                "The sign-in process could not be started.",
            )
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = stdout.map(|stream| {
        tauri::async_runtime::spawn(read_login_stream(
            BufReader::new(stream),
            app.clone(),
            provider,
            connection_id.clone(),
        ))
    });
    let stderr_task = stderr.map(|stream| {
        tauri::async_runtime::spawn(read_login_stream(
            BufReader::new(stream),
            app.clone(),
            provider,
            connection_id.clone(),
        ))
    });
    let result = wait_for_login(&mut child, login_timeout()).await;
    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }
    match result {
        Ok(true) => LoginStatus {
            connection_id: connection_id.clone(),
            provider,
            state: LoginState::Complete,
            message: format!("{} sign-in completed.", provider_name(provider)),
            url: None,
            code: None,
        },
        Ok(false) => login_error(
            provider,
            &connection_id,
            "Sign-in did not complete. Try again or sign in with the CLI directly.",
        ),
        Err(()) => login_error(
            provider,
            &connection_id,
            "Sign-in timed out. Try again or sign in with the CLI directly.",
        ),
    }
}

pub(crate) fn validate_provider(provider: Provider) -> Result<(), String> {
    if matches!(provider, Provider::Vercel | Provider::Aws) {
        Ok(())
    } else {
        Err("Sign in with this provider's official CLI outside ratelline, then connect the existing session.".into())
    }
}

async fn wait_for_login(child: &mut Child, duration: Duration) -> Result<bool, ()> {
    match tokio::time::timeout(duration, child.wait()).await {
        Ok(Ok(status)) => Ok(status.success()),
        Ok(Err(_)) => Ok(false),
        Err(_) => {
            let _ = child.kill().await;
            Err(())
        }
    }
}

async fn read_login_stream<R: AsyncBufRead + Unpin>(
    mut reader: R,
    app: AppHandle,
    provider: Provider,
    connection_id: String,
) {
    let mut consumed = 0;
    let mut line = String::new();
    while consumed < MAX_LOGIN_OUTPUT_BYTES {
        line.clear();
        let Ok(read) = reader.read_line(&mut line).await else {
            break;
        };
        if read == 0 {
            break;
        }
        consumed += read;
        let (url, code) = extract_login_hints(&line, provider);
        if url.is_none() && code.is_none() {
            continue;
        }
        let state = app.state::<AppState>();
        let mut statuses = state.login_statuses.lock().await;
        if let Some(current) = statuses.get(&connection_id).cloned() {
            statuses.insert(
                connection_id.clone(),
                LoginStatus {
                    connection_id: connection_id.clone(),
                    provider,
                    state: LoginState::Waiting,
                    message: format!(
                        "Complete {} sign-in in your browser.",
                        provider_name(provider)
                    ),
                    url: url.or_else(|| current.url.clone()),
                    code: code.or_else(|| current.code.clone()),
                },
            );
        }
    }
}

fn extract_login_hints(line: &str, provider: Provider) -> (Option<String>, Option<String>) {
    let url = line
        .split_whitespace()
        .map(|part| {
            part.trim_matches(|character: char| {
                matches!(character, '(' | ')' | '[' | ']' | '<' | '>' | ',' | '.')
            })
        })
        .find(|part| allowed_login_url(part, provider))
        .map(str::to_owned);
    let code = line
        .split(|character: char| character.is_whitespace() || matches!(character, ':' | '(' | ')'))
        .map(|part| {
            part.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '-'
            })
        })
        .find(|part| {
            part.len() >= 8
                && part.len() <= 12
                && part.contains('-')
                && part.chars().all(|character| {
                    character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
                })
        })
        .map(str::to_owned);
    (url, code)
}

fn allowed_login_url(value: &str, provider: Provider) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
        return false;
    }
    matches!(
        (provider, url.host_str()),
        (Provider::Github, Some("github.com")) | (Provider::Vercel, Some("vercel.com"))
    ) || match provider {
        Provider::GithubDeployments => url.host_str() == Some("github.com"),
        Provider::Railway => matches!(url.host_str(), Some("railway.com" | "railway.app")),
        Provider::Netlify => url.host_str() == Some("app.netlify.com"),
        Provider::Render => url.host_str() == Some("dashboard.render.com"),
        Provider::CloudflarePages | Provider::CloudflareWorkers => {
            url.host_str() == Some("dash.cloudflare.com")
        }
        Provider::Heroku => url.host_str() == Some("cli-auth.heroku.com"),
        Provider::Fly => url.host_str() == Some("fly.io"),
        Provider::Gitlab => url.host_str() == Some("gitlab.com"),
        Provider::Azure => matches!(
            url.host_str(),
            Some("microsoft.com" | "login.microsoftonline.com")
        ),
        Provider::Gcloud => url.host_str() == Some("accounts.google.com"),
        _ => false,
    } || provider == Provider::Aws
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".amazonaws.com") || host.ends_with(".awsapps.com"))
}

fn login_error(provider: Provider, connection_id: &str, message: &str) -> LoginStatus {
    LoginStatus {
        connection_id: connection_id.into(),
        provider,
        state: LoginState::Error,
        message: message.into(),
        url: None,
        code: None,
    }
}

fn login_profile(
    account: &crate::models::Account,
    requested: Option<String>,
) -> Result<Option<String>, String> {
    if account.provider != Provider::Aws {
        if requested.is_some() {
            return Err("Only AWS connections accept a profile.".into());
        }
        return Ok(None);
    }
    let bound = account
        .profile
        .clone()
        .ok_or_else(|| "The AWS connection has no profile.".to_owned())?;
    if requested.as_ref().is_some_and(|profile| profile != &bound) {
        return Err("The AWS profile does not match the selected connection.".into());
    }
    Ok(Some(bound))
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "GitHub",
        Provider::Vercel => "Vercel",
        Provider::Aws => "AWS",
        other => other.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_provider_login_urls_and_device_codes() {
        let hints = extract_login_hints(
            "Open https://github.com/login/device and enter ABCD-1234 (token secret)",
            Provider::Github,
        );
        assert_eq!(hints.0.as_deref(), Some("https://github.com/login/device"));
        assert_eq!(hints.1.as_deref(), Some("ABCD-1234"));
        assert_eq!(
            extract_login_hints("https://evil.example/steal ABCD-1234", Provider::Github).0,
            None
        );
        assert!(allowed_login_url(
            "https://device.sso.us-east-1.amazonaws.com/",
            Provider::Aws
        ));
    }

    #[test]
    fn login_profile_must_match_the_selected_account() {
        let aws = crate::models::Account {
            id: "aws-test".into(),
            provider: Provider::Aws,
            label: "AWS".into(),
            mode: crate::models::AccountMode::Local,
            profile: Some("engineering".into()),
            cli_username: None,
        };
        assert_eq!(
            login_profile(&aws, None).expect("use bound profile"),
            Some("engineering".into())
        );
        assert!(login_profile(&aws, Some("production".into())).is_err());
    }

    #[test]
    fn in_app_login_rejects_github_before_starting_a_process() {
        assert!(validate_provider(Provider::Github).is_err());
        assert!(validate_provider(Provider::GithubDeployments).is_err());
        assert!(validate_provider(Provider::Vercel).is_ok());
        assert!(validate_provider(Provider::Aws).is_ok());
    }
}
