use crate::packages::Catalog;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub fn inputs(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for directory in [
        "usr/share/swcatalog/xml",
        "usr/share/app-info/xmls",
        "var/cache/swcatalog/xml",
        "var/lib/swcatalog/xml",
        "var/cache/app-info/xmls",
        "usr/share/metainfo",
        "usr/share/appdata",
        "usr/share/applications",
    ] {
        if let Ok(entries) = fs::read_dir(root.join(directory)) {
            result.extend(entries.flatten().map(|e| e.path()).filter(|p| {
                let name = p.to_string_lossy();
                name.ends_with(".xml") || name.ends_with(".xml.gz") || name.ends_with(".desktop")
            }));
        }
    }
    result.sort();
    result
}

#[derive(Clone, Default, Debug)]
struct App {
    packages: Vec<String>,
    name: String,
    summary: String,
    description: String,
    categories: String,
    icon: Option<String>,
    screenshots: Vec<String>,
    desktop_id: Option<String>,
    desktop_file: Option<String>,
}

fn default_text(node: roxmltree::Node<'_, '_>, tag: &str) -> String {
    let nodes: Vec<_> = node.children().filter(|n| n.has_tag_name(tag)).collect();
    nodes
        .iter()
        .find(|n| {
            n.attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
                .is_none()
        })
        .or_else(|| {
            nodes.iter().find(|n| {
                n.attribute(("http://www.w3.org/XML/1998/namespace", "lang")) == Some("en")
            })
        })
        .map(|n| {
            n.descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn web_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn parse_xml(text: &str) -> Result<Vec<App>, roxmltree::Error> {
    let doc = roxmltree::Document::parse(text)?;
    let base = doc
        .root_element()
        .attribute("media_baseurl")
        .unwrap_or_default();
    Ok(doc
        .descendants()
        .filter(|n| {
            n.has_tag_name("component") && n.attribute("type") == Some("desktop-application")
        })
        .map(|node| {
            let screenshots = node
                .children()
                .filter(|n| n.has_tag_name("screenshots"))
                .flat_map(|n| n.descendants())
                .filter(|n| n.has_tag_name("image") && n.attribute("type") != Some("thumbnail"))
                .filter_map(|n| n.text())
                .map(str::trim)
                .filter(|url| !url.contains(":") || web_url(url))
                .map(|url| {
                    if web_url(url) {
                        url.to_owned()
                    } else {
                        format!(
                            "{}/{}",
                            base.trim_end_matches('/'),
                            url.trim_start_matches('/')
                        )
                    }
                })
                .filter(|url| web_url(url))
                .collect();
            App {
                packages: node
                    .children()
                    .filter(|n| n.has_tag_name("pkgname"))
                    .filter_map(|n| n.text())
                    .map(|s| s.trim().to_owned())
                    .collect(),
                name: default_text(node, "name"),
                summary: default_text(node, "summary"),
                description: default_text(node, "description"),
                categories: node
                    .children()
                    .find(|n| n.has_tag_name("categories"))
                    .map(|n| {
                        n.children()
                            .filter(|n| n.has_tag_name("category"))
                            .filter_map(|n| n.text())
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .unwrap_or_default(),
                icon: node
                    .children()
                    .filter(|n| n.has_tag_name("icon"))
                    .find(|n| matches!(n.attribute("type"), Some("cached" | "stock" | "local")))
                    .and_then(|n| n.text())
                    .map(|s| s.trim().to_owned()),
                screenshots,
                desktop_file: None,
                desktop_id: node
                    .children()
                    .find(|n| {
                        n.has_tag_name("launchable") && n.attribute("type") == Some("desktop-id")
                    })
                    .and_then(|n| n.text())
                    .map(str::to_owned),
            }
        })
        .collect())
}

fn desktop(text: &str) -> Option<App> {
    let mut values = HashMap::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
        } else if inside
            && !line.starts_with('#')
            && let Some((key, value)) = line.split_once('=')
        {
            values.insert(key, value);
        }
    }
    if values.get("Type") != Some(&"Application")
        || values.get("Hidden") == Some(&"true")
        || values.get("NoDisplay") == Some(&"true")
    {
        return None;
    }
    Some(App {
        name: values.get("Name").unwrap_or(&"").to_string(),
        summary: values.get("Comment").unwrap_or(&"").to_string(),
        categories: values
            .get("Categories")
            .unwrap_or(&"")
            .trim_end_matches(';')
            .replace(';', "; "),
        icon: values.get("Icon").map(|s| s.to_string()),
        ..Default::default()
    })
}

fn icon_index(root: &Path) -> HashMap<String, String> {
    fn visit(path: &Path, depth: usize, map: &mut HashMap<String, String>) {
        if depth > 7 {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() && !p.is_symlink() {
                visit(&p, depth + 1, map);
            } else if matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("png" | "jpg" | "jpeg")
            ) && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
            {
                map.entry(stem.to_owned())
                    .or_insert_with(|| p.to_string_lossy().into_owned());
            }
        }
    }
    let mut result = HashMap::new();
    for directory in [
        "usr/share/swcatalog/icons",
        "usr/share/app-info/icons",
        "var/cache/swcatalog/icons",
        "var/cache/app-info/icons",
        "usr/share/icons/hicolor",
        "usr/share/pixmaps",
    ] {
        visit(&root.join(directory), 0, &mut result);
    }
    result
}

pub fn enrich(
    catalog: &mut Catalog,
    root: &Path,
    files: &[PathBuf],
    ownership: &HashMap<String, String>,
) {
    if !files.iter().any(|p| {
        let path = p.to_string_lossy();
        path.contains("swcatalog/xml/") || path.contains("app-info/xmls/")
    }) {
        catalog.warnings.push("Repository AppStream catalogs are not installed. Application views cover locally mapped desktop applications; Show all packages includes the remaining packages.".into());
    }
    let icons = icon_index(root);
    let mut apps = Vec::new();
    let mut failures = 0;
    for path in files {
        let parsed = (|| -> Result<Vec<App>, Box<dyn std::error::Error>> {
            let mut text = String::new();
            if path.extension().is_some_and(|s| s == "gz") {
                flate2::read::GzDecoder::new(fs::File::open(path)?)
                    .take(64 * 1024 * 1024)
                    .read_to_string(&mut text)?;
            } else {
                text = fs::read_to_string(path)?;
            }
            if path.extension().is_some_and(|s| s == "desktop") {
                Ok(desktop(&text).into_iter().collect())
            } else {
                Ok(parse_xml(&text)?)
            }
        })();
        match parsed {
            Ok(parsed) => {
                for mut app in parsed {
                    if path.extension().is_some_and(|s| s == "desktop") {
                        app.desktop_file = Some(path.to_string_lossy().into_owned());
                    } else if let Some(id) = &app.desktop_id {
                        let path = root.join("usr/share/applications").join(id);
                        if fs::read_to_string(&path)
                            .ok()
                            .and_then(|text| desktop(&text))
                            .is_some()
                        {
                            app.desktop_file = Some(path.to_string_lossy().into_owned());
                        }
                    }
                    if app.packages.is_empty() {
                        let owner = ownership.get(path.to_string_lossy().as_ref()).or_else(|| {
                            app.desktop_id.as_ref().and_then(|id| {
                                ownership.get(
                                    root.join("usr/share/applications")
                                        .join(id)
                                        .to_string_lossy()
                                        .as_ref(),
                                )
                            })
                        });
                        if let Some(owner) = owner {
                            app.packages.push(owner.clone());
                        }
                    }
                    apps.push(app);
                }
            }
            Err(error) => {
                failures += 1;
                log::warn!("Metadata {}: {error}", path.display());
            }
        }
    }
    // Package identity, not a guessed application name, is the mapping key.
    let mut index: HashMap<String, Vec<App>> = HashMap::new();
    for app in apps {
        for name in &app.packages {
            index.entry(name.clone()).or_default().push(app.clone());
        }
    }
    for package in &mut catalog.packages {
        if let Some(apps) = index.get(&package.name) {
            package.desktop_app = true;
            let mut categories = HashSet::new();
            for app in apps {
                if root == Path::new("/")
                    && package.installed.is_some()
                    && let Some(path) = &app.desktop_file
                    && ownership
                        .get(path)
                        .is_some_and(|owner| owner == &package.name)
                {
                    package.desktop_files.push(path.clone());
                }
                if !app.name.is_empty() {
                    package.application_name = app.name.clone();
                }
                if !app.summary.is_empty() {
                    package.summary = app.summary.clone();
                }
                if !app.description.is_empty() {
                    package.description = app.description.clone();
                }
                categories.extend(
                    app.categories
                        .split(';')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                );
                if let Some(icon) = &app.icon {
                    let candidate = if Path::new(icon).is_absolute() {
                        let path = root.join(icon.trim_start_matches('/'));
                        path.is_file().then(|| path.to_string_lossy().into_owned())
                    } else {
                        Path::new(icon)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|name| icons.get(name))
                            .cloned()
                    };
                    if candidate.is_some() {
                        package.icon = candidate;
                    }
                }
                package.screenshots.extend(app.screenshots.clone());
            }
            let mut categories: Vec<_> = categories.into_iter().collect();
            categories.sort();
            package.category = categories.join("; ");
            package.desktop_files.sort();
            package.desktop_files.dedup();
            package.screenshots.sort();
            package.screenshots.dedup();
        }
        package.index_search();
    }
    if failures > 0 {
        catalog.warnings.push(format!("Skipped {failures} unreadable or invalid application metadata files. Package browsing remains available."));
    }
    if !catalog.packages.iter().any(|p| p.desktop_app) {
        catalog.warnings.push(
            "No mapped application metadata. Enable Show all packages to browse by package name."
                .into(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml_uses_package_identity_and_ignores_localized_names_and_unsafe_screenshots() {
        let apps = parse_xml(r#"<components media_baseurl="https://example.org/media"><component type="desktop-application"><pkgname>pkg</pkgname><name xml:lang="de">Deutsch</name><name>App</name><summary>A &amp; B</summary><categories><category>Graphics</category></categories><screenshots><screenshot><image type="source">shot.png</image></screenshot><screenshot><image type="source">file:///etc/passwd</image></screenshot></screenshots></component><component type="console-application"><pkgname>cli</pkgname></component></components>"#).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "App");
        assert_eq!(apps[0].summary, "A & B");
        assert_eq!(apps[0].packages, ["pkg"]);
        assert_eq!(apps[0].screenshots.len(), 1);
        assert_eq!(apps[0].screenshots[0], "https://example.org/media/shot.png");
    }
    #[test]
    fn desktop_entries_exclude_hidden_and_non_application_launchers() {
        assert!(desktop("[Desktop Entry]\nType=Application\nName=App\nNoDisplay=true").is_none());
        assert!(desktop("[Desktop Entry]\nType=Link\nName=App").is_none());
        let app = desktop("[Desktop Entry]\nType=Application\nName=App\nCategories=Graphics;Utility;\n[Desktop Action Extra]\nName=Wrong").unwrap();
        assert_eq!(app.name, "App");
        assert_eq!(app.categories, "Graphics; Utility");
    }
}
