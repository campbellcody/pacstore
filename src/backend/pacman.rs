use crate::{
    error::{AppError, Result},
    packages::{Catalog, Package},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    process::{Command, Output},
};

pub fn command(program: &str, args: &[&str]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("COLUMNS", "100000")
        .output()
        .map_err(|source| AppError::Io {
            operation: "Run read-only package query",
            source,
        })
}

pub fn checked(program: &str, args: &[&str]) -> Result<String> {
    let output = command(program, args)?;
    if !output.status.success() {
        return Err(AppError::Backend(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn configuration() -> Result<(String, String, Vec<String>)> {
    let configuration = checked("pacman-conf", &[])?;
    let dbpath = checked("pacman-conf", &["DBPath"])?.trim().to_owned();
    let repositories = checked("pacman-conf", &["--repo-list"])?
        .lines()
        .map(str::to_owned)
        .collect();
    Ok((configuration, dbpath, repositories))
}

// LC_ALL=C gives stable labels. Continuations start with whitespace; optional
// dependency descriptions retain their colons. Never parse localized output.
pub fn records(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut result = Vec::new();
    let mut record = BTreeMap::new();
    let mut key = String::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !record.is_empty() {
                result.push(std::mem::take(&mut record));
            }
            key.clear();
        } else if line.starts_with(char::is_whitespace) {
            if let Some(value) = record.get_mut(&key) {
                let value: &mut String = value;
                value.push('\n');
                value.push_str(line.trim());
            }
        } else if let Some((label, value)) = line.split_once(" : ") {
            key = label.trim().to_owned();
            record.insert(key.clone(), value.trim().to_owned());
        }
    }
    result
}

fn field(record: &BTreeMap<String, String>, key: &str) -> String {
    record
        .get(key)
        .filter(|s| s.as_str() != "None")
        .cloned()
        .unwrap_or_default()
}

fn package(record: &BTreeMap<String, String>) -> Option<Package> {
    let name = field(record, "Name");
    let version = field(record, "Version");
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some(Package {
        name,
        version,
        summary: field(record, "Description"),
        repository: field(record, "Repository"),
        architecture: field(record, "Architecture"),
        download_size: field(record, "Download Size"),
        installed_size: field(record, "Installed Size"),
        licenses: field(record, "Licenses"),
        upstream: field(record, "URL"),
        dependencies: field(record, "Depends On"),
        optional_dependencies: field(record, "Optional Deps"),
        ..Default::default()
    })
}

pub fn merge(sync: &str, local: &str, upgrades: &str) -> Vec<Package> {
    let local: HashMap<_, _> = records(local)
        .into_iter()
        .filter_map(|record| {
            let mut p = package(&record)?;
            p.installed = Some(p.version.clone());
            p.install_reason = field(&record, "Install Reason");
            Some((p.name.clone(), p))
        })
        .collect();
    let upgrades: HashSet<_> = upgrades
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let mut seen = HashSet::new();
    let mut packages: Vec<_> = records(sync)
        .iter()
        .filter_map(package)
        .map(|mut p| {
            p.alternate = !seen.insert(p.name.clone());
            if let Some(installed) = local.get(&p.name) {
                p.installed = installed.installed.clone();
                p.install_reason = installed.install_reason.clone();
            }
            p.update_available = upgrades.contains(p.name.as_str()) && !p.alternate;
            p
        })
        .collect();
    for (name, mut p) in local {
        if !seen.contains(&name) {
            p.repository = "local".into();
            packages.push(p);
        }
    }
    packages.sort_by(|a, b| (&a.name, &a.repository).cmp(&(&b.name, &b.repository)));
    packages
}

pub fn query(repositories: Vec<String>) -> Result<Catalog> {
    let mut warnings = Vec::new();
    let sync = command("pacman", &["-Si"])?;
    if !sync.status.success() || !sync.stderr.is_empty() {
        warnings.push(format!(
            "Repository query: {}. Browsing available local databases only.",
            String::from_utf8_lossy(&sync.stderr).trim()
        ));
    }
    let local = command("pacman", &["-Qi"])?;
    let local_empty =
        local.status.code() == Some(1) && local.stderr.is_empty() && local.stdout.is_empty();
    if !(local.status.success() || local_empty) {
        return Err(AppError::Backend(format!(
            "Installed package query failed: {}",
            String::from_utf8_lossy(&local.stderr)
        )));
    }
    let upgrades = command("pacman", &["-Qu"])?;
    let updates_known = (upgrades.status.success()
        || (upgrades.status.code() == Some(1)
            && upgrades.stdout.is_empty()
            && upgrades.stderr.is_empty()))
        && sync.status.success()
        && sync.stderr.is_empty();
    if !updates_known {
        warnings.push("Update availability is incomplete. Check pacman database errors; no databases were refreshed.".into());
    }
    let orphans = command("pacman", &["-Qdtq"])?;
    let orphans_known = orphans.status.success()
        || (orphans.status.code() == Some(1)
            && orphans.stdout.is_empty()
            && orphans.stderr.is_empty());
    if !orphans_known {
        warnings
            .push("Orphan detection failed. Reload after resolving pacman query errors.".into());
    }
    let orphan_names: HashSet<_> = if orphans_known {
        String::from_utf8_lossy(&orphans.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    } else {
        HashSet::new()
    };
    let mut packages = merge(
        &String::from_utf8_lossy(&sync.stdout),
        &String::from_utf8_lossy(&local.stdout),
        &String::from_utf8_lossy(&upgrades.stdout),
    );
    for package in &mut packages {
        package.orphan = package.installed_as_dependency() && orphan_names.contains(&package.name);
    }
    if packages.is_empty() && !sync.status.success() {
        return Err(AppError::Backend("No readable package databases. Initialize pacman's repository databases with your system package manager, then reload.".into()));
    }
    Ok(Catalog {
        packages,
        repositories,
        warnings,
        updates_known,
        source: "System catalog · Existing pacman databases".into(),
        loaded_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

pub fn files(name: &str) -> Result<Vec<String>> {
    let output = checked("pacman", &["-Qlq", "--", name])?;
    Ok(output.lines().map(str::to_owned).collect())
}

pub fn ownership() -> Result<HashMap<String, String>> {
    // Retain only application metadata ownership, not the whole file list.
    let text = checked("pacman", &["-Ql"])?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let (name, path) = line.split_once(' ')?;
            if path.ends_with(".desktop") || path.ends_with(".xml") {
                Some((path.to_owned(), name.to_owned()))
            } else {
                None
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_preserve_optional_dependency_continuations() {
        let r = records(
            "Name            : test\nVersion         : 1:2-3\nOptional Deps   : foo: description\n                  bar: more: detail [installed]\n\n",
        );
        assert_eq!(r[0]["Version"], "1:2-3");
        assert!(r[0]["Optional Deps"].contains("bar: more: detail"));
    }
    #[test]
    fn merge_keeps_repository_priority_and_foreign_packages() {
        let sync = "Repository      : first\nName            : test\nVersion         : 2\n\nRepository      : second\nName            : test\nVersion         : 3\n\n";
        let local = "Name            : test\nVersion         : 1\nInstall Reason  : Explicitly installed\n\nName            : foreign\nVersion         : 1\n\n";
        let p = merge(sync, local, "test 1 -> 2\n");
        assert!(
            p.iter()
                .find(|p| p.repository == "first")
                .unwrap()
                .has_update()
        );
        assert!(
            !p.iter()
                .find(|p| p.repository == "second")
                .unwrap()
                .has_update()
        );
        assert!(
            p.iter()
                .any(|p| p.repository == "local" && p.name == "foreign")
        );
        assert!(!merge(sync, local, "").iter().any(Package::has_update));
    }
}
