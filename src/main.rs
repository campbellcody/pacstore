mod artwork;
mod backend;
mod config;
mod error;
mod featured;
mod fonts;
mod helper;
mod i18n;
mod logging;
mod packages;
mod protocol;
mod recovery;
mod system_transaction;
mod task;
mod theme;
mod transaction;
mod ui;

fn main() -> eframe::Result {
    logging::init();
    let (store, settings, startup_error) = config::initialize();
    let mode = if std::env::args().any(|arg| arg == "--demo") {
        backend::Mode::Demo
    } else {
        backend::Mode::System
    };
    if std::env::args().any(|arg| arg == "--check-catalog") {
        match backend::load(mode, std::env::args().any(|arg| arg == "--reload")) {
            Ok(catalog) => {
                println!(
                    "{}: {} packages, {} applications, {} installed, {} updates",
                    catalog.source,
                    catalog.packages.len(),
                    catalog.packages.iter().filter(|p| p.desktop_app).count(),
                    catalog
                        .packages
                        .iter()
                        .filter(|p| p.installed.is_some() && !p.alternate)
                        .count(),
                    if catalog.updates_known {
                        catalog
                            .packages
                            .iter()
                            .filter(|p| p.has_update())
                            .count()
                            .to_string()
                    } else {
                        "unknown".into()
                    }
                );
                for warning in catalog.warnings {
                    eprintln!("Warning: {warning}");
                }
                return Ok(());
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    // SAFETY: geteuid has no preconditions; the GUI must stay unprivileged.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!(
            "Run Pacstore as your normal desktop user. Only its transaction helper runs as root."
        );
        std::process::exit(1);
    }
    log::info!("Starting Pacstore");
    let fonts = fonts::system_fonts();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([760.0, 520.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Pacstore",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(ui::Pacstore::new(
                &cc.egui_ctx,
                store,
                settings,
                startup_error,
                mode,
            )))
        }),
    )
}
