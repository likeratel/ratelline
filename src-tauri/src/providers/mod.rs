pub mod aws;
pub mod bitbucket;
pub mod github;
pub(crate) mod github_access;
pub mod vercel;

pub mod catalog;
pub mod cloud;
pub mod github_deployments;
pub mod hosting;
pub mod mobile;

/// Keep absent/blank execution names absent rather than inventing a workflow label.
fn execution_name(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Groups per-status probe failures by cause so a target reports each cause once
/// instead of once per queried status.
pub(crate) fn grouped_probe_failures<'a>(
    failures: &[(&'a str, crate::cli::CliError)],
) -> Vec<(crate::cli::CliError, Vec<&'a str>)> {
    let mut groups: Vec<(crate::cli::CliError, Vec<&'a str>)> = Vec::new();
    for (status, error) in failures {
        match groups
            .iter_mut()
            .find(|(known, _)| known.issue_kind() == error.issue_kind())
        {
            Some((_, statuses)) => statuses.push(status),
            None => groups.push((*error, vec![*status])),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::execution_name;

    #[test]
    fn probe_failures_with_one_cause_become_one_group() {
        use crate::cli::CliError;
        let groups = super::grouped_probe_failures(&[
            ("queued", CliError::NoAccess),
            ("in_progress", CliError::NoAccess),
            ("waiting", CliError::TimedOut),
            ("pending", CliError::NoAccess),
        ]);
        assert_eq!(
            groups,
            vec![
                (CliError::NoAccess, vec!["queued", "in_progress", "pending"]),
                (CliError::TimedOut, vec!["waiting"]),
            ]
        );
    }
    #[test]
    fn missing_execution_names_are_not_invented() {
        assert_eq!(execution_name(None), None);
        assert_eq!(execution_name(Some(" \n\t ")), None);
        assert_eq!(
            execution_name(Some("  deploy-production  ")),
            Some("deploy-production".into())
        );
    }
}
pub(crate) mod api;

pub(crate) mod http;
