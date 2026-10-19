use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

use crate::{
    cli::CliInvocation,
    models::{Account, AccountMode, ActivityStatus, ConnectionStatus, Provider},
};

use super::{azure, digitalocean, fly, gcloud, gitlab, heroku};

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
async fn heroku_invocation_and_release_contract_work_end_to_end() {
    let fake = FakeCli::new(
        "heroku",
        &[
            ("auth:whoami", "user@example.test"),
            (
                "apps --json",
                r#"[{"name":"example-app","archived_at":null}]"#,
            ),
            (
                "releases --app example-app --json --num 20",
                r#"[{"id":"00000000-0000-4000-8000-000000000001","version":7,"description":"Deploy example commit","status":"succeeded","created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:06:05Z","user":{"email":"user@example.test"}}]"#,
            ),
        ],
    );
    let account = account(Provider::Heroku);

    let discovery = heroku::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    let page = heroku::page(&fake.invocation, &discovery.targets[0], None)
        .await
        .expect("Heroku activity page");
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("releases --app example-app"));
}

#[tokio::test]
async fn fly_invocation_and_go_json_contract_work_end_to_end() {
    let fake = FakeCli::new(
        "flyctl",
        &[
            ("auth whoami", "user@example.test"),
            (
                "apps list --json",
                r#"[{"ID":"example-app","Name":"example-app","Organization":{"ID":"org-1","Name":"Example Org","Slug":"example-org"}}]"#,
            ),
            (
                "releases --app example-app --json",
                r#"[{"ID":"rel_123","Version":12,"Status":"complete","Description":"Deploy image","CreatedAt":"2026-01-02T03:04:05Z","User":{"Email":"user@example.test"}}]"#,
            ),
        ],
    );
    let account = account(Provider::Fly);

    let discovery = fly::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    assert_eq!(
        discovery.targets[0].description.as_deref(),
        Some("Fly.io organization: example-org")
    );
    let page = fly::page(&fake.invocation, &discovery.targets[0], None)
        .await
        .expect("Fly.io activity page");
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("apps list --json"));
}

#[tokio::test]
async fn gitlab_invocation_and_rest_contracts_work_end_to_end() {
    let fake = FakeCli::new(
        "glab",
        &[
            (
                "api --hostname gitlab.com user",
                r#"{"id":1,"username":"example-user"}"#,
            ),
            (
                "api --hostname gitlab.com projects?membership=true&archived=false&simple=true&order_by=last_activity_at&sort=desc&per_page=100&page=1",
                r#"[{"id":42,"name":"rocket","path_with_namespace":"example-org/rocket","description":"Example project"}]"#,
            ),
            (
                "api --hostname gitlab.com projects/42/deployments?order_by=created_at&sort=desc&per_page=20&page=1",
                r#"[{"id":41,"status":"success","ref":"main","sha":"abc123","created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:06:05Z","user":{"username":"example-user"},"environment":{"name":"production"},"deployable":{"name":"deploy-production","finished_at":"2026-01-02T03:06:00Z","commit":{"title":"Ship rocket"},"pipeline":{"id":36}}}]"#,
            ),
            (
                "api --hostname gitlab.com projects/42/pipelines?order_by=updated_at&sort=desc&per_page=20&page=1",
                r#"[{"id":36,"status":"success","ref":"main","created_at":"2026-01-02T03:03:00Z","updated_at":"2026-01-02T03:06:00Z","name":"deploy-production"}]"#,
            ),
        ],
    );
    let account = account(Provider::Gitlab);

    let discovery = gitlab::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 2);
    let deployments = discovery
        .targets
        .iter()
        .find(|target| !target.metadata.contains_key("kind"))
        .expect("deployment target");
    let pipelines = discovery
        .targets
        .iter()
        .find(|target| {
            target
                .metadata
                .get("kind")
                .is_some_and(|kind| kind == "pipelines")
        })
        .expect("pipeline target");
    let deployment_page = gitlab::page(&fake.invocation, deployments, None)
        .await
        .expect("GitLab deployment page");
    let pipeline_page = gitlab::page(&fake.invocation, pipelines, None)
        .await
        .expect("GitLab pipeline page");
    assert_eq!(
        deployment_page.activities[0].status,
        ActivityStatus::Success
    );
    assert_eq!(pipeline_page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("projects/42/deployments"));
}

#[tokio::test]
async fn digitalocean_invocation_and_app_platform_contract_work_end_to_end() {
    let app_id = "00000000-0000-4000-8000-000000000001";
    let deployment_id = "00000000-0000-4000-8000-000000000002";
    let fake = FakeCli::new(
        "doctl",
        &[
            (
                "account get --output json",
                r#"{"email":"user@example.test"}"#,
            ),
            (
                "apps list --output json",
                &format!(
                    r#"[{{"id":"{app_id}","spec":{{"name":"example-app"}},"default_ingress":"example-app.ondigitalocean.app"}}]"#
                ),
            ),
            (
                &format!("apps list-deployments {app_id} --output json"),
                &format!(
                    r#"[{{"id":"{deployment_id}","cause":"commit abc123","phase":"ACTIVE","created_at":"2026-01-02T03:04:05Z","phase_last_updated_at":"2026-01-02T03:06:05Z"}}]"#
                ),
            ),
        ],
    );
    let account = account(Provider::Digitalocean);

    let discovery = digitalocean::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    let page = digitalocean::page(&fake.invocation, &discovery.targets[0], None)
        .await
        .expect("DigitalOcean activity page");
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("apps list-deployments"));
}

