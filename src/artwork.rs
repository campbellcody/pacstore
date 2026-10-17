//! Bounded, lazy artwork loading: no filesystem access or image decoding in paint.
use crate::{backend, packages::Package, task::Task, theme};
use eframe::egui::{self, TextureHandle, Ui};
use std::collections::HashMap;

#[derive(Default)]
pub struct Artwork {
    clock: u64,
    entries: HashMap<String, (Option<TextureHandle>, u64)>,
    pending: Vec<(String, Task<Option<egui::ColorImage>>)>,
}
impl Artwork {
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.clock = self.clock.wrapping_add(1);
        let mut index = 0;
        while index < self.pending.len() {
            if let Some(result) = self.pending[index].1.poll() {
                let (path, _) = self.pending.swap_remove(index);
                let texture = result
                    .ok()
                    .flatten()
                    .map(|image| ctx.load_texture(&path, image, egui::TextureOptions::LINEAR));
                if self.entries.len() >= 128
                    && let Some(oldest) = self
                        .entries
                        .iter()
                        .min_by_key(|(_, (_, used))| used)
                        .map(|(path, _)| path.clone())
                {
                    self.entries.remove(&oldest);
                }
                self.entries.insert(path, (texture, self.clock));
            } else {
                index += 1;
            }
        }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.pending.clear();
    }
    pub fn show(&mut self, ui: &mut Ui, package: &Package, size: f32) {
        if let Some(path) = &package.icon {
            if let Some((texture, used)) = self.entries.get_mut(path) {
                *used = self.clock;
                if let Some(texture) = texture {
                    ui.add(
                        egui::Image::new((texture.id(), egui::vec2(size, size)))
                            .alt_text(package.display_name()),
                    );
                    return;
                }
            } else if self.pending.len() < 8 && !self.pending.iter().any(|(p, _)| p == path) {
                let file = path.clone();
                self.pending.push((
                    path.clone(),
                    Task::spawn(ui.ctx(), move || Ok(backend::icon(&file))),
                ));
            }
        }
        let letter = package
            .display_name()
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        ui.add_sized(
            [size, size],
            egui::Label::new(
                egui::RichText::new(letter)
                    .size(size * 0.5)
                    .color(theme::palette(ui).accent),
            ),
        )
        .on_hover_text(package.display_name());
    }
}
