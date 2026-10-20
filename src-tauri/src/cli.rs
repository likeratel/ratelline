use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::de::DeserializeOwned;
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::{OwnedSemaphorePermit, Semaphore},
    time::timeout,
};

use crate::models::{Account, AccountMode, Provider};

const GITHUB_KEYRING_NAMESPACE: &str = "RATELLINE_GH_KEYRING_NAMESPACE";

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const PROCESS_GATE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024 + 1;

#[derive(Clone)]
pub struct CliInvocation {
    program: PathBuf,
    working_directory: Option<PathBuf>,
    prefix_args: Vec<String>,
    process_gate: std::sync::Arc<Semaphore>,
    environment: Vec<(OsString, OsString)>,
    removed_environment: Vec<OsString>,
    version_args: Vec<String>,
    version_cache: std::sync::Arc<tokio::sync::OnceCell<String>>,
    fixed_identity_response_cache: std::sync::Arc<tokio::sync::OnceCell<Vec<u8>>>,
    observed_mutable_identity: std::sync::Arc<tokio::sync::OnceCell<String>>,
    github_token_cache: std::sync::Arc<tokio::sync::OnceCell<String>>,
    compatibility_error: Option<CliError>,
    github_username: Option<String>,
    vercel_global_config: Option<PathBuf>,
    expected_identity: Option<String>,
}

impl std::fmt::Debug for CliInvocation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CliInvocation")
            .field("program", &self.program)
            .field("prefix_args", &self.prefix_args)
            .field("version_args", &self.version_args)
            .field("compatibility_error", &self.compatibility_error)
            .field("has_github_selector", &self.github_username.is_some())
            .field(
                "has_vercel_global_config",
                &self.vercel_global_config.is_some(),
            )
            .field("has_expected_identity", &self.expected_identity.is_some())
            .finish_non_exhaustive()
    }
}

impl CliInvocation {
    fn direct(program: PathBuf, process_gate: std::sync::Arc<Semaphore>) -> Self {
        Self {
            program,
            working_directory: None,
            prefix_args: Vec::new(),
            process_gate,
            environment: Vec::new(),
            removed_environment: Vec::new(),
            version_args: vec!["--version".into()],
            version_cache: std::sync::Arc::new(tokio::sync::OnceCell::new()),
            fixed_identity_response_cache: std::sync::Arc::new(tokio::sync::OnceCell::new()),
            observed_mutable_identity: std::sync::Arc::new(tokio::sync::OnceCell::new()),
            github_token_cache: std::sync::Arc::new(tokio::sync::OnceCell::new()),
            compatibility_error: None,
            github_username: None,
            vercel_global_config: None,
            expected_identity: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(program: PathBuf) -> Self {
        Self::direct(program, std::sync::Arc::new(Semaphore::new(4)))
    }

    pub(crate) fn for_eas_project(&self, directory: &Path) -> Self {
        let mut invocation = self.clone();
        invocation.working_directory = Some(directory.to_path_buf());
        invocation.environment.extend([
            (OsString::from("EAS_NO_VCS"), OsString::from("1")),
            (OsString::from("EXPO_NO_DOTENV"), OsString::from("1")),
        ]);
        invocation
    }

    pub async fn version(&self) -> Option<String> {
        self.version_result().await.ok()
    }

    async fn version_result(&self) -> Result<String, CliError> {
        self.version_cache
            .get_or_try_init(|| async {
                let args = self
                    .version_args
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let (stdout, stderr) = self.run_output(&args, Duration::from_secs(5)).await?;
                let output = if stdout.iter().any(|byte| !byte.is_ascii_whitespace()) {
                    stdout
                } else {
                    stderr
                };
                let output = String::from_utf8(output).map_err(|_| CliError::InvalidOutput)?;
                output
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(str::trim)
                    .map(str::to_owned)
                    .ok_or(CliError::InvalidOutput)
            })
            .await
            .cloned()
    }

    pub async fn json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T, CliError> {
        let output = self.run(args, COMMAND_TIMEOUT).await?;
        serde_json::from_str(&output).map_err(|_| CliError::InvalidOutput)
    }

    pub(crate) fn expected_identity(&self) -> Option<&str> {
        self.expected_identity.as_deref()
    }

    pub(crate) async fn github_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
    ) -> Result<T, CliError> {
        validate_api_path(endpoint, false)?;
        let fixed_identity = endpoint == "user" && self.github_username.is_some();
        let response = if fixed_identity {
            self.fixed_identity_response_cache
                .get_or_try_init(|| self.github_json_bytes(endpoint))
                .await
                .cloned()?
        } else {
            self.github_json_bytes(endpoint).await?
        };
        if endpoint == "user" && !fixed_identity {
            self.verify_mutable_identity(Provider::Github, &response)
                .await?;
        }
        serde_json::from_slice(&response).map_err(|_| CliError::InvalidOutput)
    }

    async fn github_json_bytes(&self, endpoint: &str) -> Result<Vec<u8>, CliError> {
        let _permit = self.acquire_process().await?;
        let token = match self.github_username.as_deref() {
            Some(username) => Some(self.github_token(username).await?),
            None => None,
        };
        let mut invocation = self.clone();
        if let Some(token) = token.as_deref() {
            invocation
                .environment
                .push((OsString::from("GH_TOKEN"), OsString::from(token)));
        }
        let (stdout, _) = invocation
            .run_output_ungated(
                &[
                    "api",
                    endpoint,
                    "--hostname",
                    "github.com",
                    "--method",
                    "GET",
                    "--header",
                    "Accept: application/vnd.github+json",
                    "--header",
                    "X-GitHub-Api-Version: 2022-11-28",
                ],
                COMMAND_TIMEOUT,
            )
            .await?;
        Ok(stdout)
    }

    pub(crate) async fn vercel_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        scope: Option<&str>,
    ) -> Result<T, CliError> {
        validate_api_path(endpoint, true)?;
        if scope.is_some_and(|value| !is_safe_selector(value, 128)) {
            return Err(CliError::InvalidOutput);
        }
        let response = self.vercel_json_bytes(endpoint, scope).await?;
        if endpoint == "/v2/user" && scope.is_none() {
            self.verify_mutable_identity(Provider::Vercel, &response)
                .await?;
        }
        serde_json::from_slice(&response).map_err(|_| CliError::InvalidOutput)
    }

