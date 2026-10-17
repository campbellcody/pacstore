//! Editorial front-page collections. Entries must exist in a configured official
//! repository; metadata availability never determines whether they are featured.
use crate::packages::Package;

pub struct Category {
    pub title: &'static str,
    pub description: &'static str,
    pub packages: &'static [&'static str],
}

pub const CATEGORIES: &[Category] = &[
    Category {
        title: "Browsers",
        description: "Explore the web.",
        packages: &["firefox", "chromium", "epiphany", "falkon"],
    },
    Category {
        title: "Utilities",
        description: "Useful tools for everyday tasks.",
        packages: &[
            "keepassxc",
            "ark",
            "file-roller",
            "gnome-disk-utility",
            "qalculate-gtk",
            "gnome-calculator",
        ],
    },
    Category {
        title: "Productivity",
        description: "Write, plan, and get things done.",
        packages: &[
            "libreoffice-fresh",
            "thunderbird",
            "obsidian",
            "ghostwriter",
            "okular",
            "evince",
        ],
    },
    Category {
        title: "Development",
        description: "Edit code and build software.",
        packages: &["code", "kate", "geany", "qtcreator", "gnome-builder", "git"],
    },
    Category {
        title: "Graphics & design",
        description: "Create images, illustrations, and 3D scenes.",
        packages: &["gimp", "inkscape", "krita", "blender", "darktable"],
    },
    Category {
        title: "Music & video",
        description: "Watch, listen, record, and edit.",
        packages: &["vlc", "mpv", "audacity", "obs-studio", "kdenlive"],
    },
    Category {
        title: "Communication",
        description: "Stay in touch with people and communities.",
        packages: &["signal-desktop", "utox", "element-desktop", "discord"],
    },
    Category {
        title: "Games",
        description: "Play and explore.",
        packages: &["supertuxkart", "0ad", "wesnoth", "minetest", "luanti"],
    },
];

pub fn entries(category: &Category, packages: &[Package]) -> Vec<usize> {
    category
        .packages
        .iter()
        .filter_map(|name| {
            packages
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.name == *name && matches!(p.repository.as_str(), "core" | "extra")
                })
                .min_by_key(|(_, p)| if p.repository == "core" { 0 } else { 1 })
                .map(|(i, _)| i)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_available_official_entries_are_featured_even_without_appstream() {
        let packages = vec![
            Package {
                name: "firefox".into(),
                repository: "custom".into(),
                ..Default::default()
            },
            Package {
                name: "firefox".into(),
                repository: "extra".into(),
                alternate: true,
                ..Default::default()
            },
            Package {
                name: "chromium".into(),
                repository: "local".into(),
                ..Default::default()
            },
            Package {
                name: "unlisted".into(),
                repository: "core".into(),
                desktop_app: true,
                ..Default::default()
            },
        ];
        assert_eq!(entries(&CATEGORIES[0], &packages), [1]);
    }
    #[test]
    fn collections_do_not_duplicate_package_names() {
        let mut seen = std::collections::HashSet::new();
        for category in CATEGORIES {
            for name in category.packages {
                assert!(seen.insert(name));
            }
        }
    }
}
