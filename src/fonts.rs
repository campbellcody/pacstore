//! Resolve Fontconfig's system families once, before the GUI event loop starts.
use eframe::egui::{FontData, FontDefinitions, FontFamily};
use std::{fs, process::Command};

pub fn system_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (pattern, family) in [
        ("sans-serif", FontFamily::Proportional),
        ("monospace", FontFamily::Monospace),
    ] {
        match load(pattern) {
            Ok(data) => {
                let name = format!("system-{pattern}");
                fonts.font_data.insert(name.clone(), data.into());
                fonts.families.entry(family).or_default().insert(0, name);
                log::info!("Loaded system {pattern} font");
            }
            Err(error) => log::warn!("Using bundled {pattern} fallback: {error}"),
        }
    }
    fonts
}

fn load(pattern: &str) -> Result<FontData, Box<dyn std::error::Error>> {
    let query = format!("{pattern}:outline=true");
    let output = Command::new("fc-match")
        .args(["--format=%{file}\n%{index}\n", &query])
        .output()?;
    if !output.status.success() {
        return Err("Fontconfig lookup failed".into());
    }
    let text = std::str::from_utf8(&output.stdout)?;
    let mut lines = text.lines();
    let path = lines
        .next()
        .filter(|s| !s.is_empty())
        .ok_or("Fontconfig returned no font file")?;
    // Fontconfig uses the upper bits for variable-font instance information.
    let index = lines
        .next()
        .ok_or("Fontconfig returned no face index")?
        .parse::<u32>()?
        & 0xffff;
    let mut data = FontData::from_owned(fs::read(path)?);
    data.index = index;
    Ok(data)
}