    async fn verify_mutable_identity(
        &self,
        provider: Provider,
        response: &[u8],
    ) -> Result<(), CliError> {
        let identity = response_identity(provider, response)?;
        let observed = self
            .observed_mutable_identity
            .get_or_init(|| async { identity.clone() })
            .await;
        if observed == &identity {
            Ok(())
        } else {
            Err(CliError::AccountMismatch)
        }
    }

    async fn vercel_json_bytes(
        &self,
        endpoint: &str,
        scope: Option<&str>,
    ) -> Result<Vec<u8>, CliError> {
        let mut owned = vec![
            "api".to_owned(),
            endpoint.to_owned(),
            "--method".to_owned(),
            "GET".to_owned(),
            "--raw".to_owned(),
            "--non-interactive".to_owned(),
        ];
        if let Some(scope) = scope {
            owned.extend(["--scope".to_owned(), scope.to_owned()]);
        }
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        self.run_output(&args, COMMAND_TIMEOUT)
            .await
            .map(|(stdout, _)| stdout)
    }

    async fn github_token(&self, username: &str) -> Result<String, CliError> {
        self.github_token_cache
            .get_or_try_init(|| self.fetch_github_token(username))
            .await
            .cloned()
    }

    async fn fetch_github_token(&self, username: &str) -> Result<String, CliError> {
        if !is_safe_github_username(username) {
            return Err(CliError::InvalidOutput);
        }
        let (stdout, _) = self
            .run_output_ungated(
                &[
                    "auth",
                    "token",
                    "--hostname",
                    "github.com",
                    "--user",
                    username,
                ],
                Duration::from_secs(10),
            )
            .await?;
        let token = String::from_utf8(stdout).map_err(|_| CliError::InvalidOutput)?;
        let token = token.trim();
        if token.is_empty() || token.len() > 4096 || token.chars().any(char::is_whitespace) {
            return Err(CliError::InvalidOutput);
        }
        Ok(token.to_owned())
    }

    pub async fn run(&self, args: &[&str], duration: Duration) -> Result<String, CliError> {
        let (stdout, _) = self.run_output(args, duration).await?;
        String::from_utf8(stdout).map_err(|_| CliError::InvalidOutput)
    }

    async fn run_output(
        &self,
        args: &[&str],
        duration: Duration,
    ) -> Result<(Vec<u8>, Vec<u8>), CliError> {
        if let Some(error) = self.compatibility_error {
            return Err(error);
        }
        let _permit = self.acquire_process().await?;
        self.run_output_ungated(args, duration).await
    }

    async fn run_output_ungated(
        &self,
        args: &[&str],
        duration: Duration,
    ) -> Result<(Vec<u8>, Vec<u8>), CliError> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.prefix_args)
            .args(args)
            .env("CI", "1")
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(path) = &self.vercel_global_config {
            command.arg("--global-config").arg(path);
        }
        if let Some(directory) = &self.working_directory {
            command.current_dir(directory);
        }
        for name in &self.removed_environment {
            command.env_remove(name);
        }
        command.envs(self.environment.iter().cloned());

        #[cfg(windows)]
        command.creation_flags(0x08000000);

        let mut child = command.spawn().map_err(|_| CliError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(CliError::Unavailable)?;
        let stderr = child.stderr.take().ok_or(CliError::Unavailable)?;
        let output = timeout(duration, async move {
            let stdout_task = tokio::spawn(read_limited(stdout));
            let stderr_task = tokio::spawn(read_limited(stderr));
            let status = child.wait().await.map_err(|_| CliError::Failed)?;
            let stdout = stdout_task.await.map_err(|_| CliError::Failed)??;
            let stderr = stderr_task.await.map_err(|_| CliError::Failed)??;
            Ok::<_, CliError>((status.success(), stdout, stderr))
        })
        .await
        .map_err(|_| CliError::TimedOut)??;

        if !output.0 {
            return Err(classify_failure(&output.2));
        }
        Ok((output.1, output.2))
    }

    pub fn login_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args(&self.prefix_args)
            .args(args)
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(path) = &self.vercel_global_config {
            command.arg("--global-config").arg(path);
        }
        if let Some(directory) = &self.working_directory {
            command.current_dir(directory);
        }
        for name in &self.removed_environment {
            command.env_remove(name);
        }
        command.envs(self.environment.iter().cloned());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        command
    }

    pub async fn acquire_process(&self) -> Result<OwnedSemaphorePermit, CliError> {
        if let Some(error) = self.compatibility_error {
            return Err(error);
        }
        timeout(
            PROCESS_GATE_TIMEOUT,
            self.process_gate.clone().acquire_owned(),
        )
        .await
        .map_err(|_| CliError::Busy)?
        .map_err(|_| CliError::Unavailable)
    }
}

