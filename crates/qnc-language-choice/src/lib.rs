//! The choice of the interface language (user: "in Project there should be an option to choose
//! the language of the interface"; a setting of this computer, it applies when QNC starts
//! again). A settings panel places it as its `ui_language` section: the languages QNC has
//! dictionaries for, the one this computer chose, and a note while that is not the language
//! QNC runs in. The choice is written as this computer's setting through `qnc-settings`, never
//! into a project or its database.

use std::path::Path;

use eframe::egui;
use qnc_settings_form::GridStyle;

pub const MODULE_ID: &str = "qnc.module.language-choice";

/// The name of the section this block draws.
pub const SECTION: &str = "ui_language";

/// Draws the choice when `block` is this block's section; any other section is not its own.
pub fn section(ui: &mut egui::Ui, block: Option<&str>, root: &Path, width: f32, label: &str, style: &GridStyle) {
    if block != Some(SECTION) {
        return;
    }
    let chosen = qnc_settings::chosen_language(root);
    let languages: Vec<(String, String)> = qnc_text::languages().into_iter().map(|(code, name)| (code.to_string(), name)).collect();
    if let Some(language) = qnc_settings_form::choice_cell(ui, width, label, "qnc_ui_language", &chosen, &languages, style) {
        if let Err(error) = qnc_settings::set_language(root, &language) {
            qnc_diagnostics::log(format!("ui-language refused: {error}"));
        }
    }
    if qnc_settings::chosen_language(root) != qnc_text::current_language() {
        ui.label(egui::RichText::new(qnc_text::text("project.language_after_restart")).size(style.label_size).color(style.label_color));
    }
}