#[tokio::test]
async fn gcloud_invocation_and_cloud_run_build_contracts_work_end_to_end() {
    let fake = FakeCli::new(
        "gcloud",
        &[
            (
                "auth list --filter=status:ACTIVE --format=json",
                r#"[{"account":"user@example.test","status":"ACTIVE"}]"#,
            ),
            (
                "projects list --filter=lifecycleState:ACTIVE --limit=25 --format=json",
                r#"[{"projectId":"example-project","name":"Example Project"}]"#,
            ),
            (
                "run services list --project=example-project --platform=managed --limit=200 --format=json",
                r#"[{"metadata":{"name":"web","labels":{"cloud.googleapis.com/location":"us-central1"},"annotations":{},"creationTimestamp":"2026-01-01T00:00:00Z"}}]"#,
            ),
            (
                "run revisions list --project=example-project --region=us-central1 --service=web --platform=managed --sort-by=~metadata.creationTimestamp --limit=20 --format=json",
                r#"[{"metadata":{"name":"web-00001-abc","labels":{},"annotations":{"serving.knative.dev/creator":"user@example.test"},"creationTimestamp":"2026-01-02T03:04:05Z"},"status":{"conditions":[{"type":"Ready","status":"True","lastTransitionTime":"2026-01-02T03:06:05Z"}]}}]"#,
            ),
            (
                "builds list --project=example-project --region=global --sort-by=~createTime --limit=20 --format=json",
                r#"[{"id":"build-123","projectId":"example-project","status":"SUCCESS","createTime":"2026-01-02T03:04:05Z","startTime":"2026-01-02T03:04:10Z","finishTime":"2026-01-02T03:06:05Z","buildTriggerId":"trigger-1","substitutions":{"BRANCH_NAME":"main","TRIGGER_NAME":"deploy-production"}}]"#,
            ),
        ],
    );
    let account = account(Provider::Gcloud);

    let discovery = gcloud::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 2);
    for target in &discovery.targets {
        let page = gcloud::page(&fake.invocation, target, None)
            .await
            .expect("Google Cloud activity page");
        assert_eq!(page.activities[0].status, ActivityStatus::Success);
    }
    assert!(fake.calls().contains("run revisions list"));
    assert!(fake.calls().contains("builds list"));
}

#[tokio::test]
async fn azure_invocation_and_app_service_contract_work_end_to_end() {
    let subscription = "00000000-0000-4000-8000-000000000001";
    let resource_id = format!(
        "/subscriptions/{subscription}/resourceGroups/example-rg/providers/Microsoft.Web/sites/example-app"
    );
    let fake = FakeCli::new(
        "az",
        &[
            (
                "account show --output json --only-show-errors",
                &format!(
                    r#"{{"id":"{subscription}","name":"Example Subscription","state":"Enabled","user":{{"name":"user@example.test"}},"environmentName":"AzureCloud"}}"#
                ),
            ),
            (
                "account list --output json --only-show-errors",
                &format!(
                    r#"[{{"id":"{subscription}","name":"Example Subscription","state":"Enabled","user":{{"name":"user@example.test"}},"environmentName":"AzureCloud"}}]"#
                ),
            ),
            (
                &format!("webapp list --subscription={subscription} --output=json --only-show-errors"),
                &format!(
                    r#"[{{"id":"{resource_id}","name":"example-app","resourceGroup":"example-rg","defaultHostName":"example-app.azurewebsites.net"}}]"#
                ),
            ),
            (
                &format!("webapp log deployment list --subscription={subscription} --resource-group=example-rg --name=example-app --output=json --only-show-errors"),
                r#"[{"id":"deployment-123","status":4,"status_text":"success","author":"Example User","deployer":"ZipDeploy","message":"Deploy archive","received_time":"2026-01-02T03:04:05Z","start_time":"2026-01-02T03:04:10Z","end_time":"2026-01-02T03:06:05Z"}]"#,
            ),
        ],
    );
    let account = account(Provider::Azure);

    let discovery = azure::discover(Some(&fake.invocation), &account).await;
    assert_eq!(discovery.connection.status, ConnectionStatus::Connected);
    assert_eq!(discovery.targets.len(), 1);
    let page = azure::page(&fake.invocation, &discovery.targets[0], None)
        .await
        .expect("Azure activity page");
    assert_eq!(page.activities[0].status, ActivityStatus::Success);
    assert!(fake.calls().contains("webapp log deployment list"));
}
