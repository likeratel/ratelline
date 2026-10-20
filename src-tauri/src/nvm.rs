//! nvm installs CLIs under per-version directories that GUI launches never see,
//! because the app does not inherit the login shell's `NVM_BIN` or `PATH`.

use std::path::{Component, Path, PathBuf};

/// Alias files can point at other aliases (`default` -> `lts/iron` -> `v20.x`).
const MAX_ALIAS_HOPS: usize = 4;

type Version = (u32, u32, u32);

/// Returns the `bin` directories of installed nvm Node versions: the `default`
/// alias first, then every other version from newest to oldest.
pub fn bin_directories(nvm_dir: &Path) -> Vec<PathBuf> {
    let installed = installed_versions(&nvm_dir.join("versions/node"));
    let default = resolve_default(nvm_dir, &installed);
    default
        .iter()
        .chain(
            installed
                .iter()
                .filter(|entry| Some(*entry) != default.as_ref()),
        )
        .map(|(_, path)| path.join("bin"))
        .collect()
}

fn installed_versions(root: &Path) -> Vec<(Version, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut versions: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let version = parse_version(entry.file_name().to_str()?)?;
            Some((version, entry.path()))
        })
        .collect();
    versions.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    versions
}

fn resolve_default(nvm_dir: &Path, installed: &[(Version, PathBuf)]) -> Option<(Version, PathBuf)> {
    let mut alias = String::from("default");
    for _ in 0..MAX_ALIAS_HOPS {
        let target = read_alias(nvm_dir, &alias)?;
        if let Some(found) = match_installed(&target, installed) {
            return Some(found.clone());
        }
        alias = target;
    }
    None
}

fn read_alias(nvm_dir: &Path, name: &str) -> Option<String> {
    let relative = Path::new(name);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let value = std::fs::read_to_string(nvm_dir.join("alias").join(relative)).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Matches `22.22.1`, `v22`, or `22.22` against installed versions, newest first.
/// `node` and `stable` mean the newest installed version.
fn match_installed<'a>(
    target: &str,
    installed: &'a [(Version, PathBuf)],
) -> Option<&'a (Version, PathBuf)> {
    if matches!(target, "node" | "stable") {
        return installed.first();
    }
    let prefix = parse_prefix(target)?;
    installed.iter().find(|(version, _)| {
        let parts = [version.0, version.1, version.2];
        parts.iter().zip(&prefix).all(|(left, right)| left == right)
    })
}

fn parse_prefix(value: &str) -> Option<Vec<u32>> {
    let parts = value
        .strip_prefix('v')
        .unwrap_or(value)
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (1..=3).contains(&parts.len()).then_some(parts)
}

fn parse_version(value: &str) -> Option<Version> {
    match parse_prefix(value)?.as_slice() {
        [major, minor, patch] if value.starts_with('v') => Some((*major, *minor, *patch)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nvm_fixture(versions: &[&str], aliases: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for version in versions {
            std::fs::create_dir_all(root.path().join("versions/node").join(version).join("bin"))
                .unwrap();
        }
        for (name, target) in aliases {
            let path = root.path().join("alias").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("{target}\n")).unwrap();
        }
        root
    }

    fn bins(root: &Path, versions: &[&str]) -> Vec<PathBuf> {
        versions
            .iter()
            .map(|version| root.join("versions/node").join(version).join("bin"))
            .collect()
    }

    #[test]
    fn default_alias_comes_first_then_newest_versions() {
        let root = nvm_fixture(
            &["v20.19.5", "v22.22.1", "v24.11.0", "v9.0.0"],
            &[("default", "22.22.1")],
        );

        assert_eq!(
            bin_directories(root.path()),
            bins(root.path(), &["v22.22.1", "v24.11.0", "v20.19.5", "v9.0.0"])
        );
    }

    #[test]
    fn partial_and_chained_default_aliases_pick_newest_match() {
        let root = nvm_fixture(
            &["v20.10.0", "v20.19.5", "v22.22.1"],
            &[("default", "lts/iron"), ("lts/iron", "v20")],
        );

        assert_eq!(
            bin_directories(root.path()),
            bins(root.path(), &["v20.19.5", "v22.22.1", "v20.10.0"])
        );
    }

    #[test]
    fn unresolvable_or_escaping_default_falls_back_to_newest_first() {
        let root = nvm_fixture(
            &["v20.19.5", "v22.22.1"],
            &[("default", "../versions/node/v20.19.5")],
        );

        assert_eq!(
            bin_directories(root.path()),
            bins(root.path(), &["v22.22.1", "v20.19.5"])
        );
    }

    #[test]
    fn missing_nvm_directory_yields_nothing() {
        let root = tempfile::tempdir().unwrap();

        assert!(bin_directories(&root.path().join("absent")).is_empty());
    }
}