fn validate_api_path(value: &str, require_leading_slash: bool) -> Result<(), CliError> {
    if value.is_empty()
        || require_leading_slash && !value.starts_with('/')
        || value.starts_with("http")
        || value.contains("..")
        || value.contains(['\r', '\n'])
    {
        Err(CliError::InvalidOutput)
    } else {
        Ok(())
    }
}

fn response_identity(provider: Provider, response: &[u8]) -> Result<String, CliError> {
    let value: serde_json::Value =
        serde_json::from_slice(response).map_err(|_| CliError::InvalidOutput)?;
    match provider {
        Provider::Github => value
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .map(|identity| identity.to_string()),
        Provider::Vercel => value
            .get("user")
            .and_then(|user| user.get("id"))
            .and_then(serde_json::Value::as_str)
            .filter(|identity| !identity.is_empty() && identity.len() <= 128)
            .map(str::to_owned),
        _ => None,
    }
    .ok_or(CliError::InvalidOutput)
}

fn is_safe_selector(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn is_safe_github_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 39
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
}

async fn read_limited<R: tokio::io::AsyncRead + Unpin>(reader: R) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_OUTPUT_BYTES)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| CliError::Failed)?;
    if bytes.len() as u64 >= MAX_OUTPUT_BYTES {
        return Err(CliError::OutputTooLarge);
    }
    Ok(bytes)
}

