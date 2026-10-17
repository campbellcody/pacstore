//! Shared semantic colors for both system-selected themes.
use eframe::egui::{self, Color32, Response, RichText, Stroke, Ui};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_selected: Color32,
    pub border: Color32,
    pub control_border: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
}

pub const DARK: Palette = Palette {
    background: rgb(0x081525),
    surface: rgb(0x102338),
    surface_hover: rgb(0x18344D),
    surface_selected: rgb(0x16445C),
    border: rgb(0x31516A),
    control_border: rgb(0x698DA5),
    text_primary: rgb(0xE7F3FC),
    text_secondary: rgb(0xA5BED3),
    accent: rgb(0x38CFF2),
    accent_hover: rgb(0x72E2FA),
    accent_pressed: rgb(0x16A8D0),
    on_accent: rgb(0x062033),
    success: rgb(0x6EE7B7),
    warning: rgb(0xFCD34D),
    danger: rgb(0xFDA4AF),
    info: rgb(0x7DD3FC),
};
pub const LIGHT: Palette = Palette {
    background: rgb(0xF2F8FC),
    surface: rgb(0xFFFFFF),
    surface_hover: rgb(0xE3F1FA),
    surface_selected: rgb(0xD2EDF8),
    border: rgb(0xBDD1DF),
    control_border: rgb(0x56758C),
    text_primary: rgb(0x102A43),
    text_secondary: rgb(0x486581),
    accent: rgb(0x006A91),
    accent_hover: rgb(0x005777),
    accent_pressed: rgb(0x00445E),
    on_accent: rgb(0xFFFFFF),
    success: rgb(0x146C43),
    warning: rgb(0x8A4B00),
    danger: rgb(0xB4233B),
    info: rgb(0x075985),
};

pub fn palette(ui: &Ui) -> Palette {
    if ui.visuals().dark_mode { DARK } else { LIGHT }
}

fn visuals(dark: bool) -> egui::Visuals {
    let p = if dark { DARK } else { LIGHT };
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = None;
    v.weak_text_color = Some(p.text_secondary);
    v.panel_fill = p.background;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.extreme_bg_color = p.background;
    v.text_edit_bg_color = Some(p.surface);
    v.faint_bg_color = p.surface_hover;
    v.code_bg_color = p.surface_hover;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.selection.bg_fill = p.surface_selected;
    v.selection.stroke = Stroke::new(1.0, p.text_primary);
    for (widget, fill, stroke) in [
        (
            &mut v.widgets.noninteractive,
            p.surface,
            Stroke::new(1.0, p.border),
        ),
        (
            &mut v.widgets.inactive,
            p.surface,
            Stroke::new(1.0, p.control_border),
        ),
        (
            &mut v.widgets.hovered,
            p.surface_hover,
            Stroke::new(1.0, p.accent),
        ),
        (
            &mut v.widgets.active,
            p.surface_selected,
            Stroke::new(2.0, p.accent),
        ),
        (
            &mut v.widgets.open,
            p.surface_selected,
            Stroke::new(1.0, p.accent),
        ),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = stroke;
        widget.fg_stroke = Stroke::new(1.0, p.text_primary);
    }
    v
}

pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
    ctx.set_visuals_of(egui::Theme::Light, visuals(false));
    ctx.set_theme(egui::ThemePreference::System);
}

fn focus_ring(ui: &Ui, response: &Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(3.0),
            4.0,
            Stroke::new(2.0, palette(ui).accent),
            egui::StrokeKind::Outside,
        );
    }
}

pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let p = palette(ui);
    let response = ui
        .scope(|ui| {
            if enabled {
                let v = ui.visuals_mut();
                for (widget, fill) in [
                    (&mut v.widgets.inactive, p.accent),
                    (&mut v.widgets.hovered, p.accent_hover),
                    (&mut v.widgets.active, p.accent_pressed),
                ] {
                    widget.bg_fill = fill;
                    widget.weak_bg_fill = fill;
                    widget.fg_stroke.color = p.on_accent;
                    widget.bg_stroke = Stroke::NONE;
                }
            }
            ui.add_enabled(enabled, egui::Button::new(label))
        })
        .inner;
    focus_ring(ui, &response);
    response
}

pub fn danger_button(ui: &mut Ui, label: &str) -> Response {
    let p = palette(ui);
    let response = ui.add(
        egui::Button::new(RichText::new(label).color(p.danger)).stroke(Stroke::new(1.0, p.danger)),
    );
    focus_ring(ui, &response);
    response
}

pub fn selectable(ui: &mut Ui, selected: bool, label: impl Into<egui::WidgetText>) -> Response {
    let response = ui.selectable_label(selected, label);
    if selected {
        ui.painter().line_segment(
            [response.rect.left_bottom(), response.rect.right_bottom()],
            Stroke::new(2.0, palette(ui).accent),
        );
    }
    focus_ring(ui, &response);
    response
}

/// Repeated detail controls expose the package name to assistive technology.
pub fn details_button(ui: &mut Ui, name: &str) -> Response {
    let response = ui.button(crate::t!("Details"));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::accesskit::Role::Button,
            ui.is_enabled(),
            crate::trf!("Details for {name}", name = name),
        )
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(color: Color32) -> f64 {
        [color.r(), color.g(), color.b()]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(c, weight)| {
                let c = c as f64 / 255.0;
                (if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }) * weight
            })
            .sum()
    }
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn normal_text_and_semantic_colors_remain_readable_in_both_themes() {
        for p in [DARK, LIGHT] {
            for text in [p.text_primary, p.text_secondary] {
                for surface in [p.background, p.surface, p.surface_hover, p.surface_selected] {
                    assert!(contrast(text, surface) >= 4.5);
                }
            }
            for fill in [p.accent, p.accent_hover, p.accent_pressed] {
                assert!(contrast(p.on_accent, fill) >= 4.5);
            }
            for status in [p.success, p.warning, p.danger, p.info] {
                assert!(contrast(status, p.surface) >= 4.5);
            }
            assert!(contrast(p.control_border, p.surface) >= 3.0);
        }
    }
}
