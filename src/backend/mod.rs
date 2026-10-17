mod cache;
pub mod metadata;
mod pacman;

use crate::{error::Result, packages::Catalog};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Mode {
    #[default]
    System,
    Demo,
}

pub fn load(mode: Mode, force: bool) -> Result<Catalog> {
    match mode {
        Mode::System => cache::load(force),
        Mode::Demo => Ok(Catalog::demo()),
    }
}

pub struct Details {
    pub files: Vec<String>,
    pub icon: Option<eframe::egui::ColorImage>,
}

pub fn details(mode: Mode, name: &str, installed: bool, icon: Option<String>) -> Result<Details> {
    let files = if mode == Mode::System && installed {
        pacman::files(name)?
    } else {
        Vec::new()
    };
    let icon = icon.as_deref().and_then(self::icon);
    Ok(Details { files, icon })
}

pub fn icon(path: &str) -> Option<eframe::egui::ColorImage> {
    let reader = image::ImageReader::open(path).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    let mut reader = reader.with_guessed_format().ok()?;
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(96, 96).to_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

pub fn launch(package: &str, desktop_file: &str) -> Result<()> {
    use crate::error::AppError;
    let path = std::path::Path::new(desktop_file)
        .canonicalize()
        .map_err(|source| AppError::Io {
            operation: "Find application launcher",
            source,
        })?;
    if path.extension().is_none_or(|ext| ext != "desktop")
        || !(path.starts_with("/usr/share/applications")
            || path.starts_with("/usr/local/share/applications"))
    {
        return Err(AppError::Backend(
            "Only installed system desktop entries can be launched.".into(),
        ));
    }
    let owner = pacman::checked(
        "pacman",
        &[
            "-Qoq",
            "--",
            path.to_str()
                .ok_or_else(|| AppError::Backend("Invalid launcher path".into()))?,
        ],
    )?;
    if !owner.lines().any(|name| name == package) {
        return Err(AppError::Backend(
            "The application launcher no longer belongs to this package. Reload the catalog."
                .into(),
        ));
    }
    let status = std::process::Command::new("/usr/bin/gio")
        .arg("launch")
        .arg(&path)
        .status()
        .map_err(|source| AppError::Io {
            operation: "Launch application using gio",
            source,
        })?;
    if !status.success() {
        return Err(AppError::Backend(format!(
            "Application launch failed: {}",
            status
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artwork_decodes_to_a_bounded_thumbnail_and_missing_files_fall_back() {
        let path = std::env::temp_dir().join(format!("pacstore-icon-{}.png", std::process::id()));
        image::RgbaImage::from_pixel(512, 256, image::Rgba([0, 128, 255, 255]))
            .save(&path)
            .unwrap();
        let image = icon(path.to_str().unwrap()).unwrap();
        assert!(image.size[0] <= 96 && image.size[1] <= 96);
        std::fs::remove_file(&path).unwrap();
        assert!(icon(path.to_str().unwrap()).is_none());
    }

    #[test]
    fn launcher_never_executes_an_arbitrary_desktop_file() {
        let path =
            std::env::temp_dir().join(format!("pacstore-untrusted-{}.desktop", std::process::id()));
        std::fs::write(&path, "[Desktop Entry]\nType=Application\nExec=false\n").unwrap();
        assert!(launch("fixture", path.to_str().unwrap()).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