fn classify_failure(stderr: &[u8]) -> CliError {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if [
        "not logged",
        "not authenticated",
        "authentication required",
        "no oauth token found",
        "no existing credentials found",
        "log in",
        "login required",
    ]
    .iter()
    .any(|needle| text.contains(needle))
    {
        CliError::SignedOut
    } else if text.contains("rate limit") {
        CliError::RateLimited
    } else if [
        "unknown command",
        "unknown option",
        "unknown flag",
        "unrecognized argument",
        "unrecognized option",
        "invalid choice",
        "unexpected argument",
        "wasn't expected",
        "is not a command",
    ]
    .iter()
    .any(|needle| text.contains(needle))
    {
        CliError::UnsupportedVersion
    } else {
        CliError::Failed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliError {
    Unavailable,
    UnsupportedVersion,
    Busy,
    SignedOut,
    AccountMismatch,
    RateLimited,
    TimedOut,
    OutputTooLarge,
    InvalidOutput,
    Failed,
}

impl CliError {
    pub fn user_message(self, provider: &str) -> String {
        match self {
            Self::Unavailable => format!("{provider} CLI is not available."),
            Self::UnsupportedVersion => format!(
                "The installed {provider} CLI version is not compatible with this request. Update the CLI and try again."
            ),
            Self::Busy => {
                format!("{provider} is already handling another request. Try again shortly.")
            }
            Self::SignedOut => format!("Sign in to {provider} to continue."),
            Self::AccountMismatch => format!(
                "The active {provider} CLI account does not match this connection. Reconnect the intended account."
            ),
            Self::RateLimited => {
                format!("{provider} rate limit was reached. Cached data is shown.")
            }
            Self::TimedOut => format!("{provider} did not respond in time. Cached data is shown."),
            Self::OutputTooLarge => {
                format!("{provider} returned more data than ratelline can safely process.")
            }
            Self::InvalidOutput => format!("{provider} returned an unsupported response."),
            Self::Failed => {
                format!("{provider} request failed. Check account authorization and the network connection.")
            }
        }
    }
}

#[derive(Debug)]
pub struct Toolchain {
    github: Option<CliInvocation>,
    vercel: Option<CliInvocation>,
    pub aws: Option<CliInvocation>,
    local: HashMap<Provider, CliInvocation>,
    account_config_root: PathBuf,
    account_gates: std::sync::Mutex<HashMap<String, std::sync::Arc<Semaphore>>>,
    /// Concurrent CLI processes allowed per account.
    account_gate_permits: usize,
}

/// Production allows four CLI processes per account at a time.
const ACCOUNT_GATE_PERMITS: usize = 4;

impl Toolchain {
    pub async fn discover(_resource_dir: Option<&Path>, app_data_dir: &Path) -> Self {
        let directories = candidate_directories();
        Self::discover_in(&directories, app_data_dir).await
    }

    async fn discover_in(directories: &[PathBuf], app_data_dir: &Path) -> Self {
        let execution_path = env::join_paths(directories).ok();
        let aws = discover_cli(&["aws"], 1, &["--version"], directories, &execution_path);
        let github = discover_cli(&["gh"], 4, &["--version"], directories, &execution_path);
        let vercel = discover_cli(&["vercel"], 4, &["--version"], directories, &execution_path);
        // Firebase reuses the same gcloud executable and version check.
        let local = futures::future::join_all(
            crate::providers::catalog::local_tools()
                .iter()
                .filter(|(provider, _)| *provider != Provider::Firebase)
                .map(|(provider, names)| {
                    let execution_path = &execution_path;
                    async move {
                        discover_cli(
                            names,
                            4,
                            version_args(*provider),
                            directories,
                            execution_path,
                        )
                        .await
                        .map(|invocation| (*provider, invocation))
                    }
                }),
        );
        let (aws, github, vercel, local) = tokio::join!(aws, github, vercel, local);
        let mut local: HashMap<_, _> = local.into_iter().flatten().collect();
        if let Some(gcloud) = local.get(&Provider::Gcloud).cloned() {
            local.insert(Provider::Firebase, gcloud);
        }
        Self {
            github,
            vercel,
            aws,
            local,
            account_config_root: app_data_dir.join("accounts"),
            account_gates: std::sync::Mutex::new(HashMap::new()),
            account_gate_permits: ACCOUNT_GATE_PERMITS,
        }
    }

    #[cfg(test)]
    pub(crate) fn unavailable_for_test(app_data_dir: &Path) -> Self {
        Self {
            github: None,
            vercel: None,
            aws: None,
            local: HashMap::new(),
            account_config_root: app_data_dir.join("accounts"),
            account_gates: std::sync::Mutex::new(HashMap::new()),
            account_gate_permits: ACCOUNT_GATE_PERMITS,
        }
    }

    /// The fake GitHub CLI of the refresh tests blocks active probes until a later recent
    /// lookup starts, so the per-account gate must hold every probe plus the lookups they wait
    /// for; with the production four, the probes starve the lookup on a slow runner.
    #[cfg(all(test, unix))]
    pub(crate) fn github_for_test(app_data_dir: &Path, cli: CliInvocation) -> Self {
        Self {
            github: Some(cli),
            account_gate_permits: 32,
            ..Self::unavailable_for_test(app_data_dir)
        }
    }

    pub fn invocation_for(&self, account: &Account) -> Result<Option<CliInvocation>, String> {
        if matches!(
            account.provider,
            Provider::Bitbucket | Provider::CloudflarePages | Provider::CloudflareWorkers
        ) {
            account.validate()?;
            return Ok(None);
        }
        self.prepare_account(account)?;
        let base = match (account.provider, account.mode) {
            (Provider::Github | Provider::GithubDeployments, _) => self.github.as_ref(),
            (Provider::Vercel, _) => self.vercel.as_ref(),
            (Provider::Aws, _) => self.aws.as_ref(),
            (provider, _) => self.local.get(&provider),
        };
        let Some(base) = base else {
            return Ok(None);
        };
        let gate = {
            let mut gates = self
                .account_gates
                .lock()
                .map_err(|_| "CLI account coordination is unavailable.".to_owned())?;
            gates
                .entry(account.id.clone())
                .or_insert_with(|| std::sync::Arc::new(Semaphore::new(self.account_gate_permits)))
                .clone()
        };
        let mut invocation = base.clone();
        invocation.process_gate = gate;
        // Authentication is revalidated once for every independently requested
        // operation while clones within that operation share the result.
        invocation.fixed_identity_response_cache =
            std::sync::Arc::new(tokio::sync::OnceCell::new());
        invocation.observed_mutable_identity = std::sync::Arc::new(tokio::sync::OnceCell::new());
        invocation.github_token_cache = std::sync::Arc::new(tokio::sync::OnceCell::new());
        invocation.removed_environment = provider_environment(account.provider)
            .iter()
            .map(OsString::from)
            .collect();

        invocation.expected_identity = account.profile.clone();
        if matches!(
            account.provider,
            Provider::Github | Provider::GithubDeployments
        ) {
            invocation.github_username = account.cli_username.clone();
            if account.mode == AccountMode::Isolated && invocation.github_username.is_none() {
                return Err("Reconnect this GitHub account to select its CLI identity.".into());
            }
        }
        if account.provider == Provider::Vercel && account.mode == AccountMode::Isolated {
            let directory = self.account_config_root.join(&account.id);
            std::fs::create_dir_all(&directory)
                .map_err(|_| "Could not prepare the Vercel account directory.".to_owned())?;
            invocation.vercel_global_config = Some(directory);
        }

        Ok(Some(invocation))
    }

    pub async fn github_accounts(&self) -> Result<Vec<String>, CliError> {
        let mut cli = self.github.clone().ok_or(CliError::Unavailable)?;
        cli.removed_environment = provider_environment(Provider::Github)
            .iter()
            .map(OsString::from)
            .collect();
        let accounts: Vec<String> = cli
            .json(&[
                "auth",
                "status",
                "--hostname",
                "github.com",
                "--json",
                "hosts",
                "--jq",
                ".hosts[\"github.com\"] | map(.login) | unique",
            ])
            .await?;
        if accounts
            .iter()
            .any(|account| !is_safe_github_username(account))
        {
            return Err(CliError::InvalidOutput);
        }
        Ok(accounts)
    }

    pub fn prepare_account(&self, account: &Account) -> Result<(), String> {
        account.validate()
    }
}

fn provider_environment(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Github | Provider::GithubDeployments => &[
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GH_HOST",
            GITHUB_KEYRING_NAMESPACE,
        ],
        Provider::Vercel => &[
            "VERCEL_TOKEN",
            "VERCEL_TEAM_ID",
            "VERCEL_ORG_ID",
            "VERCEL_PROJECT_ID",
            "VERCEL_SCOPE",
        ],
        Provider::Eas => &[
            "EXPO_TOKEN",
            "EAS_PROJECT_ROOT",
            "EXPO_API_URL",
            "EXPO_STAGING",
            "EXPO_LOCAL",
        ],
        Provider::Firebase => &[
            "FIREBASE_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "CLOUDSDK_AUTH_ACCESS_TOKEN",
            "CLOUDSDK_AUTH_ACCESS_TOKEN_FILE",
            "CLOUDSDK_AUTH_TOKEN_HOST",
            "CLOUDSDK_AUTH_CREDENTIAL_FILE_OVERRIDE",
            "CLOUDSDK_AUTH_IMPERSONATE_SERVICE_ACCOUNT",
            "CLOUDSDK_CORE_ACCOUNT",
            "CLOUDSDK_CORE_PROJECT",
            "CLOUDSDK_API_ENDPOINT_OVERRIDES_CLOUDRESOURCEMANAGER",
        ],
        Provider::Supabase => &["SUPABASE_ACCESS_TOKEN"],
        _ => &[],
    }
}

fn version_args(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Digitalocean => &["version"],
        _ => &["--version"],
    }
}

