use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

use crate::{
    cli::CliInvocation,
    models::{Account, AccountMode, ActivityStatus, ConnectionStatus, Provider},
};

use super::{netlify, railway, render};

struct FakeCli {
    invocation: CliInvocation,
    root: PathBuf,
    calls: PathBuf,
}

impl FakeCli {
    fn new(name: &str, cases: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ratelline-{name}-provider-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create fake CLI directory");
        let script = root.join(name);
        let calls = root.join("calls");
        let branches = cases
            .iter()
            .map(|(args, output)| format!("  '{args}') printf '%s\\n' '{output}' ;;"))
            .collect::<Vec<_>>()
            .join("\n");
        let source = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'
case \"$*\" in
  '--version') printf '%s\\n' 'fixture 1.0.0' ;;
{branches}
  *) printf '%s\\n' \"unexpected fake CLI invocation: $*\" >&2; exit 64 ;;
esac\n",
            calls.display()
        );
        fs::write(&script, source).expect("write fake CLI");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .expect("make fake CLI executable");
        Self {
            invocation: CliInvocation::for_test(script),
            root,
            calls,
        }
    }

    fn calls(&self) -> String {
        fs::read_to_string(&self.calls).expect("read fake CLI calls")
    }
}

impl Drop for FakeCli {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn account(provider: Provider) -> Account {
    Account {
        id: format!("{}:local", provider.as_str()),
        provider,
        label: format!("{} local", provider.as_str()),
        mode: AccountMode::Local,
        profile: None,
        cli_username: None,
    }
}

#[tokio::test]
async fn railway_invocation_and_current_json_contract_work_end_to_end() {
    let fake = FakeCli::new(
        "railway",
        &[
            (
                "whoami --json",
                r#"{"name":"Example User","email":"user@example.test","workspaces":[{"id":"workspace-1","name":"Example Org"}]}"#,
            ),
            (
                "project list --json",
                r#"[{"workspace":{"id":"workspace-1","name":"Example Org"},"isFavorite":false,"id":"project-1","name":"Example Project","environments":{"edges":[{"node":{"id":"environment-1","name":"production","canAccess":true}}]}}]"#,
            ),
            (
                "service list --project project-1 --environment environment-1 --json",
                r#"[{"id":"service-1","name":"api","source":{"repo":"example-org/api","image":null}}]"#,
            ),
            (
                "deployment list --project project-1 --environment environment-1 --service service-1 --limit 21 --json",
                r#"[{"id":"deployment-1","status":"SUCCESS","createdAt":"2026-01-02T03:04:05Z","meta":{"branch":"main","commitMessage":"Ship example"}}]"#,
            ),
        ],
    );
    let account = account(Provider::Railway);

    let discovery = railway::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    let page = railway::page(&fake.invocation, &discovery.targets[0], None)
        .await
        .expect("Railway activity page");
    assert_eq!(page.activities.len(), 1);
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("deployment list --project project-1"));
}

#[tokio::test]
async fn netlify_invocation_and_open_api_contract_work_end_to_end() {
    let fake = FakeCli::new(
        "netlify",
        &[
            (
                "api getCurrentUser",
                r#"{"id":"user-1","full_name":"Example User","email":"user@example.test"}"#,
            ),
            (
                r#"api listSites --data {"filter":"all","page":1,"per_page":100}"#,
                r#"[{"id":"site-1","name":"example-site","account_name":"Example Team","build_settings":{"repo_url":"https://example.test/example/site","repo_branch":"main"}}]"#,
            ),
            (
                r#"api listSiteDeploys --data {"site_id":"site-1","page":1,"per_page":20}"#,
                r#"[{"id":"deploy-1","user_id":"user-1","state":"ready","name":"example-site","branch":"main","created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:06:05Z","published_at":"2026-01-02T03:06:00Z","title":"Ship example","context":"production"}]"#,
            ),
        ],
    );
    let account = account(Provider::Netlify);

    let discovery = netlify::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    let page = netlify::page(
        &fake.invocation,
        &discovery.targets[0],
        None,
        Some("user-1"),
    )
    .await
    .expect("Netlify activity page");
    assert_eq!(page.activities.len(), 1);
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("api listSiteDeploys --data"));
}

#[tokio::test]
async fn render_v2_28_invocation_and_flat_deploy_contract_work_end_to_end() {
    let fake = FakeCli::new(
        "render",
        &[
            (
                "whoami --output json --confirm",
                r#"{"id":"user-1","name":"Example User","email":"user@example.test"}"#,
            ),
            (
                "workspace current --output json --confirm",
                r#"{"id":"workspace-1","name":"Example Org"}"#,
            ),
            (
                "workspaces --output json --confirm",
                r#"[{"id":"workspace-1","name":"Example Org"}]"#,
            ),
            (
                "services --output json --confirm",
                r#"[{"service":{"id":"srv-example","name":"api","ownerId":"workspace-1","type":"web_service","branch":"main","dashboardUrl":"https://dashboard.render.com/web/srv-example"},"project":{"id":"project-1","name":"Example Project"},"environment":{"id":"environment-1","name":"Production"}}]"#,
            ),
            (
                "deploys list srv-example --output json --confirm",
                r#"[{"id":"dep-example","commit":{"id":"abcdef123456","message":"Ship example"},"createdAt":"2026-01-02T03:04:05Z","startedAt":"2026-01-02T03:04:10Z","finishedAt":"2026-01-02T03:06:05Z","status":"live","updatedAt":"2026-01-02T03:06:05Z"}]"#,
            ),
        ],
    );
    let account = account(Provider::Render);

    let discovery = render::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    let page = render::page(&fake.invocation, &discovery.targets[0], None)
        .await
        .expect("Render activity page");
    assert_eq!(page.activities.len(), 1);
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("deploys list srv-example"));
}
