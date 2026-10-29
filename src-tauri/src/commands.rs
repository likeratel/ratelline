use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::{
    login,
    models::{
        Account, Discovery, LoginStatus, Provider, Settings, SettingsChange, Snapshot,
        AWS_DISCOVERY_ID, LOCAL_GITHUB_ID, LOCAL_VERCEL_ID,
    },
    providers::{aws, github, vercel},
    state::AppState,
};

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    state.save_settings(settings, &app).await
}

#[tauri::command]
pub async fn change_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    change: SettingsChange,
) -> Result<Settings, String> {
    state.change_settings(change, &app).await
}

#[tauri::command]
pub async fn refresh(app: AppHandle, state: State<'_, AppState>) -> Result<Snapshot, String> {
    // A user-requested check re-evaluates unbound GitHub connections immediately.
    state.forget_github_pin_attempts().await;
    Ok(state.refresh(&app).await)
}

#[tauri::command]
pub fn request_refresh(state: State<'_, AppState>) {
    state.request_refresh();
}

#[tauri::command]
pub async fn discover(
    state: State<'_, AppState>,
    provider: Provider,
    connection_id: Option<String>,
    profile: Option<String>,
    region: Option<String>,
) -> Result<Discovery, String> {
    validate_optional("AWS profile", profile.as_deref())?;
    validate_optional("AWS region", region.as_deref())?;
    let connection_id = match (provider, connection_id) {
        (Provider::Github, None) => LOCAL_GITHUB_ID.to_owned(),
        (Provider::Vercel, None) => LOCAL_VERCEL_ID.to_owned(),
        (Provider::Aws, None) => {
            if profile.is_some() || region.is_some() {
                return Err("Select an AWS connection before discovering resources.".into());
            }
            return aws::discover(None, None, AWS_DISCOVERY_ID).await;
        }
        (_, Some(connection_id)) => connection_id,
        (provider, None) => format!("{}:local", provider.as_str()),
    };
    let account = state.account(&connection_id, provider).await?;
    let cli = state.tools.invocation_for(&account)?;
    match provider {
        Provider::Github => Ok(github::discover(cli.as_ref(), &connection_id).await),
        Provider::Vercel => Ok(vercel::discover(cli.as_ref(), &connection_id).await),
        Provider::Aws => {
            let bound = account
                .profile
                .clone()
                .ok_or_else(|| "The AWS connection has no profile.".to_owned())?;
            if profile.as_ref().is_some_and(|value| value != &bound) {
                return Err("The AWS profile does not match the selected connection.".into());
            }
            aws::discover(Some(bound), region, &connection_id).await
        }
        _ => Ok(crate::providers::catalog::discover(cli.as_ref(), &account).await),
    }
}

#[tauri::command]
pub async fn create_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    label: String,
    profile: Option<String>,
) -> Result<Account, String> {
    validate_optional("AWS profile", profile.as_deref())?;
    state
        .create_connection(provider, label, profile, &app)
        .await
}

#[tauri::command]
pub async fn connect_cloudflare(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    account_id: String,
    token: String,
    connection_id: Option<String>,
) -> Result<Account, String> {
    state.ensure_settings_writable()?;
    let credential = crate::providers::hosting::authenticate(provider, account_id, token).await?;
    state
        .connect_cloudflare(provider, credential, connection_id.as_deref(), &app)
        .await
}

#[tauri::command]
pub async fn register_eas_project(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    project_id: String,
    owner: String,
    slug: String,
) -> Result<Vec<crate::models::Target>, String> {
    state
        .register_eas_project(&connection_id, &project_id, &owner, &slug, &app)
        .await
}

#[tauri::command]
pub async fn connect_bitbucket(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    token: String,
    connection_id: Option<String>,
) -> Result<Account, String> {
    let credential = crate::providers::bitbucket::authenticate(email, token).await?;
    state
        .connect_bitbucket(credential, connection_id.as_deref(), &app)
        .await
}

#[tauri::command]
pub async fn get_github_cli_accounts(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .tools
        .github_accounts()
        .await
        .map_err(|error| error.user_message("GitHub"))
}

/// Lists gh accounts with their sign-in state. The active flag is informational;
/// GitHub connections always use their own selected account.
#[tauri::command]
pub async fn get_github_cli_account_states(
    state: State<'_, AppState>,
) -> Result<Vec<crate::github_cli_accounts::GithubCliAccount>, String> {
    state.github_account_states().await
}

/// Reports, for each signed-in gh account, whether it can read every repository
/// selected for the connection. Only repository IDs and logins are returned.
#[tauri::command]
pub async fn diagnose_github_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<crate::state::GithubAccountAccess>, String> {
    state.diagnose_github_connection(&connection_id).await
}

#[tauri::command]
pub async fn connect_github_cli_account(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    connection_id: String,
    username: String,
    allow_identity_change: Option<bool>,
) -> Result<Account, String> {
    state
        .connect_github_cli_account(
            provider,
            &connection_id,
            &username,
            allow_identity_change.unwrap_or(false),
            &app,
        )
        .await
}

#[tauri::command]
pub async fn remove_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Settings, String> {
    state.remove_connection(&connection_id, &app).await
}

#[tauri::command]
pub async fn begin_login(
    app: AppHandle,
    provider: Provider,
    connection_id: String,
    profile: Option<String>,
) -> Result<LoginStatus, String> {
    login::validate_provider(provider)?;
    validate_optional("AWS profile", profile.as_deref())?;
    login::begin(&app, provider, connection_id, profile).await
}

