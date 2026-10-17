use crate::{t, trf};
use eframe::egui;

use crate::{
    artwork::Artwork,
    backend::{self, Mode},
    config::{Scale, Settings, Store, Theme},
    error::AppError,
    featured,
    packages::{Catalog, Package},
    protocol::{Command as HelperCommand, Operation},
    recovery,
    system_transaction::Session,
    task::Task,
    theme,
    transaction::{self, Request},
};

#[derive(Clone, Copy, Default, PartialEq)]
enum Page {
    #[default]
    Explore,
    Search,
    Installed,
    Updates,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Explore => t!("Explore"),
            Self::Search => t!("Search"),
            Self::Installed => t!("Installed"),
            Self::Updates => t!("Updates"),
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    Install(usize),
    Remove(usize),
    Upgrade,
    CleanOrphans,
}

impl Action {
    fn title(self) -> &'static str {
        match self {
            Self::Install(_) => t!("Preview installation"),
            Self::Remove(_) => t!("Preview removal"),
            Self::Upgrade {} => t!("Preview full system upgrade"),
            Self::CleanOrphans => t!("Review orphan cleanup"),
        }
    }
}

#[derive(Default)]
pub struct Pacstore {
    page: Page,
    query: String,
    all_packages: bool,
    theme: Theme,
    scale: Scale,
    installed_scope: u8,
    installed_query: String,
    search_focus: bool,
    artwork: Artwork,
    launch_task: Option<Task<()>>,
    packages: Vec<Package>,
    loading: Option<Task<Catalog>>,
    mode: Mode,
    repositories: Vec<String>,
    warnings: Vec<String>,
    source: String,
    updates_known: bool,
    loaded_at: u64,
    repository_filter: String,
    category_filter: String,
    installed_filter: u8,
    detail_task: Option<Task<backend::Details>>,
    detail_index: Option<usize>,
    detail_error: Option<String>,
    files: Vec<String>,
    icon: Option<egui::TextureHandle>,
    transaction: Option<Task<Vec<Package>>>,
    system: Option<Session>,
    system_finished: bool,
    optional_for: String,
    optional_targets: std::collections::BTreeSet<String>,
    cascade_dependents: bool,
    config_save: Option<Task<()>>,
    config_store: Option<Store>,
    error: Option<AppError>,
    selected: Option<usize>,
    pending: Option<Action>,
    notice: Option<String>,
    close_after_tasks: bool,
}

impl Pacstore {
    pub fn new(
        ctx: &egui::Context,
        store: Option<Store>,
        settings: Settings,
        error: Option<AppError>,
        mode: Mode,
    ) -> Self {
        settings.theme.apply(ctx);
        ctx.set_zoom_factor(settings.scale.factor());
        let mut app = Self {
            mode,
            theme: settings.theme,
            scale: settings.scale,
            config_store: store,
            all_packages: settings.all_packages,
            error,
            ..Default::default()
        };
        app.loading = Some(Task::spawn(ctx, move || backend::load(mode, false)));
        app
    }

    fn busy(&self) -> bool {
        self.loading.is_some()
            || self.transaction.is_some()
            || self
                .system
                .as_ref()
                .is_some_and(|session| session.outcome.is_none())
    }

