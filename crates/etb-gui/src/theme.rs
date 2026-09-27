//! Visual settings.
//!
//! Larger text and taller controls than a developer tool would use. For the person
//! this is built for that is a functional requirement, not decoration.

pub const BASE_TEXT_SIZE: f32 = 17.0;
pub const BUTTON_HEIGHT: f32 = 38.0;
pub const BIG_BUTTON_HEIGHT: f32 = 52.0;

pub fn apply(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle};

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(26.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(BASE_TEXT_SIZE, FontFamily::Proportional),
            ),
            (
                TextStyle::Button,
                FontId::new(BASE_TEXT_SIZE, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(15.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.interact_size.y = 28.0;
    });
}

/// The one button that matters.
pub fn run_button_fill(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(34, 110, 60)
    } else {
        egui::Color32::from_rgb(38, 132, 70)
    }
}

pub fn error_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(240, 120, 120)
    } else {
        egui::Color32::from_rgb(178, 34, 34)
    }
}

pub fn warning_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(226, 178, 84)
    } else {
        egui::Color32::from_rgb(150, 100, 0)
    }
}

pub fn success_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(120, 200, 140)
    } else {
        egui::Color32::from_rgb(20, 110, 50)
    }
}
