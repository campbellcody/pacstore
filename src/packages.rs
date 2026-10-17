use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Package {
    #[serde(default)]
    pub desktop_files: Vec<String>,
    #[serde(default)]
    pub orphan: bool,
    pub name: String,
    pub summary: String,
    pub category: String,
    pub version: String,
    pub installed: Option<String>,
    pub desktop_app: bool,
    pub repository: String,
    pub architecture: String,
    pub download_size: String,
    pub installed_size: String,
    pub install_reason: String,
    pub licenses: String,
    pub upstream: String,
    pub dependencies: String,
    pub optional_dependencies: String,
    pub application_name: String,
    pub description: String,
    pub icon: Option<String>,
    pub screenshots: Vec<String>,
    pub update_available: bool,
    // Repository priority determines which duplicate name is used by Installed/Updates.
    pub alternate: bool,
    pub search_text: String,
}

impl Package {
    pub fn explicitly_installed(&self) -> bool {
        self.install_reason == "Explicitly installed"
    }
    pub fn installed_as_dependency(&self) -> bool {
        self.install_reason == "Installed as a dependency for another package"
    }
    pub fn has_update(&self) -> bool {
        self.installed.is_some() && self.update_available && !self.alternate
    }
    pub fn matches(&self, query: &str, all_packages: bool) -> bool {
        if !all_packages && !self.desktop_app {
            return false;
        }
        let query = query.trim().to_lowercase();
        if self.search_text.is_empty() {
            format!(
                "{} {} {} {}",
                self.name, self.summary, self.category, self.application_name
            )
            .to_lowercase()
            .contains(&query)
        } else {
            self.search_text.contains(&query)
        }
    }
    pub fn index_search(&mut self) {
        self.search_text = format!(
            "{} {} {} {} {}",
            self.name, self.summary, self.category, self.application_name, self.description
        )
        .to_lowercase();
    }
    pub fn display_name(&self) -> &str {
        if self.application_name.is_empty() {
            &self.name
        } else {
            &self.application_name
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub packages: Vec<Package>,
    pub repositories: Vec<String>,
    pub warnings: Vec<String>,
    pub updates_known: bool,
    pub source: String,
    pub loaded_at: u64,
}

impl Catalog {
    pub fn demo() -> Self {
        Self {
            packages: sample_packages(),
            repositories: vec!["extra".into()],
            updates_known: true,
            source: "Demo catalog · No system changes".into(),
            ..Default::default()
        }
    }
}

pub fn sample_packages() -> Vec<Package> {
    // Illustrative versions and installation state, never read from the host.
    let mut packages = vec![
        Package {
            name: "firefox".into(),
            summary: "Browse the web".into(),
            category: "Internet".into(),
            version: "demo-2".into(),
            update_available: true,
            installed: Some("demo-1".into()),
            desktop_app: true,
            repository: "extra".into(),
            ..Default::default()
        },
        Package {
            name: "gimp".into(),
            summary: "Create and edit images".into(),
            category: "Graphics".into(),
            version: "demo-1".into(),
            installed: None,
            desktop_app: true,
            repository: "extra".into(),
            ..Default::default()
        },
        Package {
            name: "vlc".into(),
            summary: "Play music and videos".into(),
            category: "Multimedia".into(),
            version: "demo-1".into(),
            installed: Some("demo-1".into()),
            desktop_app: true,
            repository: "extra".into(),
            ..Default::default()
        },
        Package {
            name: "kate".into(),
            summary: "Edit text and source code".into(),
            category: "Development".into(),
            version: "demo-1".into(),
            installed: None,
            desktop_app: true,
            repository: "extra".into(),
            ..Default::default()
        },
        Package {
            name: "git".into(),
            summary: "Distributed version control".into(),
            category: "Development".into(),
            version: "demo-1".into(),
            installed: Some("demo-1".into()),
            desktop_app: false,
            repository: "extra".into(),
            ..Default::default()
        },
        Package {
            name: "openssl".into(),
            summary: "Cryptography and TLS library".into(),
            category: "Libraries".into(),
            version: "demo-2".into(),
            update_available: true,
            installed: Some("demo-1".into()),
            desktop_app: false,
            repository: "extra".into(),
            ..Default::default()
        },
    ];
    for p in &mut packages {
        if p.installed.is_some() {
            p.install_reason = "Explicitly installed".into();
        }
    }
    let library = packages.iter_mut().find(|p| p.name == "openssl").unwrap();
    library.install_reason = "Installed as a dependency for another package".into();
    library.orphan = true;
    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_filter_keeps_cli_tools_searchable_when_enabled() {
        let packages = sample_packages();
        let git = packages.iter().find(|p| p.name == "git").unwrap();
        assert!(!git.matches("git", false));
        assert!(git.matches("  GIT ", true));
        assert!(packages[0].matches("INTERNET", false));
    }

    #[test]
    fn updates_include_libraries_and_exclude_uninstalled_packages() {
        let names: Vec<_> = sample_packages()
            .into_iter()
            .filter(Package::has_update)
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["firefox", "openssl"]);
    }
}