    fn reload(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let mode = self.mode;
        self.loading = Some(Task::spawn(ctx, move || backend::load(mode, true)));
        self.detail_task = None;
        self.detail_index = None;
        self.artwork.clear();
        self.files.clear();
        self.icon = None;
        self.selected = None;
        self.pending = None;
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        self.artwork.poll(ctx);
        if let Some(result) = self.launch_task.as_ref().and_then(Task::poll) {
            self.launch_task = None;
            match result {
                Ok(()) => self.notice = Some(t!("Launch request sent.").into()),
                Err(error) => self.error = Some(error),
            }
        }
        if !self.system_finished
            && let Some(session) = &mut self.system
        {
            session.poll();
            if let Some((success, message)) = session.outcome.clone() {
                self.system_finished = true;
                if success {
                    self.notice = Some(message);
                } else {
                    self.error = Some(AppError::Backend(message));
                }
                self.selected = None;
                self.detail_task = None;
                self.detail_index = None;
                // Refresh even after failure: commit may have changed some packages.
                self.loading = Some(Task::spawn(ctx, || backend::load(Mode::System, true)));
            }
        }
        if let Some(result) = self.loading.as_ref().and_then(Task::poll) {
            self.loading = None;
            match result {
                Ok(catalog) => {
                    self.packages = catalog.packages;
                    self.repositories = catalog.repositories;
                    self.warnings = catalog.warnings;
                    self.updates_known = catalog.updates_known;
                    self.source = catalog.source;
                    self.loaded_at = catalog.loaded_at;
                    log::info!("Catalog loaded: {} packages", self.packages.len());
                }
                Err(error) => {
                    self.error = Some(error);
                    self.updates_known = false;
                    for package in &mut self.packages {
                        package.update_available = false;
                    }
                    if !self.packages.is_empty() {
                        self.source =
                            t!("Previous catalog · Reload failed · Package state may be stale")
                                .into();
                    }
                }
            }
        }
        if let Some(result) = self.transaction.as_ref().and_then(Task::poll) {
            self.transaction = None;
            match result {
                Ok(packages) => {
                    self.packages = packages;
                    self.notice =
                        Some(t!("Demo state updated. No system packages were changed.").into());
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.config_save.as_ref().and_then(Task::poll) {
            self.config_save = None;
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
    }

    fn submit(&mut self, ctx: &egui::Context, action: Action) {
        if self.busy() {
            return;
        }
        if self.mode == Mode::System {
            let operation = match action {
                Action::Install(i) => {
                    let package = &self.packages[i];
                    let mut targets = vec![format!("{}/{}", package.repository, package.name)];
                    targets.extend(self.optional_targets.iter().cloned());
                    Operation::Install { targets }
                }
                Action::Remove(i) => Operation::Remove {
                    target: self.packages[i].name.clone(),
                    cascade: self.cascade_dependents,
                },
                Action::Upgrade => Operation::Upgrade {},
                Action::CleanOrphans => Operation::CleanOrphans {},
            };
            match Session::start(ctx, operation) {
                Ok(session) => {
                    self.system = Some(session);
                    self.system_finished = false;
                    self.error = None;
                    self.selected = None;
                    self.notice = None;
                }
                Err(error) => self.error = Some(error),
            }
            return;
        }
        let request = match action {
            Action::Install(i) => Request::Install(self.packages[i].name.clone()),
            Action::Remove(i) => Request::Remove(self.packages[i].name.clone()),
            Action::Upgrade => Request::Upgrade,
            Action::CleanOrphans => Request::CleanOrphans,
        };
        let packages = self.packages.clone();
        self.transaction = Some(Task::spawn(ctx, move || {
            transaction::execute_demo(packages, request)
        }));
        self.selected = None;
        self.notice = None;
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        let Some(store) = self.config_store.clone() else {
            self.error = Some(AppError::Configuration(
                t!("Preferences cannot be saved without a configuration directory.").into(),
            ));
            return;
        };
        let settings = Settings {
            all_packages: self.all_packages,
            theme: self.theme,
            scale: self.scale,
        };
        self.config_save = Some(Task::spawn(ctx, move || store.save(settings)));
    }

    fn visible(&self, package: &Package) -> bool {
        if self.page != Page::Updates
            && ((!self.repository_filter.is_empty()
                && self.repository_filter != package.repository)
                || (!self.category_filter.is_empty()
                    && !package
                        .category
                        .split(';')
                        .any(|c| c.trim() == self.category_filter))
                || (self.page == Page::Search
                    && self.installed_filter == 1
                    && package.installed.is_none())
                || (self.page == Page::Search
                    && self.installed_filter == 2
                    && package.installed.is_some()))
        {
            return false;
        }
        match self.page {
            Page::Explore => package.matches("", self.all_packages),
            Page::Search => package.matches(&self.query, self.all_packages),
            Page::Installed => {
                !package.alternate
                    && package.installed.is_some()
                    && package.matches(&self.installed_query, true)
                    && match self.installed_scope {
                        0 => package.explicitly_installed(),
                        1 => package.installed_as_dependency(),
                        3 => package.orphan,
                        _ => true,
                    }
            }
            // A system upgrade must include libraries and CLI tools as well.
            Page::Updates => package.has_update(),
        }
    }

    fn featured_page(&mut self, ui: &mut egui::Ui) {
        ui.label(t!(
            "Notable apps from core and extra, selected for everyday use."
        ));
        if ui.button(t!("Search all packages")).clicked() {
            self.page = Page::Search;
            self.all_packages = true;
            self.query.clear();
            self.repository_filter.clear();
            self.category_filter.clear();
            self.installed_filter = 0;
        }
        ui.separator();
        if self.busy() {
            ui.add(egui::Spinner::new().color(theme::palette(ui).accent));
            ui.label(t!("Loading featured apps…"));
            return;
        }
        let sections: Vec<_> = featured::CATEGORIES
            .iter()
            .map(|category| (category, featured::entries(category, &self.packages)))
            .filter(|(_, entries)| !entries.is_empty())
            .collect();
        if sections.is_empty() {
            ui.label(t!("No featured apps are available in the loaded core and extra databases. Reload the catalog or use Search to browse other packages."));
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("featured_collections")
            .show(ui, |ui| {
                let columns = if ui.available_width() >= 1000.0 { 3 } else { 2 };
                for (category, entries) in sections {
                    ui.heading(t!(category.title));
                    ui.weak(t!(category.description));
                    ui.add_space(6.0);
                    for row in entries.chunks(columns) {
                        ui.columns(columns, |columns| {
                            for (column, &index) in columns.iter_mut().zip(row) {
                                let package = &self.packages[index];
                                column.push_id((&package.repository, &package.name), |ui| {
                                    egui::Frame::group(ui.style()).show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.set_min_height(108.0);
                                        self.artwork.show(ui, package, 36.0);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(package.display_name())
                                                    .strong(),
                                            )
                                            .truncate(),
                                        );
                                        ui.weak(format!(
                                            "{} · {}",
                                            package.name, package.repository
                                        ));
                                        ui.add(egui::Label::new(&package.summary).truncate());
                                        ui.horizontal(|ui| {
                                            if theme::details_button(ui, package.display_name())
                                                .clicked()
                                            {
                                                self.selected = Some(index);
                                            }
                                            if package.installed.is_some() {
                                                ui.colored_label(
                                                    theme::palette(ui).success,
                                                    t!("Installed"),
                                                );
                                            }
                                        });
                                    });
                                });
                            }
                        });
                        ui.add_space(6.0);
                    }
                    ui.add_space(18.0);
                }
            });
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.page.title());
        ui.add_space(8.0);
        if self.page == Page::Explore {
            self.featured_page(ui);
            return;
        }
        match self.page {
            Page::Explore => unreachable!("Explore renders its own collections"),
            Page::Search => {
                let label = ui.label(t!("Search packages"));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(t!("Search by package name, description, or category"))
                        .desired_width(f32::INFINITY),
                );
                let response = response.labelled_by(label.id);
                if self.search_focus {
                    response.request_focus();
                    self.search_focus = false;
                }
            }
            Page::Installed => {
                ui.label(t!("Manage installed applications and packages."));
                ui.add(
                    egui::TextEdit::singleline(&mut self.installed_query)
                        .hint_text(t!("Search installed packages")),
                );
                ui.horizontal_wrapped(|ui| {
                    for (scope, label) in [
                        (0, t!("Manually installed")),
                        (1, t!("Dependencies")),
                        (2, t!("All installed")),
                        (3, t!("Orphans")),
                    ] {
                        ui.selectable_value(&mut self.installed_scope, scope, label);
                    }
                });
                let count = self
                    .packages
                    .iter()
                    .filter(|p| p.orphan && !p.alternate)
                    .count();
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(trf!(
                            "Review orphan cleanup ({count} known)",
                            count = count
                        )),
                    )
                    .clicked()
                {
                    self.pending = Some(Action::CleanOrphans);
                }
                ui.weak(t!("Cleanup checks all unused dependency packages again before preparing a removal plan."));
            }
            Page::Updates => {
                ui.label(t!(
                    "Full system upgrades include applications, tools, and libraries."
                ));
                if !self.updates_known {
                    ui.colored_label(theme::palette(ui).warning, t!("Update availability is incomplete or unknown; reload after resolving database errors."));
                }
                let count = self.packages.iter().filter(|p| p.has_update()).count();
                if theme::primary_button(
                    ui,
                    &trf!(
                        "Review full system upgrade ({count} known updates)",
                        count = count
                    ),
                    !self.busy(),
                )
                .clicked()
                {
                    self.pending = Some(Action::Upgrade);
                }
            }
        }
        if self.page == Page::Search
            && ui
                .add_enabled(
                    self.config_save.is_none(),
                    egui::Checkbox::new(
                        &mut self.all_packages,
                        t!("Show all packages (including libraries and command-line tools)"),
                    ),
                )
                .changed()
        {
            self.save_settings(ui.ctx());
        }
        if self.page != Page::Updates {
            ui.horizontal_wrapped(|ui| {
                let mut repositories = self.repositories.clone();
                if self.packages.iter().any(|p| p.repository == "local") {
                    repositories.push("local".into());
                }
                egui::ComboBox::from_label(t!("Repository"))
                    .selected_text(if self.repository_filter.is_empty() {
                        t!("All repositories")
                    } else {
                        &self.repository_filter
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.repository_filter,
                            String::new(),
                            t!("All repositories"),
                        );
                        for repository in repositories {
                            ui.selectable_value(
                                &mut self.repository_filter,
                                repository.clone(),
                                repository,
                            );
                        }
                    });
                let categories: std::collections::BTreeSet<String> = self
                    .packages
                    .iter()
                    .flat_map(|p| p.category.split(';'))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                egui::ComboBox::from_label(t!("Category"))
                    .selected_text(if self.category_filter.is_empty() {
                        t!("All categories")
                    } else {
                        &self.category_filter
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.category_filter,
                            String::new(),
                            t!("All categories"),
                        );
                        for category in categories {
                            ui.selectable_value(
                                &mut self.category_filter,
                                category.clone(),
                                category,
                            );
                        }
                    });
                if self.page == Page::Search {
                    egui::ComboBox::from_label(t!("Installation status"))
                        .selected_text(match self.installed_filter {
                            1 => t!("Installed"),
                            2 => t!("Not installed"),
                            _ => t!("Any install status"),
                        })
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (0, t!("Any install status")),
                                (1, t!("Installed")),
                                (2, t!("Not installed")),
                            ] {
                                ui.selectable_value(&mut self.installed_filter, value, label);
                            }
                        });
                }
            });
        }
        ui.separator();
        if self.busy() {
            ui.add(egui::Spinner::new().color(theme::palette(ui).accent));
            ui.label(
                if self.system.as_ref().is_some_and(|s| s.outcome.is_none()) {
                    t!("Package transaction in progress…")
                } else if self.transaction.is_some() {
                    t!("Applying demo change…")
                } else {
                    t!("Loading package catalog…")
                },
            );
            return;
        }
        let visible: Vec<_> = self
            .packages
            .iter()
            .enumerate()
            .filter(|(_, p)| self.visible(p))
            .map(|(i, _)| i)
            .collect();
        if visible.is_empty() {
            ui.label(match self.page {
                Page::Search => {
                    t!("No matching packages. Try a different search or show all packages.")
                }
                Page::Updates if !self.updates_known => t!("Update availability is unknown. Reload when package databases are available."),
                Page::Updates => t!("No updates in the currently loaded databases. This does not check remote repositories."),
                _ => t!("No packages to display with this filter."),
            });
        }
        let line = ui.text_style_height(&egui::TextStyle::Body);
        let row_height = line * 2.0 + line.max(28.0) + ui.spacing().interact_size.y + 24.0;
        egui::ScrollArea::vertical().show_rows(ui, row_height, visible.len(), |ui, rows| {
            for row in rows {
                let index = visible[row];
                let package = &self.packages[index];
                ui.push_id((&package.repository, &package.name), |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.set_min_height(row_height - 12.0);
                        ui.horizontal(|ui| {
                            self.artwork.show(ui, package, 28.0);
                            if theme::selectable(
                                ui,
                                self.selected == Some(index),
                                egui::RichText::new(package.display_name()).strong(),
                            )
                            .clicked()
                            {
                                self.selected = Some(index);
                            }
                            if package.has_update() {
                                ui.colored_label(theme::palette(ui).info, t!("Update available"));
                            } else if package.installed.is_some() {
                                ui.colored_label(theme::palette(ui).success, t!("Installed"));
                            }
                        });
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{} · {}",
                                    package.name, package.repository
                                ))
                                .weak(),
                            )
                            .truncate(),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(&package.summary).weak())
                                .truncate(),
                        );
                        if theme::details_button(ui, package.display_name()).clicked() {
                            self.selected = Some(index);
                        }
                    });
                });
            }
        });
    }

    fn details(&mut self, ctx: &egui::Context) {
        let Some(index) = self.selected else { return };
        if self.detail_index != Some(index) {
            self.detail_index = Some(index);
            self.files.clear();
            self.icon = None;
            self.detail_error = None;
            let package = &self.packages[index];
            let name = package.name.clone();
            let installed = package.installed.is_some();
            let icon = package.icon.clone();
            let mode = self.mode;
            self.detail_task = Some(Task::spawn(ctx, move || {
                backend::details(mode, &name, installed, icon)
            }));
        }
        if let Some(result) = self.detail_task.as_ref().and_then(Task::poll) {
            self.detail_task = None;
            match result {
                Ok(details) => {
                    self.files = details.files;
                    self.icon = details.icon.map(|image| {
                        ctx.load_texture("package-icon", image, egui::TextureOptions::LINEAR)
                    });
                }
                Err(error) => self.detail_error = Some(error.to_string()),
            }
        }
        let package = &self.packages[index];
        let mut open = true;
        egui::Window::new(trf!("{name} — package details", name = package.name))
            .open(&mut open)
            .default_width(560.0)
            .default_height(580.0)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(icon) = &self.icon {
                        ui.image((icon.id(), egui::vec2(48.0, 48.0)));
                    }
                    ui.vertical(|ui| {
                        ui.heading(package.display_name());
                        ui.label(egui::RichText::new(&package.summary).weak());
                    });
                });
                ui.separator();
                egui::Grid::new("package_metadata")
                    .striped(true)
                    .show(ui, |ui| {
                        for (label, value) in [
                            (t!("Package"), package.name.as_str()),
                            (
                                t!("Available"),
                                if package.repository == "local" {
                                    t!("Not in configured repositories")
                                } else {
                                    package.version.as_str()
                                },
                            ),
                            (
                                t!("Installed"),
                                package.installed.as_deref().unwrap_or(t!("Not installed")),
                            ),
                            (t!("Repository"), package.repository.as_str()),
                            (t!("Architecture"), package.architecture.as_str()),
                            (t!("Download size"), package.download_size.as_str()),
                            (t!("Installed size"), package.installed_size.as_str()),
                            (t!("Install reason"), package.install_reason.as_str()),
                            (t!("License"), package.licenses.as_str()),
                            (t!("Categories"), package.category.as_str()),
                            (
                                t!("Type"),
                                if package.desktop_app {
                                    t!("Desktop application")
                                } else {
                                    t!("Other package")
                                },
                            ),
                        ] {
                            ui.label(label);
                            ui.label(if value.is_empty() {
                                t!("Not available")
                            } else {
                                value
                            });
                            ui.end_row();
                        }
                    });
                if package.upstream.starts_with("https://")
                    || package.upstream.starts_with("http://")
                {
                    ui.hyperlink_to(t!("Upstream website"), &package.upstream);
                }
                if !package.description.is_empty() {
                    ui.separator();
                    ui.label(&package.description);
                }
                ui.collapsing(t!("Dependencies"), |ui| {
                    ui.label(if package.dependencies.is_empty() {
                        t!("None")
                    } else {
                        &package.dependencies
                    });
                });
                ui.collapsing(t!("Optional dependencies"), |ui| {
                    ui.label(if package.optional_dependencies.is_empty() {
                        t!("None")
                    } else {
                        &package.optional_dependencies
                    });
                });
                ui.collapsing(t!("Screenshots"), |ui| {
                    if package.screenshots.is_empty() {
                        ui.label(t!("No screenshots in local application metadata."));
                    }
                    for (i, url) in package.screenshots.iter().enumerate() {
                        ui.hyperlink_to(
                            trf!(
                                "Open screenshot {number} (requires network)",
                                number = i + 1
                            ),
                            url,
                        );
                    }
                });
                ui.collapsing(t!("Installed files"), |ui| {
                    if self.detail_task.is_some() {
                        ui.spinner();
                        ui.label(t!("Reading installed files…"));
                    } else if let Some(error) = &self.detail_error {
                        ui.colored_label(theme::palette(ui).danger, error);
                    } else if self.files.is_empty() {
                        ui.label(t!("No installed file list available for this package."));
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("installed_files")
                        .max_height(180.0)
                        .show_rows(
                            ui,
                            ui.text_style_height(&egui::TextStyle::Monospace),
                            self.files.len(),
                            |ui, rows| {
                                for row in rows {
                                    ui.monospace(&self.files[row]);
                                }
                            },
                        );
                });
                ui.separator();
                {
                    if package.installed.is_some() {
                        if theme::danger_button(ui, t!("Remove…")).clicked() {
                            self.pending = Some(Action::Remove(index));
                        }
                    } else if theme::primary_button(ui, t!("Install…"), true).clicked() {
                        self.pending = Some(Action::Install(index));
                    }
                }
                if self.mode == Mode::System && package.installed.is_some() {
                    for (i, desktop_file) in package.desktop_files.iter().enumerate() {
                        let label = if package.desktop_files.len() == 1 {
                            t!("Launch application").to_owned()
                        } else {
                            trf!(
                                "Launch {name}",
                                name = std::path::Path::new(desktop_file)
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            )
                        };
                        if ui
                            .push_id(("launch", i), |ui| {
                                ui.add_enabled(self.launch_task.is_none(), egui::Button::new(label))
                            })
                            .inner
                            .clicked()
                        {
                            let name = package.name.clone();
                            let file = desktop_file.clone();
                            self.launch_task =
                                Some(Task::spawn(ctx, move || backend::launch(&name, &file)));
                        }
                    }
                    if self.launch_task.is_some() {
                        ui.label(t!("Launching application…"));
                    }
                }
                if package.has_update() && ui.button(t!("View system updates")).clicked() {
                    self.page = Page::Updates;
                }
            });
        if !open {
            self.selected = None;
            self.detail_task = None;
            self.detail_index = None;
        }
    }

    fn system_preflight(&mut self, ctx: &egui::Context, action: Action) {
        let key = match action {
            Action::Install(i) | Action::Remove(i) => self.packages[i].name.clone(),
            Action::Upgrade => "upgrade".into(),
            Action::CleanOrphans => "clean-orphans".into(),
        };
        if self.optional_for != key {
            self.optional_for = key;
            self.optional_targets.clear();
            self.cascade_dependents = false;
        }
        let mut open = true;
        let mut prepare = false;
        egui::Window::new(action.title()).open(&mut open).default_width(500.0).vscroll(true).show(ctx, |ui| {
            match action {
                Action::Install(i) => {
                    let package = &self.packages[i];
                    ui.heading(trf!("Install {repository}/{name}", repository = package.repository, name = package.name));
                    ui.label(t!("Repositories will be refreshed and a full system upgrade included. Review the resolved changes before confirming."));
                    if !package.optional_dependencies.is_empty() {
                        ui.separator(); ui.label(t!("Optional dependencies (select any to include):"));
                        for dependency in package.optional_dependencies.lines() {
                            let dependency = dependency.trim();
                            let name = dependency.split(':').next().unwrap_or("").split([' ', '<', '>', '=']).next().unwrap_or("");
                            if !crate::protocol::valid_name(name) { continue; }
                            let mut selected = self.optional_targets.contains(name);
                            if ui.checkbox(&mut selected, dependency).changed() {
                                if selected { self.optional_targets.insert(name.into()); } else { self.optional_targets.remove(name); }
                            }
                        }
                    }
                }
                Action::Remove(i) => {
                    ui.heading(trf!("Remove {name}", name = self.packages[i].name));
                    ui.checkbox(&mut self.cascade_dependents, t!("Include packages that depend on this package"));
                    ui.label(t!("Dependent packages are preserved unless you select this option. The complete removal list will be shown before confirmation."));
                }
                Action::CleanOrphans => {
                    ui.heading(t!("Review orphan cleanup"));
                    ui.label(t!("The helper will identify dependency packages that no installed package needs as a required or optional dependency. Manually installed packages are preserved. Review the exact removal list before confirming."));
                }
                Action::Upgrade => {
                    ui.heading(t!("Full system upgrade"));
                    ui.label(t!("Refresh repositories, then review every package change, including libraries and command-line tools."));
                }
            }
            ui.separator();
            ui.label(t!("Your desktop will request administrator authorization to prepare the transaction. No package changes start until you confirm the prepared plan."));
            prepare = theme::primary_button(ui, t!("Authenticate and prepare"), !self.busy()).clicked();
        });
        if prepare {
            self.submit(ctx, action);
            self.pending = None;
        }
        if !open {
            self.pending = None;
        }
    }

    fn system_dialog(&mut self, ctx: &egui::Context) {
        let Some(session) = &mut self.system else {
            return;
        };
        let mut dismiss = false;
        egui::Window::new(t!("Package transaction")).default_width(650.0).default_height(560.0)
            .vscroll(true).show(ctx, |ui| {
                ui.label(&session.status);
                if let Some((success, message)) = &session.outcome {
                    ui.colored_label(if *success { theme::palette(ui).success } else { theme::palette(ui).danger }, message);
                    if !success { ui.label(t!(recovery::advice(message))); }
                } else {
                    ui.add(egui::ProgressBar::new(session.progress).fill(theme::palette(ui).accent).show_percentage());
                }
                if let Some((message, choices)) = session.question.clone() {
                    ui.separator(); ui.heading(t!("Package manager question")); ui.label(message);
                    for (choice, label) in choices.iter().enumerate() {
                        if ui.button(label).clicked() { session.send(HelperCommand::Answer { choice }); }
                    }
                }
                if let Some(plan) = &session.plan {
                    ui.separator(); ui.heading(t!("Review package changes"));
                    ui.label(trf!("Download: {download} MiB · Disk change: {disk} MiB", download = format!("{:.2}", plan.download_bytes as f64 / 1048576.0), disk = format!("{:+.2}", plan.disk_delta as f64 / 1048576.0)));
                    if let Some(free) = plan.root_free { ui.label(trf!("Free on installation filesystem: {space} GiB", space = format!("{:.2}", free as f64 / 1073741824.0))); }
                    if let Some(free) = plan.cache_free { ui.label(trf!("Free on package cache filesystem: {space} GiB", space = format!("{:.2}", free as f64 / 1073741824.0))); }
                    egui::ScrollArea::vertical().id_salt("transaction_changes").max_height(250.0)
                        .show_rows(ui, ui.text_style_height(&egui::TextStyle::Body), plan.changes.len(), |ui, rows| {
                        for index in rows {
                        let change = &plan.changes[index];
                        let text = match (&change.old, &change.new) {
                            (None, Some(new)) => trf!("Install {name} {version}", name = change.name, version = new),
                            (Some(old), None) => trf!("Remove {name} {version}", name = change.name, version = old),
                            (Some(old), Some(new)) => trf!("Change {name}: {old} → {new}", name = change.name, old = old, new = new),
                            _ => change.name.clone(),
                        };
                        ui.add(egui::Label::new(egui::RichText::new(&text).color(if change.new.is_none() { theme::palette(ui).danger } else { theme::palette(ui).text_primary })).truncate()).on_hover_text(text);
                    }
                    });
                    for decision in &plan.decisions { ui.label(decision); }
                    ui.label(t!("The transaction holds pacman's database lock. Confirm to apply exactly this plan, or cancel to release it."));
                    if theme::primary_button(ui, t!("Confirm package changes"), !session.cancel_requested).clicked() {
                        session.send(HelperCommand::Confirm {});
                    }
                }
                ui.separator();
                if session.outcome.is_some() {
                    dismiss = ui.button(t!("Dismiss transaction")).clicked();
                } else if session.committing {
                    ui.label(t!("Package changes have started. Cancellation is unavailable; keep the system running until this finishes."));
                } else if session.cancel_requested {
                    ui.label(t!("Cancel requested. Waiting for a safe stopping point; dismiss any open authorization dialog."));
                } else if ui.button(t!("Cancel preparation")).clicked() {
                    session.send(HelperCommand::Cancel {});
                }
                ui.collapsing(t!("Transaction log"), |ui| {
                    if ui.button(t!("Copy log")).clicked() {
                        ui.ctx().copy_text(session.logs.iter().cloned().collect::<Vec<_>>().join("\n"));
                    }
                    for line in &session.logs { ui.label(egui::RichText::new(line).monospace()); }
                });
            });
        if dismiss {
            self.system = None;
        }
    }

    fn transaction_preview(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else { return };
        if self.mode == Mode::System {
            self.system_preflight(ctx, action);
            return;
        }
        let mut open = true;
        let mut confirmed = false;
        let mut canceled = false;
        egui::Window::new(action.title()).open(&mut open)
            .collapsible(false).resizable(false).show(ctx, |ui| {
                ui.colored_label(theme::palette(ui).warning, t!("Demo only: confirmation changes sample data in this window."));
                ui.separator();
                match action {
                    Action::Install(i) => { ui.label(trf!("Install {name} {version}", name = self.packages[i].name, version = self.packages[i].version)); }
                    Action::Remove(i) => { ui.label(trf!("Remove {name}", name = self.packages[i].name)); }
                    Action::CleanOrphans => {
                        for package in self.packages.iter().filter(|p| p.orphan && p.installed.is_some()) { ui.label(trf!("Remove {name}", name = package.name)); }
                    }
                    Action::Upgrade => {
                        for package in self.packages.iter().filter(|p| p.has_update()) {
                            ui.label(trf!("{name}: {old} → {new}", name = package.name, old = package.installed.as_deref().unwrap_or(t!("Unknown")), new = package.version));
                        }
                    }
                }
                ui.separator();
                ui.label(t!("Real transactions will resolve dependencies, conflicts, sizes, and authorization before proceeding."));
                ui.horizontal(|ui| {
                    confirmed = if matches!(action, Action::Remove(_) | Action::CleanOrphans) {
                        theme::danger_button(ui, t!("Confirm demo removal")).clicked()
                    } else {
                        theme::primary_button(ui, t!("Confirm demo change"), true).clicked()
                    };
                    canceled = ui.button(t!("Cancel")).clicked();
                });
            });
        if confirmed {
            self.submit(ctx, action);
        }
        if confirmed || canceled || !open {
            self.pending = None;
        }
    }
}

