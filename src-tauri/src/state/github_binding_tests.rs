use super::*;
use crate::{cli::CliInvocation, models::ConnectionStatus, settings::SettingsStore};

fn access(login: &str, can_access_all: bool) -> GithubAccountAccess {
    GithubAccountAccess {
        login: login.into(),
        active: false,
        state: GithubCliAccountState::Ok,
        can_access_all,
        inaccessible_targets: Vec::new(),
        check_failed: None,
        identity_mismatch: false,
        used_by_connection_id: None,
    }
}

#[test]
fn only_one_conclusive_unused_match_is_pinned() {
    assert_eq!(
        pin_decision(&[access("orbit-developer", true), access("orbit-work", false)]),
        PinDecision::Pin("orbit-developer".into())
    );
    assert_eq!(
        pin_decision(&[
            access("orbit-developer", false),
            access("orbit-work", false)
        ]),
        PinDecision::NoCandidate
    );
    assert_eq!(pin_decision(&[]), PinDecision::NoCandidate);
    assert_eq!(
        pin_decision(&[access("orbit-developer", true), access("orbit-work", true)]),
        PinDecision::Ambiguous
    );
    let failed = GithubAccountAccess {
        check_failed: Some(IssueKind::Network),
        ..access("orbit-work", false)
    };
    assert_eq!(
        pin_decision(&[access("orbit-developer", true), failed]),
        PinDecision::Undetermined
    );
    let used = GithubAccountAccess {
        used_by_connection_id: Some("github-00000000-0000-0000-0000-000000000001".into()),
        ..access("orbit-developer", true)
    };
    assert_eq!(pin_decision(&[used]), PinDecision::InUse);
}

#[cfg(unix)]
struct FakeGh {
    _root: tempfile::TempDir,
    script: std::path::PathBuf,
}

#[cfg(unix)]
impl FakeGh {
    /// `accounts` is the `auth status` JSON. Each `access-<login>` file lists the
    /// repositories that login can read; `fail-<login>` makes its API calls fail
    /// with a network error.
    fn start(accounts: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$0.calls"
case "$1 $2" in
  "auth status") cat "$0.accounts"; exit 0;;
  "auth token") printf 'token-%s\n' "$6"; exit 0;;
  "--version "*) printf 'gh version 2.90.0\n'; exit 0;;