async fn discover_cli(
    names: &[&str],
    concurrency: usize,
    version_args: &[&str],
    directories: &[PathBuf],
    execution_path: &Option<OsString>,
) -> Option<CliInvocation> {
    let path = find_executable_in(names, directories.iter().cloned())?;
    let mut invocation =
        CliInvocation::direct(path, std::sync::Arc::new(Semaphore::new(concurrency)));
    invocation.version_args = version_args
        .iter()
        .map(|argument| (*argument).into())
        .collect();
    if let Some(path) = execution_path
        .as_ref()
        .and_then(|path| sibling_first_path(invocation.program.parent(), path))
    {
        invocation.environment.push((OsString::from("PATH"), path));
    }
    if invocation
        .version_result()
        .await
        .is_ok_and(|version| parse_version(&version).is_none())
    {
        invocation.compatibility_error = Some(CliError::UnsupportedVersion);
    }
    Some(invocation)
}

/// Puts the CLI's own directory first so `#!/usr/bin/env node` scripts run
/// with the interpreter installed next to them (for example, the same nvm version).
fn sibling_first_path(directory: Option<&Path>, execution_path: &OsString) -> Option<OsString> {
    let directories = directory
        .map(Path::to_path_buf)
        .into_iter()
        .chain(env::split_paths(execution_path));
    let mut seen = HashSet::new();
    env::join_paths(directories.filter(|path| seen.insert(path.clone()))).ok()
}

fn parse_version(value: &str) -> Option<(u32, u32, u32)> {
    value
        .split(|character: char| !character.is_ascii_digit() && character != '.')
        .find_map(|part| {
            let numbers = part
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            (numbers.len() >= 2)
                .then(|| (numbers[0], numbers[1], numbers.get(2).copied().unwrap_or(0)))
        })
}

fn find_executable_in(
    names: &[&str],
    directories: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    directories
        .into_iter()
        .flat_map(|dir| {
            executable_names(names)
                .into_iter()
                .map(move |name| dir.join(name))
        })
        .find(|path| path.is_file())
}

fn executable_names(names: &[&str]) -> Vec<String> {
    if cfg!(windows) {
        names
            .iter()
            .flat_map(|name| [format!("{name}.exe"), format!("{name}.cmd")])
            .collect()
    } else {
        names.iter().map(|name| (*name).to_owned()).collect()
    }
}