impl eframe::App for Pacstore {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let had_tasks = self.busy() || self.config_save.is_some();
        self.poll_tasks(ctx);
        let go_search =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
        if go_search {
            self.page = Page::Search;
            self.search_focus = true;
        }
        for (key, page) in [
            (egui::Key::Num1, Page::Explore),
            (egui::Key::Num2, Page::Search),
            (egui::Key::Num3, Page::Installed),
            (egui::Key::Num4, Page::Updates),
        ] {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, key)) {
                self.page = page;
            }
        }
        if !self.busy()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5))
        {
            self.reload(ctx);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.pending = None;
            self.selected = None;
            self.detail_task = None;
            self.detail_index = None;
        }
        if self.system.as_ref().is_some_and(|s| s.outcome.is_none()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if ctx.input(|input| input.viewport().close_requested()) && had_tasks {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if let Some(session) = &mut self.system
                && !session.committing
                && session.outcome.is_none()
            {
                session.send(HelperCommand::Cancel {});
            }
            self.close_after_tasks = true;
            self.notice = Some(t!("Finishing background work before closing…").into());
        }
        if self.close_after_tasks && !self.busy() && self.config_save.is_none() {
            self.close_after_tasks = false;
            if self.error.is_none() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(t!("Pacstore"));
                ui.colored_label(theme::palette(ui).info, t!("FIRST RELEASE PROTOTYPE"));
            });
            ui.label(if self.source.is_empty() {
                t!("Loading package data…")
            } else {
                &self.source
            });
            ui.add_enabled_ui(self.config_save.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("Theme:"));
                    let previous = self.theme;
                    for choice in [Theme::System, Theme::Light, Theme::Dark] {
                        ui.selectable_value(&mut self.theme, choice, choice.label());
                    }
                    let old_scale = self.scale;
                    egui::ComboBox::from_label(t!("Scale"))
                        .selected_text(self.scale.label())
                        .show_ui(ui, |ui| {
                            for scale in Scale::ALL {
                                ui.selectable_value(&mut self.scale, scale, scale.label());
                            }
                        });
                    if old_scale != self.scale || self.theme != previous {
                        ctx.set_zoom_factor(self.scale.factor());
                        self.theme.apply(&ctx);
                        self.save_settings(&ctx);
                    }
                    ui.menu_button(t!("Keyboard shortcuts"), |ui| {
                        ui.label(t!("Ctrl+F: search packages"));
                        ui.label(t!("Ctrl+1–4: Explore, Search, Installed, Updates"));
                        ui.label(t!("F5: reload catalog"));
                        ui.label(t!("Tab / Shift+Tab: move between controls"));
                        ui.label(t!("Enter / Space: activate the focused control"));
                        ui.label(t!("Escape: close package details or an unstarted preview"));
                    });
                });
            });
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                for page in [Page::Explore, Page::Search, Page::Installed, Page::Updates] {
                    if theme::selectable(ui, self.page == page, page.title()).clicked() {
                        self.page = page;
                    }
                }
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(if self.mode == Mode::Demo {
                            t!("Reset demo")
                        } else {
                            t!("Reload catalog")
                        }),
                    )
                    .clicked()
                {
                    self.reload(&ctx);
                    self.notice = None;
                }
            });
            if !self.warnings.is_empty() {
                ui.collapsing(
                    trf!("Catalog notices ({count})", count = self.warnings.len()),
                    |ui| {
                        for warning in &self.warnings {
                            ui.colored_label(theme::palette(ui).warning, warning);
                        }
                    },
                );
            }
            if self.loaded_at > 0 {
                let age = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(self.loaded_at);
                ui.weak(if age < 60 {
                    t!("Catalog loaded less than a minute ago").into()
                } else {
                    trf!("Catalog loaded {minutes} minutes ago", minutes = age / 60)
                });
            }
            if self.config_save.is_some() {
                ui.label(t!("Saving preferences…"));
            }
            if let Some(error) = &self.error {
                let message = error.to_string();
                ui.group(|ui| {
                    ui.colored_label(
                        theme::palette(ui).danger,
                        egui::RichText::new(t!("Operation failed")).strong(),
                    );
                    ui.label(&message);
                    ui.label(t!(recovery::advice(&message)));
                    if ui.button(t!("Copy error")).clicked() {
                        ui.ctx().copy_text(message.clone());
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button(t!("Dismiss error")).clicked() {
                            self.error = None;
                        }
                        if ui
                            .add_enabled(!self.busy(), egui::Button::new(t!("Reload catalog")))
                            .clicked()
                        {
                            self.error = None;
                            self.reload(&ctx);
                        }
                    });
                });
            }
            if let Some(notice) = self.notice.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(theme::palette(ui).info, notice);
                    if ui.small_button(t!("Dismiss")).clicked() {
                        self.notice = None;
                    }
                });
            }
            ui.separator();
            self.content(ui);
        });
        self.details(&ctx);
        self.transaction_preview(&ctx);
        self.system_dialog(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::sample_packages;

    #[test]
    fn installed_scopes_and_orphan_preview_preserve_manual_packages() {
        let mut app = Pacstore {
            page: Page::Installed,
            packages: sample_packages(),
            mode: Mode::Demo,
            ..Default::default()
        };
        app.installed_filter = 2;
        assert_eq!(app.packages.iter().filter(|p| app.visible(p)).count(), 3);
        app.installed_scope = 1;
        assert_eq!(app.packages.iter().filter(|p| app.visible(p)).count(), 1);
        app.installed_scope = 2;
        assert_eq!(app.packages.iter().filter(|p| app.visible(p)).count(), 4);
        app.installed_scope = 3;
        assert_eq!(app.packages.iter().filter(|p| app.visible(p)).count(), 1);
        app.pending = Some(Action::CleanOrphans);
        assert!(app.packages[5].installed.is_some());
        app.submit(&egui::Context::default(), Action::CleanOrphans);
        finish_tasks(&mut app);
        assert!(app.packages[5].installed.is_none());
        assert!(app.packages[0].installed.is_some());
    }

    #[test]
    fn search_shortcut_focuses_a_labeled_input_and_large_catalog_is_virtualized() {
        use eframe::App;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install(&ctx);
        let mut frame = eframe::Frame::_new_kittest();
        let packages = (0..16000)
            .map(|i| Package {
                name: format!("application-{i}"),
                desktop_app: true,
                repository: "extra".into(),
                ..Default::default()
            })
            .collect();
        let mut app = Pacstore {
            packages,
            ..Default::default()
        };
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            app.logic(ui.ctx(), &mut frame);
            app.ui(ui, &mut frame);
        });
        output.textures_delta.clear();
        assert!(app.page == Page::Search);
        assert!(ctx.memory(|memory| memory.focused()).is_some());
        assert!(
            output.shapes.len() < 5000,
            "Only visible rows should be painted"
        );
        assert!(output.platform_output.accesskit_update.is_some());
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(tree.nodes.iter().any(|(_, node)| {
            node.label()
                .is_some_and(|label| label.starts_with("Details for "))
        }));
    }

    #[test]
    fn repository_category_and_install_filters_combine_but_never_hide_updates() {
        let mut packages = sample_packages();
        packages[0].category = "Network; WebBrowser".into();
        let mut app = Pacstore {
            page: Page::Search,
            packages,
            all_packages: true,
            repository_filter: "extra".into(),
            category_filter: "WebBrowser".into(),
            installed_filter: 1,
            ..Default::default()
        };
        assert!(app.visible(&app.packages[0]));
        assert!(!app.visible(&app.packages[1]));
        app.installed_filter = 2;
        assert!(!app.visible(&app.packages[0]));
        app.page = Page::Updates;
        app.repository_filter = "missing".into();
        assert_eq!(app.packages.iter().filter(|p| app.visible(p)).count(), 2);
    }

    fn finish_tasks(app: &mut Pacstore) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.busy() {
            app.poll_tasks(&egui::Context::default());
            assert!(std::time::Instant::now() < deadline, "worker timed out");
            std::thread::yield_now();
        }
        assert!(app.error.is_none());
    }

    #[test]
    fn catalog_worker_delivers_data() {
        let ctx = egui::Context::default();
        let mut app = Pacstore::new(&ctx, None, Settings::default(), None, Mode::Demo);
        finish_tasks(&mut app);
        assert_eq!(app.packages.len(), sample_packages().len());
    }

    #[test]
    fn failed_transaction_preserves_catalog_and_releases_busy_state() {
        let ctx = egui::Context::default();
        let mut app = Pacstore {
            packages: sample_packages(),
            mode: Mode::Demo,
            ..Default::default()
        };
        app.transaction = Some(Task::spawn(&ctx, || {
            Err(AppError::InvalidRequest("stale request".into()))
        }));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.busy() {
            app.poll_tasks(&egui::Context::default());
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(matches!(app.error, Some(AppError::InvalidRequest(_))));
        assert_eq!(app.packages[0].installed.as_deref(), Some("demo-1"));
        app.error = None;
        app.reload(&ctx);
        finish_tasks(&mut app);
    }

    #[test]
    fn busy_state_rejects_overlapping_operations() {
        let ctx = egui::Context::default();
        let mut app = Pacstore {
            packages: sample_packages(),
            mode: Mode::Demo,
            ..Default::default()
        };
        app.submit(&ctx, Action::Install(1));
        app.submit(&ctx, Action::Remove(0));
        app.reload(&ctx);
        assert!(app.loading.is_none());
        finish_tasks(&mut app);
        assert!(app.packages[1].installed.is_some());
        assert!(app.packages[0].installed.is_some());
    }

    #[test]
    fn loading_empty_and_error_screens_render() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();
        for mut app in [
            Pacstore::default(),
            Pacstore {
                loading: Some(Task::spawn(&ctx, || Ok(Catalog::default()))),
                ..Default::default()
            },
            Pacstore {
                transaction: Some(Task::spawn(&ctx, || Ok(Vec::new()))),
                ..Default::default()
            },
            Pacstore {
                error: Some(AppError::WorkerStopped),
                ..Default::default()
            },
        ] {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui, &mut frame));
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
        }
    }

    #[test]
    fn screens_and_previews_render_without_changing_packages() {
        use eframe::App;
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.set_fonts(crate::fonts::system_fonts());
        let mut frame = eframe::Frame::_new_kittest();
        let mut app = Pacstore {
            packages: sample_packages(),
            mode: Mode::Demo,
            ..Default::default()
        };
        for variant in [egui::Theme::Dark, egui::Theme::Light] {
            ctx.set_theme(variant);
            for page in [Page::Explore, Page::Search, Page::Installed, Page::Updates] {
                app.page = page;
                for action in [Action::Install(1), Action::Remove(0), Action::Upgrade] {
                    app.selected = Some(0);
                    app.pending = Some(action);
                    let mut output =
                        ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui, &mut frame));
                    assert!(!output.shapes.is_empty());
                    // This headless pass has no renderer to consume texture uploads.
                    output.textures_delta.clear();
                    assert!(app.packages[1].installed.is_none());
                    assert_eq!(app.packages.iter().filter(|p| p.has_update()).count(), 2);
                }
            }
        }
    }

    #[test]
    fn full_upgrade_includes_hidden_libraries() {
        let mut app = Pacstore {
            packages: sample_packages(),
            mode: Mode::Demo,
            page: Page::Updates,
            ..Default::default()
        };
        assert_eq!(app.packages.iter().filter(|p| app.visible(p)).count(), 2);
        app.submit(&egui::Context::default(), Action::Upgrade);
        finish_tasks(&mut app);
        assert!(!app.packages.iter().any(Package::has_update));
        assert!(
            app.packages
                .iter()
                .find(|p| p.name == "gimp")
                .unwrap()
                .installed
                .is_none()
        );
    }

    #[test]
    fn demo_install_and_remove_change_installed_view() {
        let mut app = Pacstore {
            packages: sample_packages(),
            mode: Mode::Demo,
            page: Page::Installed,
            ..Default::default()
        };
        assert!(!app.visible(&app.packages[1]));
        app.submit(&egui::Context::default(), Action::Install(1));
        finish_tasks(&mut app);
        assert!(app.visible(&app.packages[1]));
        app.submit(&egui::Context::default(), Action::Remove(1));
        finish_tasks(&mut app);
        assert!(!app.visible(&app.packages[1]));
    }
}
