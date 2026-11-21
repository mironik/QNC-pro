//! The Advanced settings (copied from the old QNC's `project_advanced.rs`): its sections come
//! from the layout contract (fields drawn by the kit `qnc-settings-form`); this panel only
//! composes the blocks that are more than fields: the keyboard preset, the export preset, the
//! interface language and the workflow applications.

use eframe::egui::{self, RichText};
use qnc_desktop_theme::Theme;
use qnc_project_layout::{AdvancedSection, AdvancedSettings};
use qnc_project_session::ProjectIntent;
use qnc_settings_path::{path_string, set_string_path};

use crate::panel::{bar, Panel};

/// The Advanced bar: opens and closes everything technical of the panel (the journalist sees
/// the story data and the template; the rest only under Advanced).
pub(crate) fn header(ui: &mut egui::Ui, content_w: f32, advanced: &AdvancedSettings, theme: &Theme, open: &mut bool) {
    ui.set_max_width(content_w);
    let title = RichText::new(&advanced.title).size(13.0).strong().color(theme.text);
    if bar(ui, content_w, theme.surface_alt, theme, title, *open) {
        *open = !*open;
    }
}

/// The look of a field grid of the panel, from the contract.
fn grid_style(advanced: &AdvancedSettings, theme: &Theme) -> qnc_settings_form::GridStyle {
    let grid = &advanced.grid;
    qnc_settings_form::GridStyle { label_color: theme.text_muted, label_size: grid.label_size, min_cell_width: grid.min_cell_width, gap: grid.gap, cell_height: grid.cell_height, label_beside: None }
}

/// The keyboard presets of the catalog (id, name), the ones of the contract's order first.
fn keyboard_presets(advanced: &AdvancedSettings) -> Vec<(String, String)> {
    let catalog = qnc_actions::catalog();
    let mut ids: Vec<String> = catalog.presets().map(str::to_string).collect();
    ids.sort_by_key(|id| advanced.keyboard_preset_order.iter().position(|first| first == id).unwrap_or(usize::MAX));
    ids.into_iter().map(|id| (id.clone(), id)).collect()
}

pub(crate) fn sections(panel: &mut Panel<'_>, ui: &mut egui::Ui, content_w: f32) {
    let (theme, advanced) = (panel.theme, &panel.layout.advanced_settings);
    let metrics = &panel.layout.right_settings_panel;
    let style = grid_style(advanced, theme);
    let presets = keyboard_presets(advanced);
    let applications = panel.session.component.applications.view();
    ui.spacing_mut().item_spacing.y = metrics.section_gap;
    let mut changed = false;
    let mut chosen = None;
    for section in &advanced.sections {
        qnc_settings_rows::section(ui, content_w, &section.title, metrics, theme, |ui| {
            let width = ui.available_width();
            let one = qnc_settings_form::cell_width(width, 1, &style);
            match section.block.as_deref() {
                Some("keyboard_preset") => {
                    let path = "keyboard_shortcuts.active_preset";
                    let current = path_string(&panel.session.draft_settings, path, "qnc");
                    if let Some(preset) = qnc_settings_form::choice_cell(ui, one, &section.label, "pts_kbd_preset", &current, &presets, &style) {
                        set_string_path(&mut panel.session.draft_settings, path, preset);
                        changed = true;
                    }
                }
                Some("export_preset") => {
                    let mut presets: Vec<_> = qnc_export_preset::presets(&panel.session.draft_settings).into_iter().map(|preset| (preset.id, preset.name)).collect();
                    presets.push((qnc_export_preset::MANUAL_PRESET_ID.to_string(), advanced.texts.manual_preset.clone()));
                    let current = path_string(&panel.session.draft_settings, "export.preset", qnc_export_preset::MANUAL_PRESET_ID);
                    let selected = qnc_settings_form::choice_cell(ui, one, &section.label, "pts_export_preset", &current, &presets, &style).unwrap_or(current);
                    changed |= qnc_export_preset::apply_preset(&mut panel.session.draft_settings, &selected);
                }
                Some("applications") => chosen = chosen.take().or(workflow(ui, advanced, theme, &applications)),
                other => qnc_language_choice::section(ui, other, panel.root, one, &section.label, &style),
            }
            changed |= qnc_settings_form::field_grid(ui, width, &mut panel.session.draft_settings, &section.fields, &advanced.options, &style);
            if let Some(then) = section.then.as_ref().filter(|then| then.when.holds(&panel.session.draft_settings)) {
                ui.add_space(metrics.section_gap);
                changed |= qnc_settings_form::field_grid(ui, ui.available_width(), &mut panel.session.draft_settings, &then.fields, &advanced.options, &style);
                if then.save_preset {
                    changed |= save_preset(ui, section, advanced, theme, panel.session);
                }
            }
        });
    }
    if changed {
        panel.session.status = panel.layout.texts.settings_changed.clone();
    }
    panel.intents.extend(chosen);
}

/// The row that keeps the manual export settings as a preset of the project.
fn save_preset(ui: &mut egui::Ui, _section: &AdvancedSection, advanced: &AdvancedSettings, theme: &Theme, session: &mut qnc_project_session::ProjectSession) -> bool {
    let mut changed = false;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add(qnc_ui_kit::text_field(&mut session.export_preset_draft_name).desired_width((ui.available_width() - 160.0).max(120.0)).hint_text(&advanced.texts.new_preset_hint));
        let can_save = !session.export_preset_draft_name.trim().is_empty();
        if qnc_settings_rows::accent_btn(ui, &advanced.texts.save_preset, can_save, theme).clicked() && qnc_export_preset::save_custom_preset(&mut session.draft_settings, &session.export_preset_draft_name) {
            session.export_preset_draft_name.clear();
            changed = true;
        }
    });
    changed
}

/// The applications of the workflow, one column per priority group.
fn workflow(ui: &mut egui::Ui, advanced: &AdvancedSettings, theme: &Theme, applications: &qnc_application_selection::ApplicationSelectionView) -> Option<ProjectIntent> {
    ui.spacing_mut().item_spacing.y = 6.0;
    let columns: Vec<_> = applications
        .groups
        .iter()
        .map(|group| {
            let mut options: Vec<_> = group.choices.iter().map(|choice| qnc_ui_kit::OptionItem { id: Some(choice.application_id.clone()), label: choice.label.clone(), selected: choice.selected }).collect();
            if !applications.required_groups.contains(&group.priority_group) {
                options.push(qnc_ui_kit::OptionItem { id: None, label: advanced.texts.no_selection.clone(), selected: group.no_selection });
            }
            qnc_ui_kit::OptionColumn { id: group.priority_group.clone(), label: advanced.texts.group.replace("{group}", &group.priority_group), options }
        })
        .collect();
    let style = qnc_ui_kit::OptionColumnsStyle { font_size: theme.font_ui, text: theme.text, min_column_width: 160.0, column_gap: 12.0, row_gap: 6.0 };
    let chosen = qnc_ui_kit::show_option_columns(ui, "project_workflow_groups", &columns, &style).map(|selected| ProjectIntent::SelectWorkflowGroup { priority_group: selected.column_id, application_id: selected.option_id });
    if applications.loading {
        ui.label(&advanced.texts.loading_catalog);
    }
    if let Some(error) = &applications.error {
        ui.label(error);
    }
    chosen
}
