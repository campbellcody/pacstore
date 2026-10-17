use crate::error::{AppError, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => crate::t!("System"),
            Self::Light => crate::t!("Light"),
            Self::Dark => crate::t!("Dark"),
        }
    }

    fn value(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn apply(self, ctx: &eframe::egui::Context) {
        use eframe::egui::ThemePreference;
        ctx.set_theme(match self {
            Self::System => ThemePreference::System,
            Self::Light => ThemePreference::Light,
            Self::Dark => ThemePreference::Dark,
        });
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Scale {
    Small,
    #[default]
    Normal,
    Large,
    ExtraLarge,
}
impl Scale {
    pub const ALL: [Self; 4] = [Self::Small, Self::Normal, Self::Large, Self::ExtraLarge];
    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "85%",
            Self::Normal => "100%",
            Self::Large => "125%",
            Self::ExtraLarge => "150%",
        }
    }
    pub fn factor(self) -> f32 {
        match self {
            Self::Small => 0.85,
            Self::Normal => 1.0,
            Self::Large => 1.25,
            Self::ExtraLarge => 1.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Settings {
    pub all_packages: bool,
    pub theme: Theme,
    pub scale: Scale,
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
}

fn config_path(xdg: Option<PathBuf>, home: Option<PathBuf>) -> Result<PathBuf> {
    let base = xdg
        .filter(|p| p.is_absolute())
        .or_else(|| home.filter(|p| p.is_absolute()).map(|p| p.join(".config")))
        .ok_or_else(|| {
            AppError::Configuration(
                "Set an absolute XDG_CONFIG_HOME or HOME to save preferences.".into(),
            )
        })?;
    Ok(base.join("pacstore/settings.conf"))
}

fn parse(text: &str) -> Result<Settings> {
    let mut version = None;
    let mut all_packages = None;
    let mut theme = None;
    let mut scale = None;
    for line in text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
            Some(("version", "1")) if version.is_none() => version = Some(1),
            Some(("all_packages", value)) if all_packages.is_none() => {
                all_packages = Some(value.parse::<bool>().map_err(|_| {
                    AppError::Configuration("all_packages must be true or false.".into())
                })?)
            }
            Some(("theme", value)) if theme.is_none() => {
                theme = Some(match value {
                    "system" => Theme::System,
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    _ => {
                        return Err(AppError::Configuration(
                            "theme must be system, light, or dark.".into(),
                        ));
                    }
                })
            }
            Some(("scale", value)) if scale.is_none() => {
                scale = Some(match value {
                    "85%" => Scale::Small,
                    "100%" => Scale::Normal,
                    "125%" => Scale::Large,
                    "150%" => Scale::ExtraLarge,
                    _ => {
                        return Err(AppError::Configuration(
                            "scale must be 85%, 100%, 125%, or 150%.".into(),
                        ));
                    }
                })
            }
            _ => return Err(AppError::Configuration(
                "Unsupported, duplicate, or invalid preference. See README for supported settings."
                    .into(),
            )),
        }
    }
    if version.is_none() || all_packages.is_none() {
        return Err(AppError::Configuration(
            "Missing version or all_packages setting.".into(),
        ));
    }
    Ok(Settings {
        all_packages: all_packages.unwrap(),
        theme: theme.unwrap_or_default(),
        scale: scale.unwrap_or_default(),
    })
}

impl Store {
    pub fn load(&self) -> Result<Settings> {
        match fs::read_to_string(&self.path) {
            Ok(text) => parse(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(source) => Err(AppError::Io {
                operation: "Read preferences",
                source,
            }),
        }
    }

    pub fn save(&self, settings: Settings) -> Result<()> {
        // Never silently overwrite an invalid or newer configuration file.
        self.load()?;
        let parent = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|source| AppError::Io {
            operation: "Create preferences directory",
            source,
        })?;
        let temporary = parent.join(format!(
            ".settings-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| AppError::Io {
                operation: "Create temporary preferences file",
                source,
            })?;
        let result = (|| {
            use std::io::Write;
            write!(
                file,
                "version=1\nall_packages={}\ntheme={}\nscale={}\n",
                settings.all_packages,
                settings.theme.value(),
                settings.scale.label()
            )?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|source| AppError::Io {
            operation: "Save preferences",
            source,
        })
    }
}

pub fn initialize() -> (Option<Store>, Settings, Option<AppError>) {
    let path = config_path(
        env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
    );
    match path {
        Ok(path) => {
            let store = Store { path };
            match store.load() {
                Ok(settings) => (Some(store), settings, None),
                Err(error) => {
                    log::error!("{error}");
                    (Some(store), Settings::default(), Some(error))
                }
            }
        }
        Err(error) => {
            log::error!("{error}");
            (None, Settings::default(), Some(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_preferences_accept_old_files_and_reject_invalid_values() {
        let original = "version=1\nall_packages=false\n";
        assert_eq!(parse(original).unwrap().theme, Theme::System);
        for (value, expected) in [
            ("system", Theme::System),
            ("light", Theme::Light),
            ("dark", Theme::Dark),
        ] {
            assert_eq!(
                parse(&format!("{original}theme={value}\n")).unwrap().theme,
                expected
            );
        }
        assert!(parse(&format!("{original}theme=unknown\n")).is_err());
        assert!(parse(&format!("{original}theme=light\ntheme=dark\n")).is_err());
    }

    #[test]
    fn respects_xdg_and_ignores_relative_paths() {
        assert_eq!(
            config_path(Some("/tmp/xdg".into()), Some("/tmp/home".into())).unwrap(),
            PathBuf::from("/tmp/xdg/pacstore/settings.conf")
        );
        assert_eq!(
            config_path(Some("relative".into()), Some("/tmp/home".into())).unwrap(),
            PathBuf::from("/tmp/home/.config/pacstore/settings.conf")
        );
        assert!(config_path(None, None).is_err());
    }
    #[test]
    fn saves_atomically_and_preserves_invalid_configuration() {
        let root = env::temp_dir().join(format!(
            "pacstore-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store {
            path: root.join("settings.conf"),
        };
        assert_eq!(store.load().unwrap(), Settings::default());
        let settings = Settings {
            all_packages: true,
            theme: Theme::Dark,
            scale: Scale::Large,
        };
        store.save(settings).unwrap();
        assert_eq!(store.load().unwrap(), settings);
        fs::write(&store.path, "version=2\nall_packages=true\n").unwrap();
        assert!(store.save(Settings::default()).is_err());
        assert!(
            fs::read_to_string(&store.path)
                .unwrap()
                .starts_with("version=2")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