fn candidate_directories() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> =
        env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
    for variable in ["NVM_BIN", "PNPM_HOME"] {
        if let Some(path) = env::var_os(variable) {
            candidates.push(PathBuf::from(path));
        }
    }
    for variable in ["VOLTA_HOME", "CARGO_HOME"] {
        if let Some(root) = env::var_os(variable) {
            candidates.push(PathBuf::from(root).join("bin"));
        }
    }
    if cfg!(target_os = "macos") {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ]);
        if let Some(home) = env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates.extend([
                home.join(".local/bin"),
                home.join(".npm-global/bin"),
                home.join(".local/share/pnpm"),
                home.join("Library/pnpm"),
                home.join(".volta/bin"),
                home.join(".asdf/shims"),
                home.join(".local/share/mise/shims"),
                home.join("google-cloud-sdk/bin"),
            ]);
            candidates.extend(crate::nvm::bin_directories(&nvm_dir(&home)));
        }
    }
    if cfg!(target_os = "linux") {
        candidates.extend([
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/snap/bin"),
        ]);
        if let Some(home) = env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates.extend([
                home.join(".local/bin"),
                home.join(".npm-global/bin"),
                home.join(".local/share/pnpm"),
                home.join(".volta/bin"),
                home.join(".asdf/shims"),
                home.join(".local/share/mise/shims"),
                home.join("google-cloud-sdk/bin"),
            ]);
            candidates.extend(crate::nvm::bin_directories(&nvm_dir(&home)));
        }
    }
    if cfg!(windows) {
        for variable in ["LOCALAPPDATA", "APPDATA", "ProgramFiles"] {
            if let Some(root) = env::var_os(variable) {
                let root = PathBuf::from(root);
                candidates.extend([
                    root.join("Programs/GitHub CLI"),
                    root.join("npm"),
                    root.join("Programs/nodejs"),
                    root.join("Amazon/AWSCLIV2"),
                    root.join("heroku/bin"),
                    root.join("Microsoft SDKs/Azure/CLI2/wbin"),
                    root.join("Google/Cloud SDK/google-cloud-sdk/bin"),
                ]);
            }
        }
        if let Some(root) = env::var_os("USERPROFILE") {
            candidates.push(PathBuf::from(root).join("scoop/shims"));
        }
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn nvm_dir(home: &Path) -> PathBuf {
    env::var_os("NVM_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".nvm"))
}

pub fn login_timeout() -> Duration {
    LOGIN_TIMEOUT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_errors_are_classified_without_returning_stderr() {
        let error = classify_failure(b"token abc-secret is invalid; authentication required");
        assert_eq!(error, CliError::SignedOut);
        assert!(!error.user_message("GitHub").contains("abc-secret"));
    }

    #[test]
    fn parses_cli_versions_without_trusting_surrounding_text() {
        assert_eq!(
            parse_version("gh version 2.66.0 (2025-01-30)"),
            Some((2, 66, 0))
        );
        assert_eq!(parse_version("Vercel CLI 50.29.0"), Some((50, 29, 0)));
        assert_eq!(parse_version("unknown"), None);
        assert_eq!(
            parse_version("aws-cli/2.17.40 Python/3.11"),
            Some((2, 17, 40))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn version_is_cached_across_concurrent_invocation_clones() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("ratelline-version-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-cli");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '1' >> \"$0.calls\"\nprintf 'fixture 1.0.0\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cli = CliInvocation::for_test(script.clone());
        let clone = cli.clone();
        let (first, second) = tokio::join!(cli.version(), clone.version());
        assert_eq!(first, second);
        assert_eq!(first.as_deref(), Some("fixture 1.0.0"));
        assert_eq!(
            std::fs::read_to_string(script.with_extension("calls")).unwrap(),
            "1"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn selected_github_user_uses_ephemeral_token_without_global_switch() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("ratelline-gh-user-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "token" ]; then
  printf 'fictional-secret-token\n'
  printf '%s\n' "$*" >> "$0.log"
  exit 0
fi
printf '%s token=%s\n' "$*" "${GH_TOKEN:+present}" >> "$0.log"
printf '{}\n'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut cli = CliInvocation::for_test(script.clone());
        cli.github_username = Some("octo-user".into());
        let _: serde_json::Value = cli.github_json("user").await.unwrap();
        let _: serde_json::Value = cli.github_json("repos/sample").await.unwrap();
        let mut second = CliInvocation::for_test(script.clone());
        second.github_username = Some("work-user".into());
        let _: serde_json::Value = second.github_json("user").await.unwrap();
        let log = std::fs::read_to_string(script.with_extension("log")).unwrap();
        assert!(log.contains("auth token --hostname github.com --user octo-user"));
        assert!(log.contains("auth token --hostname github.com --user work-user"));
        assert!(log.contains("api user --hostname github.com --method GET"));
        assert!(log.contains("token=present"));
        assert!(!log.contains("auth switch"));
        assert_eq!(
            log.lines()
                .filter(|line| {
                    line.contains("auth token --hostname github.com --user octo-user")
                })
                .count(),
            1,
            "one operation must reuse its ephemeral token"
        );
        assert!(!format!("{cli:?}").contains("fictional-secret-token"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unbound_github_identity_change_blocks_resource_calls_until_a_fresh_operation() {
        use std::os::unix::fs::PermissionsExt;
        async fn verified_resource(cli: &CliInvocation) -> Result<serde_json::Value, CliError> {
            let _: serde_json::Value = cli.github_json("user").await?;
            cli.github_json("repos/sample").await
        }

        let root =
            std::env::temp_dir().join(format!("ratelline-gh-identity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
if [ "$2" = user ]; then
  if [ "$(cat "$0.identity")" = A ]; then
    printf '{"id":1,"login":"account-a"}\n'
  else
    printf '{"id":2,"login":"account-b"}\n'
  fi
  exit 0
fi
printf 'resource\n' >> "$0.resources"
printf '{}\n'
"#,
        )
        .unwrap();
        std::fs::write(script.with_extension("identity"), "A").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let operation = CliInvocation::for_test(script.clone());

        verified_resource(&operation).await.unwrap();
        std::fs::write(script.with_extension("identity"), "B").unwrap();
        assert_eq!(
            verified_resource(&operation).await.unwrap_err(),
            CliError::AccountMismatch
        );
        assert_eq!(
            std::fs::read_to_string(script.with_extension("resources"))
                .unwrap()
                .lines()
                .count(),
            1,
            "a changed account must be rejected before resource lookup"
        );

        let fresh_operation = CliInvocation::for_test(script.clone());
        verified_resource(&fresh_operation).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(script.with_extension("resources"))
                .unwrap()
                .lines()
                .count(),
            2,
            "a fresh operation may accept the current account"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn transient_identity_failure_is_not_cached() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("ratelline-identity-retry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
printf 'identity\n' >> "$0.calls"
if [ ! -f "$0.ready" ]; then
  touch "$0.ready"
  printf 'authentication required\n' >&2
  exit 1
fi
printf '{"id":42,"login":"synthetic-user"}\n'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cli = CliInvocation::for_test(script.clone());

        assert_eq!(
            cli.github_json::<serde_json::Value>("user")
                .await
                .unwrap_err(),
            CliError::SignedOut
        );
        let recovered: serde_json::Value = cli.github_json("user").await.unwrap();
        assert_eq!(recovered["login"], "synthetic-user");
        assert_eq!(
            std::fs::read_to_string(script.with_extension("calls"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn isolated_vercel_api_always_passes_config_and_explicit_scope() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("ratelline-vercel-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("vercel");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$0.log\"\nprintf '{}\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut cli = CliInvocation::for_test(script.clone());
        cli.vercel_global_config = Some(root.join("private-config"));
        let _: serde_json::Value = cli
            .vercel_json("/v9/projects", Some("personal-scope"))
            .await
            .unwrap();
        let log = std::fs::read_to_string(script.with_extension("log")).unwrap();
        assert!(log.contains("api /v9/projects --method GET --raw --non-interactive"));
        assert!(log.contains("--global-config"));
        assert!(log.contains("private-config"));
        assert!(log.contains("--scope personal-scope"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn vercel_identity_change_blocks_resource_calls_until_a_fresh_operation() {
        use std::os::unix::fs::PermissionsExt;
        async fn verified_resource(cli: &CliInvocation) -> Result<serde_json::Value, CliError> {
            let _: serde_json::Value = cli.vercel_json("/v2/user", None).await?;
            cli.vercel_json("/v7/deployments", Some("team-scope")).await
        }

        let root = std::env::temp_dir().join(format!(
            "ratelline-vercel-identity-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("vercel");
        std::fs::write(
            &script,
            r#"#!/bin/sh
if [ "$2" = /v2/user ]; then
  if [ "$(cat "$0.identity")" = A ]; then
    printf '{"user":{"id":"user_1","username":"account-a"}}\n'
  else
    printf '{"user":{"id":"user_2","username":"account-b"}}\n'
  fi
  exit 0
fi
printf 'resource\n' >> "$0.resources"
printf '{}\n'
"#,
        )
        .unwrap();
        std::fs::write(script.with_extension("identity"), "A").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let operation = CliInvocation::for_test(script.clone());

        verified_resource(&operation).await.unwrap();
        std::fs::write(script.with_extension("identity"), "B").unwrap();
        assert_eq!(
            verified_resource(&operation).await.unwrap_err(),
            CliError::AccountMismatch
        );
        assert_eq!(
            std::fs::read_to_string(script.with_extension("resources"))
                .unwrap()
                .lines()
                .count(),
            1,
            "a changed account must be rejected before resource lookup"
        );

        let fresh_operation = CliInvocation::for_test(script.clone());
        verified_resource(&fresh_operation).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(script.with_extension("resources"))
                .unwrap()
                .lines()
                .count(),
            2,
            "a fresh operation may accept the current account"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatibility_failures_are_separate_from_auth_and_generic_failures() {
        assert_eq!(
            classify_failure(b"error: unknown command 'deployments:list'"),
            CliError::UnsupportedVersion
        );
        assert_eq!(
            classify_failure(b"error: unexpected argument '--json' was found"),
            CliError::UnsupportedVersion
        );
        assert_eq!(
            classify_failure(b"remote service unavailable"),
            CliError::Failed
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn startup_probes_run_together_and_share_gcloud_detection() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        for name in ["aws", "gh", "vercel", "gcloud"] {
            let script = root.path().join(name);
            std::fs::write(&script, r#"#!/bin/sh
root=${0%/*}
printf 'started\n' >> "$0.started"
attempt=0
while [ ! -f "$root/aws.started" ] || [ ! -f "$root/gh.started" ] || [ ! -f "$root/vercel.started" ] || [ ! -f "$root/gcloud.started" ]; do
  attempt=$((attempt + 1))
  [ "$attempt" -lt 200 ] || exit 1
  /bin/sleep 0.01
done
printf '1.2.3\n'
"#).unwrap();
            std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let tools = Toolchain::discover_in(&[root.path().to_owned()], root.path()).await;
        for cli in [
            tools.aws.as_ref(),
            tools.github.as_ref(),
            tools.vercel.as_ref(),
            tools.local.get(&Provider::Firebase),
            tools.local.get(&Provider::Gcloud),
        ] {
            assert_eq!(cli.unwrap().version().await.as_deref(), Some("1.2.3"));
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join("gcloud.started"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installed_cli_with_unrecognized_version_reports_compatibility_error() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "ratelline-unknown-version-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-cli");
        std::fs::write(&script, "#!/bin/sh\nprintf 'development build\\n'\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let invocation = discover_cli(
            &["fake-cli"],
            1,
            &["--version"],
            std::slice::from_ref(&root),
            &None,
        )
        .await
        .expect("discover installed CLI");
        assert_eq!(
            invocation.version().await.as_deref(),
            Some("development build")
        );
        assert_eq!(
            invocation.run(&["list"], Duration::from_secs(1)).await,
            Err(CliError::UnsupportedVersion)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provider_version_subcommand_and_stderr_version_are_supported() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("ratelline-version-args-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        let doctl = root.join("doctl");
        std::fs::write(
            &doctl,
            "#!/bin/sh\n[ \"$1\" = version ] || exit 2\nprintf 'doctl version 1.120.0\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&doctl, std::fs::Permissions::from_mode(0o700)).unwrap();
        let doctl = discover_cli(
            &["doctl"],
            1,
            version_args(Provider::Digitalocean),
            std::slice::from_ref(&root),
            &None,
        )
        .await
        .expect("discover doctl");
        assert_eq!(
            doctl.version().await.as_deref(),
            Some("doctl version 1.120.0")
        );

        let aws = root.join("aws");
        std::fs::write(
            &aws,
            "#!/bin/sh\nprintf 'aws-cli/2.17.40 Python/3.11\\n' >&2\n",
        )
        .unwrap();
        std::fs::set_permissions(&aws, std::fs::Permissions::from_mode(0o700)).unwrap();
        let aws = discover_cli(
            &["aws"],
            1,
            &["--version"],
            std::slice::from_ref(&root),
            &None,
        )
        .await
        .expect("discover aws");
        assert_eq!(
            aws.version().await.as_deref(),
            Some("aws-cli/2.17.40 Python/3.11")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn transient_version_probe_failure_does_not_disable_installed_cli() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("ratelline-version-retry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("retry-cli");
        std::fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = --version ]; then\n  if [ ! -f \"$0.ready\" ]; then touch \"$0.ready\"; exit 1; fi\n  printf 'retry-cli 2.0.0\\n'\nelse\n  printf 'ok\\n'\nfi\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cli = discover_cli(
            &["retry-cli"],
            1,
            &["--version"],
            std::slice::from_ref(&root),
            &None,
        )
        .await
        .expect("keep CLI after transient probe failure");

        assert_eq!(
            cli.run(&["list"], Duration::from_secs(1)).await.as_deref(),
            Ok("ok\n")
        );
        assert_eq!(cli.version().await.as_deref(), Some("retry-cli 2.0.0"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn discovered_cli_receives_expanded_gui_path() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("ratelline-cli-path-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("path-cli");
        std::fs::write(&script, "#!/bin/sh\nprintf 'path-cli 1.0.0\\n'\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let expanded_path = env::join_paths([root.clone()]).unwrap();
        let cli = discover_cli(
            &["path-cli"],
            1,
            &["--version"],
            std::slice::from_ref(&root),
            &Some(expanded_path.clone()),
        )
        .await
        .expect("discover CLI with expanded PATH");

        assert!(cli
            .environment
            .contains(&(OsString::from("PATH"), expanded_path)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cli_directory_leads_execution_path_without_duplicates() {
        let shared = PathBuf::from("/fixture/shared/bin");
        let execution_path = env::join_paths([PathBuf::from("/usr/bin"), shared.clone()]).unwrap();

        assert_eq!(
            sibling_first_path(Some(&shared), &execution_path),
            env::join_paths([shared, PathBuf::from("/usr/bin")]).ok()
        );
        assert_eq!(
            sibling_first_path(None, &execution_path),
            Some(execution_path)
        );
    }

    #[test]
    fn executable_discovery_uses_explicit_directory_order_and_aliases() {
        let root =
            std::env::temp_dir().join(format!("ratelline-discovery-{}", uuid::Uuid::new_v4()));
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let expected = second.join(executable_names(&["flyctl"])[0].clone());
        std::fs::write(&expected, "fixture").unwrap();

        assert_eq!(
            find_executable_in(&["fly", "flyctl"], [first, second]),
            Some(expected)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