esac
login=${GH_TOKEN#token-}
if [ -z "$GH_TOKEN" ]; then printf 'unbound api call\n' >> "$0.unbound"; exit 3; fi
printf '%s %s\n' "$GH_TOKEN" "$2" >> "$0.api"
if [ -f "$0.fail-$login" ]; then printf 'dial tcp: lookup api.github.com: no such host\n' >&2; exit 1; fi
case "$2" in
  user) case "$login" in orbit-developer) id=101;; orbit-work) id=202;; *) id=303;; esac
        printf '{"id":%s,"login":"%s"}' "$id" "$login";;
  repos/*/actions/runs*) printf '{"total_count":0,"workflow_runs":[]}';;
  repos/*) repo=${2#repos/}
        if grep -qx "$repo" "$0.access-$login" 2>/dev/null; then printf '{}'; else printf '{"message":"Not Found","status":"404"}gh: Not Found (HTTP 404)\n' >&2; exit 1; fi;;
  *) printf '{}';;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(script.with_extension("accounts"), accounts).unwrap();
        Self {
            _root: root,
            script,
        }
    }

    fn grant(&self, login: &str, repositories: &[&str]) {
        std::fs::write(
            self.script.with_extension(format!("access-{login}")),
            repositories.join("\n") + "\n",
        )
        .unwrap();
    }

    fn fail(&self, login: &str) {
        std::fs::write(self.script.with_extension(format!("fail-{login}")), "").unwrap();
    }

    fn api_calls(&self) -> String {
        std::fs::read_to_string(self.script.with_extension("api")).unwrap_or_default()
    }

    fn status_calls(&self) -> usize {
        self.calls()
            .lines()
            .filter(|line| line.starts_with("auth status"))
            .count()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.script.with_extension("calls")).unwrap_or_default()
    }

    fn state(&self, settings: Settings) -> AppState {
        let directory = self.script.parent().unwrap().join("app");
        AppState {
            tools: Toolchain::github_for_test(
                &directory,
                CliInvocation::for_test(self.script.clone()),
            ),
            store: SettingsStore::new(directory),
            snapshot: RwLock::new(Snapshot::initial(settings)),
            refresh_lock: Mutex::new(()),
            refresh_wake: refresh_wake::RefreshWake::default(),
            publish_lock: Mutex::new(()),
            settings_revision: AtomicU64::new(0),
            refresh_epoch: AtomicU64::new(0),
            history_lock: Mutex::new(()),
            history_cache: Mutex::new(HistoryCache::default()),
            notification_engine: Mutex::new(NotificationEngine::default()),
            login_statuses: Mutex::new(HashMap::new()),
            github_pin_attempts: Mutex::new(HashMap::new()),
        }
    }
}

const TWO_ACCOUNTS: &str = r#"[{"login":"orbit-developer","state":"success","active":false},{"login":"orbit-work","state":"success","active":true}]"#;

fn settings_with_repository(repository: &str) -> Settings {
    Settings {
        targets: vec![Target {
            id: repository.into(),
            provider: Provider::Github,
            connection_id: LOCAL_GITHUB_ID.into(),
            name: repository.into(),
            description: None,
            metadata: HashMap::new(),
        }],
        ..Settings::default()
    }
}

#[cfg(unix)]
async fn local_github(state: &AppState) -> Account {
    state
        .account(LOCAL_GITHUB_ID, Provider::Github)
        .await
        .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn single_account_with_access_is_pinned_without_switching_gh() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    gh.grant("orbit-developer", &["sample/app"]);
    let state = gh.state(settings_with_repository("sample/app"));

    state.auto_pin_github_connections(&NoopPublisher).await;

    let account = local_github(&state).await;
    assert_eq!(account.cli_username.as_deref(), Some("orbit-developer"));
    assert_eq!(account.profile.as_deref(), Some("101"));
    assert_eq!(
        account.id, LOCAL_GITHUB_ID,
        "the connection ID must not change"
    );
    let calls = gh.calls();
    assert!(!calls.contains("auth switch"));
    assert!(!calls.contains("auth login"));
    assert!(!gh.script.with_extension("unbound").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn ambiguous_missing_or_inconclusive_access_is_not_pinned() {
    for scenario in ["both", "none", "network", "in-use"] {
        let gh = FakeGh::start(TWO_ACCOUNTS);
        let mut settings = settings_with_repository("sample/app");
        match scenario {
            "both" => {
                gh.grant("orbit-developer", &["sample/app"]);
                gh.grant("orbit-work", &["sample/app"]);
            }
            "network" => {
                gh.grant("orbit-developer", &["sample/app"]);
                gh.fail("orbit-work");
            }
            "in-use" => {
                gh.grant("orbit-developer", &["sample/app"]);
                settings.accounts.push(Account {
                    id: "github-00000000-0000-0000-0000-000000000001".into(),
                    provider: Provider::Github,
                    label: "Work".into(),
                    mode: crate::models::AccountMode::Isolated,
                    profile: Some("101".into()),
                    cli_username: None,
                });
            }
            _ => {}
        }
        let state = gh.state(settings);
        state.auto_pin_github_connections(&NoopPublisher).await;
        assert_eq!(
            local_github(&state).await.cli_username,
            None,
            "{scenario} must leave the connection unbound"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn diagnosis_lists_inaccessible_repositories_per_account() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    gh.grant("orbit-developer", &["sample/app", "sample/site"]);
    gh.grant("orbit-work", &["sample/site"]);
    let mut settings = settings_with_repository("sample/app");
    settings.targets.push(Target {
        id: "sample/site".into(),
        ..settings.targets[0].clone()
    });
    let state = gh.state(settings);

    let results = state
        .diagnose_github_connection(LOCAL_GITHUB_ID)
        .await
        .unwrap();
    let work = results
        .iter()
        .find(|item| item.login == "orbit-work")
        .unwrap();
    assert!(!work.can_access_all);
    assert_eq!(work.inaccessible_targets, vec!["sample/app".to_owned()]);
    assert!(work.active);
    let developer = results
        .iter()
        .find(|item| item.login == "orbit-developer")
        .unwrap();
    assert!(developer.can_access_all);
    assert!(state
        .diagnose_github_connection("github:local;rm -rf")
        .await
        .is_err());
    assert!(state
        .diagnose_github_connection(crate::models::LOCAL_VERCEL_ID)
        .await
        .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn unbound_connection_reports_needs_account_without_using_the_active_account() {
    let gh = FakeGh::start("[]");
    let state = gh.state(settings_with_repository("sample/app"));

    let done = state.refresh(&NoopPublisher).await;

    let connection = done
        .connections
        .iter()
        .find(|connection| connection.connection_id == LOCAL_GITHUB_ID)
        .unwrap();
    assert_eq!(connection.status, ConnectionStatus::Error);
    assert_eq!(connection.kind, Some(IssueKind::NeedsAccount));
    let issues: Vec<_> = done
        .issues
        .iter()
        .filter(|issue| issue.connection_id.as_deref() == Some(LOCAL_GITHUB_ID))
        .collect();
    assert_eq!(issues.len(), 1, "one issue per connection: {issues:?}");
    assert_eq!(issues[0].kind, IssueKind::NeedsAccount);
    assert!(!gh.calls().lines().any(|line| line.starts_with("api ")));
}

#[cfg(unix)]
#[tokio::test]
async fn refresh_pins_then_reads_with_the_pinned_account() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    gh.grant("orbit-work", &["sample/app"]);
    let state = gh.state(settings_with_repository("sample/app"));

    let done = state.refresh(&NoopPublisher).await;

    let connection = done
        .connections
        .iter()
        .find(|connection| connection.connection_id == LOCAL_GITHUB_ID)
        .unwrap();
    assert_eq!(connection.status, ConnectionStatus::Connected);
    assert_eq!(connection.account.as_deref(), Some("orbit-work"));
    assert!(done.issues.is_empty(), "{:?}", done.issues);
    assert!(!gh.script.with_extension("unbound").exists());
}

#[cfg(unix)]
fn bound_to_work(mut settings: Settings) -> Settings {
    settings.accounts = settings
        .accounts
        .into_iter()
        .map(|account| {
            if account.id == LOCAL_GITHUB_ID {
                Account {
                    profile: Some("202".into()),
                    cli_username: Some("orbit-work".into()),
                    ..account
                }
            } else {
                account
            }
        })
        .collect();
    settings
}

#[cfg(unix)]
#[tokio::test]
async fn another_github_user_replaces_the_identity_only_with_explicit_consent() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    gh.grant("orbit-developer", &["sample/app"]);
    let state = gh.state(bound_to_work(settings_with_repository("sample/app")));
    let cached = crate::models::Activity {
        id: "github:1:1".into(),
        provider: Provider::Github,
        connection_id: LOCAL_GITHUB_ID.into(),
        target_id: "sample/app".into(),
        kind: "push".into(),
        title: "Deploy".into(),
        workflow_name: None,
        project: "sample/app".into(),
        branch: None,
        url: "https://github.com/sample/app/actions/runs/1".into(),
        status: crate::models::ActivityStatus::Success,
        raw_status: "success".into(),
        actor: Some("orbit-work".into()),
        ownership: crate::models::Ownership::Mine,
        started_at: "2026-10-01T00:00:00Z".into(),
        finished_at: None,
        updated_at: "2026-10-01T00:00:00Z".into(),
    };
    state.snapshot.write().await.activities = vec![cached];

    let refused = state
        .connect_github_cli_account(
            Provider::Github,
            LOCAL_GITHUB_ID,
            "orbit-developer",
            false,
            &NoopPublisher,
        )
        .await;
    assert!(refused.is_err());
    let account = local_github(&state).await;
    assert_eq!(account.cli_username.as_deref(), Some("orbit-work"));
    assert_eq!(state.snapshot().await.activities.len(), 1);

    let rebound = state
        .connect_github_cli_account(
            Provider::Github,
            LOCAL_GITHUB_ID,
            "orbit-developer",
            true,
            &NoopPublisher,
        )
        .await
        .expect("explicit rebind");
    assert_eq!(rebound.id, LOCAL_GITHUB_ID);
    assert_eq!(rebound.profile.as_deref(), Some("101"));
    assert_eq!(rebound.cli_username.as_deref(), Some("orbit-developer"));
    assert!(
        state.snapshot().await.activities.is_empty(),
        "ownership from the previous identity must not be reused"
    );
    let snapshot = state.snapshot().await;
    let key = snapshot.settings.targets[0].key();
    assert!(
        snapshot.pending_targets.contains(&key),
        "the target is reported as being checked, not as an empty success"
    );
    assert_eq!(
        snapshot
            .query_health
            .iter()
            .find(|health| health.target_id == "sample/app")
            .map(|health| health.status),
        Some(crate::models::QueryHealthStatus::Checking)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn explicit_identity_change_still_rejects_an_account_used_elsewhere() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    let mut settings = bound_to_work(settings_with_repository("sample/app"));
    settings.accounts.push(Account {
        id: "github-00000000-0000-0000-0000-000000000001".into(),
        provider: Provider::Github,
        label: "Other".into(),
        mode: crate::models::AccountMode::Isolated,
        profile: Some("101".into()),
        cli_username: Some("orbit-developer".into()),
    });
    let state = gh.state(settings);
    assert!(state
        .connect_github_cli_account(
            Provider::Github,
            LOCAL_GITHUB_ID,
            "orbit-developer",
            true,
            &NoopPublisher,
        )
        .await
        .is_err());
    assert_eq!(
        local_github(&state).await.cli_username.as_deref(),
        Some("orbit-work")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn automatic_binding_never_changes_a_saved_identity() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    gh.grant("orbit-developer", &["sample/app"]);
    let mut settings = settings_with_repository("sample/app");
    settings.accounts = settings
        .accounts
        .into_iter()
        .map(|account| {
            if account.id == LOCAL_GITHUB_ID {
                Account {
                    profile: Some("202".into()),
                    ..account
                }
            } else {
                account
            }
        })
        .collect();
    let state = gh.state(settings);
    state.auto_pin_github_connections(&NoopPublisher).await;
    let account = local_github(&state).await;
    assert_eq!(account.cli_username, None);
    assert_eq!(account.profile.as_deref(), Some("202"));
    assert!(
        !gh.api_calls()
            .contains("token-orbit-developer repos/sample/app"),
        "automatic binding must not read repositories as another GitHub user"
    );

    let results = state
        .diagnose_github_connection(LOCAL_GITHUB_ID)
        .await
        .unwrap();
    assert!(gh
        .api_calls()
        .contains("token-orbit-developer repos/sample/app"));
    let other = results
        .iter()
        .find(|item| item.login == "orbit-developer")
        .unwrap();
    assert!(other.identity_mismatch);
    assert!(other.inaccessible_targets.is_empty());
    assert!(!other.can_access_all);
}

#[test]
fn failed_attempts_back_off_to_fifteen_minutes() {
    let minutes = |attempt: &PinAttempt| attempt.retry_after.as_secs() / 60;
    let mut previous: Option<PinAttempt> = None;
    let mut delays = Vec::new();
    for _ in 0..5 {
        let next = failed_attempt(previous.as_ref(), "same".into(), false);
        delays.push(minutes(&next));
        previous = Some(next);
    }
    assert_eq!(delays, vec![1, 2, 4, 15, 15]);
    let changed = failed_attempt(previous.as_ref(), "changed".into(), false);
    assert_eq!(minutes(&changed), 1, "new inputs restart the backoff");
    assert_eq!(minutes(&failed_attempt(None, "x".into(), true)), 15);
}

#[cfg(unix)]
#[tokio::test]
async fn inconclusive_or_failed_listing_is_not_retried_every_cycle() {
    let gh = FakeGh::start(TWO_ACCOUNTS);
    gh.grant("orbit-developer", &["sample/app"]);
    gh.fail("orbit-work");
    let state = gh.state(settings_with_repository("sample/app"));
    state.auto_pin_github_connections(&NoopPublisher).await;
    let api_after_first = gh.api_calls().lines().count();
    assert_eq!(gh.status_calls(), 1);
    state.auto_pin_github_connections(&NoopPublisher).await;
    state.forget_github_pin_attempts().await;
    state.auto_pin_github_connections(&NoopPublisher).await;
    assert_eq!(
        gh.status_calls(),
        1,
        "backoff and the 30-second floor apply"
    );
    assert_eq!(gh.api_calls().lines().count(), api_after_first);
    assert_eq!(local_github(&state).await.cli_username, None);

    let broken = FakeGh::start("not json");
    let state = broken.state(settings_with_repository("sample/app"));
    state.auto_pin_github_connections(&NoopPublisher).await;
    state.auto_pin_github_connections(&NoopPublisher).await;
    assert_eq!(
        broken.status_calls(),
        1,
        "a failed account listing is retried only after its backoff"
    );
}
