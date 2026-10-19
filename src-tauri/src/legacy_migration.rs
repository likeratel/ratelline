//! One-time migration from the former `crowline` product identity.
//!
//! The application identifier changed from `io.crowline.desktop` to
//! `io.ratelline.desktop`, which moves the app data directory. On startup the
//! old directory is copied (never moved or deleted) when the new one is empty.

use std::path::{Path, PathBuf};

/// Directory name used by releases published before the rename.
pub const LEGACY_APP_IDENTIFIER: &str = "io.crowline.desktop";

#[derive(Debug, Default, Eq, PartialEq)]
pub struct MigrationReport {
    pub copied_files: usize,
    pub failures: Vec<String>,
}

/// Returns the legacy sibling directory for an app data directory.
pub fn legacy_sibling(app_data_dir: &Path) -> Option<PathBuf> {
    let parent = app_data_dir.parent()?;
    let legacy = parent.join(LEGACY_APP_IDENTIFIER);
    (legacy != app_data_dir).then_some(legacy)
}

/// Copies files from `legacy_dir` into `target_dir` when the target is missing
/// or empty and the legacy directory exists. Returns `None` when no migration
/// was needed. Per-entry failures are collected and do not stop the copy.
pub fn copy_legacy_data(legacy_dir: &Path, target_dir: &Path) -> Option<MigrationReport> {
    if !legacy_dir.is_dir() || !is_missing_or_empty(target_dir) {
        return None;
    }
    let mut report = MigrationReport::default();
    if let Err(error) = std::fs::create_dir_all(target_dir) {
        report
            .failures
            .push(format!("create {}: {error}", target_dir.display()));
        return Some(report);
    }
    copy_directory(legacy_dir, target_dir, &mut report);
    Some(report)
}

/// Runs the migration for the app data directory and logs the outcome.
pub fn migrate_app_data_dir(app_data_dir: &Path) {
    let Some(legacy) = legacy_sibling(app_data_dir) else {
        return;
    };
    let Some(report) = copy_legacy_data(&legacy, app_data_dir) else {
        return;
    };
    eprintln!(
        "Copied {} file(s) from the previous app data directory.",
        report.copied_files
    );
    for failure in &report.failures {
        eprintln!("Failed to copy previous app data: {failure}");
    }
}

fn is_missing_or_empty(directory: &Path) -> bool {
    match std::fs::read_dir(directory) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn copy_directory(source: &Path, target: &Path, report: &mut MigrationReport) {
    let entries = match std::fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => {
            report
                .failures
                .push(format!("read {}: {error}", source.display()));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => copy_entry(&entry, target, report),
            Err(error) => report
                .failures
                .push(format!("read {}: {error}", source.display())),
        }
    }
}

fn copy_entry(entry: &std::fs::DirEntry, target: &Path, report: &mut MigrationReport) {
    let source = entry.path();
    let destination = target.join(entry.file_name());
    // `DirEntry::file_type` does not follow symlinks, so links are skipped.
    let result = match entry.file_type() {
        Ok(kind) if kind.is_dir() => std::fs::create_dir_all(&destination)
            .map(|()| copy_directory(&source, &destination, report)),
        Ok(kind) if kind.is_file() => std::fs::copy(&source, &destination).map(|_| {
            report.copied_files += 1;
        }),
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        report
            .failures
            .push(format!("copy {}: {error}", source.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_APP_IDENTIFIER);
        let target = root.path().join("io.ratelline.desktop");
        std::fs::create_dir_all(legacy.join("accounts/vercel")).unwrap();
        std::fs::write(legacy.join("settings.json"), r#"{"version":1}"#).unwrap();
        std::fs::write(legacy.join("accounts/vercel/auth.json"), "{}").unwrap();
        (root, legacy, target)
    }

    #[test]
    fn legacy_sibling_uses_parent_directory() {
        let target = Path::new("/support/io.ratelline.desktop");
        assert_eq!(
            legacy_sibling(target),
            Some(PathBuf::from("/support").join(LEGACY_APP_IDENTIFIER))
        );
        assert_eq!(
            legacy_sibling(&Path::new("/support").join(LEGACY_APP_IDENTIFIER)),
            None
        );
    }

    #[test]
    fn copies_into_missing_target_and_keeps_legacy() {
        let (_root, legacy, target) = layout();
        let report = copy_legacy_data(&legacy, &target).unwrap();
        assert_eq!(report.copied_files, 2);
        assert!(report.failures.is_empty());
        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).unwrap(),
            r#"{"version":1}"#
        );
        assert!(target.join("accounts/vercel/auth.json").is_file());
        assert!(legacy.join("settings.json").is_file());
    }

    #[test]
    fn copies_into_empty_target() {
        let (_root, legacy, target) = layout();
        std::fs::create_dir_all(&target).unwrap();
        let report = copy_legacy_data(&legacy, &target).unwrap();
        assert_eq!(report.copied_files, 2);
    }

    #[test]
    fn skips_when_target_has_data() {
        let (_root, legacy, target) = layout();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("settings.json"), "new").unwrap();
        assert_eq!(copy_legacy_data(&legacy, &target), None);
        assert_eq!(
            std::fs::read_to_string(target.join("settings.json")).unwrap(),
            "new"
        );
    }

    #[test]
    fn skips_when_legacy_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("io.ratelline.desktop");
        assert_eq!(
            copy_legacy_data(&root.path().join(LEGACY_APP_IDENTIFIER), &target),
            None
        );
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn records_failures_and_continues() {
        use std::os::unix::fs::PermissionsExt;
        let (_root, legacy, target) = layout();
        let unreadable = legacy.join("unreadable.json");
        std::fs::write(&unreadable, "{}").unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
        let report = copy_legacy_data(&legacy, &target).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o600)).unwrap();
        if report.failures.is_empty() {
            // Running with elevated privileges; permissions are not enforced.
            return;
        }
        assert_eq!(report.copied_files, 2);
        assert_eq!(report.failures.len(), 1);
    }
}