#[tauri::command]
pub async fn get_login_status(
    app: AppHandle,
    provider: Provider,
    connection_id: String,
) -> Result<LoginStatus, String> {
    login::get(&app, provider, connection_id).await
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    validate_external_url(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "The link could not be opened.".to_owned())
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) -> Result<(), String> {
    crate::windows::hide_panel(&app)
}

#[tauri::command]
pub async fn open_dashboard(app: AppHandle) -> Result<(), String> {
    crate::windows::open_dashboard(&app)
}

fn validate_optional(label: &str, value: Option<&str>) -> Result<(), String> {
    if value.is_some_and(|value| {
        value.is_empty()
            || value.len() > 128
            || value.contains('\0')
            || value.chars().any(char::is_control)
    }) {
        Err(format!("{label} is invalid."))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_external_url(value: &str) -> Result<(), String> {
    if value.len() > 2_048 {
        return Err("The link is too long.".into());
    }
    let url = url::Url::parse(value).map_err(|_| "The link is invalid.".to_owned())?;
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
        return Err("Only secure provider links can be opened.".into());
    }
    const INSTALL_GUIDES: &[&str] = &[
        "https://cli.github.com/",
        "https://vercel.com/docs/cli",
        "https://docs.railway.com/guides/cli",
        "https://docs.netlify.com/cli/get-started/",
        "https://render.com/docs/cli",
        "https://devcenter.heroku.com/articles/heroku-cli",
        "https://fly.io/docs/flyctl/install/",
        "https://docs.gitlab.com/cli/",
        "https://docs.digitalocean.com/reference/doctl/how-to/install/",
        "https://cloud.google.com/sdk/docs/install",
        "https://docs.expo.dev/eas/cli/",
        "https://learn.microsoft.com/cli/azure/install-azure-cli",
        "https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html",
    ];
    if INSTALL_GUIDES.contains(&value) {
        return Ok(());
    }
    if url.host_str() == Some("id.atlassian.com") {
        if url.path() == "/manage-profile/security/api-tokens"
            && url.port().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
        {
            return Ok(());
        }
        return Err("This Atlassian link is not the API token page.".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "The link has no host.".to_owned())?;
    let allowed = host == "github.com"
        || host == "vercel.com"
        || host.ends_with(".vercel.app")
        || host == "console.aws.amazon.com"
        || host.ends_with(".console.aws.amazon.com")
        || is_aws_device_host(host)
        || [
            "railway.com",
            "railway.app",
            "app.netlify.com",
            "dashboard.render.com",
            "dash.cloudflare.com",
            "dashboard.heroku.com",
            "fly.io",
            "gitlab.com",
            "bitbucket.org",
            "cloud.digitalocean.com",
            "console.cloud.google.com",
            "portal.azure.com",
            "expo.dev",
            "console.firebase.google.com",
            "cli-auth.heroku.com",
            "microsoft.com",
            "login.microsoftonline.com",
            "accounts.google.com",
        ]
        .contains(&host);
    if !allowed {
        return Err("This link is outside the supported provider hosts.".into());
    }
    Ok(())
}

fn is_aws_device_host(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() == 5
        && labels[0] == "device"
        && labels[1] == "sso"
        && labels[3] == "amazonaws"
        && labels[4] == "com"
        && !labels[2].is_empty()
        && labels[2]
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_url_validation_enforces_https_and_exact_host_boundaries() {
        assert!(validate_external_url("https://github.com/acme/rocket/actions/runs/1").is_ok());
        assert!(validate_external_url("https://iad1.console.aws.amazon.com/ecs/home").is_ok());
        assert!(validate_external_url(
            "https://device.sso.us-east-1.amazonaws.com/?user_code=ABCD-EFGH"
        )
        .is_ok());
        assert!(validate_external_url("http://github.com/acme/rocket").is_err());
        assert!(validate_external_url("https://cli.github.com/").is_ok());
        assert!(validate_external_url("https://vercel.com/docs/cli").is_ok());
        assert!(validate_external_url("https://cli.github.com.evil.example/").is_err());
        assert!(validate_external_url("https://vercel.com.evil.example/docs/cli").is_err());
        assert!(validate_external_url("https://device.sso.evil.amazonaws.com.example/").is_err());
        assert!(validate_external_url("https://github.com.evil.example/steal").is_err());
        assert!(validate_external_url("https://user:password@github.com/acme").is_err());
        assert!(validate_external_url(
            "https://id.atlassian.com/manage-profile/security/api-tokens"
        )
        .is_ok());
        assert!(validate_external_url(
            "https://id.atlassian.com/manage-profile/security/api-tokens?token=secret"
        )
        .is_err());
        assert!(validate_external_url("https://id.atlassian.com/manage-profile").is_err());
        assert!(validate_external_url(
            "https://id.atlassian.com:8443/manage-profile/security/api-tokens"
        )
        .is_err());
        assert!(validate_external_url(
            "https://bitbucket.org/workspace/repository/pipelines/results/7"
        )
        .is_ok());
    }
}

#[tauri::command]
pub async fn load_more(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    target_id: String,
    cursor: String,
    generation: u64,
) -> Result<Snapshot, String> {
    state
        .load_more(&connection_id, &target_id, &cursor, generation, &app)
        .await
}
