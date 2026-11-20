//! The editor of a sheet: its name, the lock, each field (name, kind, choices, order, removal) and a new field.

use eframe::egui::{self, RichText};
use qnc_desktop_theme::Theme;
use qnc_settings_form::FieldKind;

use crate::{button, label, new_field_in, code_of, right_row, row_box, Answer, SheetOverlay, LABEL_WIDTH, ROW_GAP, ROW_HEIGHT};

const KINDS: [(FieldKind, &str); 4] = [
    (FieldKind::Text, "sheet.kind_text"),
    (FieldKind::Choice, "sheet.kind_choice"),
    (FieldKind::Duration, "sheet.kind_duration"),
    (FieldKind::Integer, "sheet.kind_number"),
];

/// The editor of the sheet: its name, the lock, each field (name, kind, choices, order,
/// removal) and a new field; Save sheet keeps it on this computer, Back leaves it as it was.
pub(crate) fn editor(ui: &mut egui::Ui, overlay: &mut SheetOverlay, theme: &Theme) {
    let width = ui.available_width();
    ui.spacing_mut().interact_size.y = ROW_HEIGHT;
    ui.spacing_mut().item_spacing.y = ROW_GAP / 2.0;
    let Some(sheet) = overlay.editing.as_mut() else { return };
    ui.horizontal(|ui| {
        label(ui, theme, qnc_text::text("sheet.name"));
        ui.add_enabled(!sheet.locked, row_box(&mut sheet.name, width - LABEL_WIDTH - ROW_GAP));
    });
    qnc_ui_kit::check_box(ui, &mut sheet.locked, qnc_text::text("sheet.locked"), true);
    ui.add_space(ROW_GAP);
    fields_editor(ui, "qnc_story_sheet", &mut sheet.fields, &mut sheet.options, &mut overlay.new_label, sheet.locked, theme);
    if !overlay.message.is_empty() {
        ui.label(RichText::new(&overlay.message).color(theme.danger));
    }
}

/// The editor of a list of fields (user: the same for a story sheet and for the fields
/// of a kind of script block): each field's name, kind, choices, order and removal, and a new
/// field whose code comes from its name. Nothing changes while `locked`.
pub fn fields_editor(
    ui: &mut egui::Ui,
    salt: &str,
    fields: &mut Vec<qnc_settings_form::FieldSpec>,
    options: &mut qnc_settings_form::OptionSets,
    new_label: &mut String,
    locked: bool,
    theme: &Theme,
) {
    let width = ui.available_width();
    let field_label_w = width * 0.4;
    let (mut up, mut down, mut gone) = (None, None, None);
    let count = fields.len();
    for (index, field) in fields.iter_mut().enumerate() {
        ui.add_enabled_ui(!locked, |ui| {
            ui.horizontal(|ui| {
                ui.add(row_box(&mut field.label, field_label_w));
                let kind = KINDS.iter().find(|(kind, _)| *kind == field.kind).map_or("", |(_, code)| qnc_text::text(code));
                egui::ComboBox::from_id_salt((salt, "kind", index)).selected_text(kind).width(110.0).show_ui(ui, |ui| {
                    for (kind, code) in KINDS {
                        ui.selectable_value(&mut field.kind, kind, qnc_text::text(code));
                    }
                });
                up = up.or((button(ui, "▲", index > 0, theme)).then_some(index));
                down = down.or((button(ui, "▼", index + 1 < count, theme)).then_some(index));
                gone = gone.or((button(ui, qnc_text::text("sheet.remove"), true, theme)).then_some(index));
            });
            if field.kind == FieldKind::Choice {
                choices_row(ui, field, options, width);
            }
        });
    }
    if let Some(index) = up {
        fields.swap(index, index - 1);
    }
    if let Some(index) = down {
        fields.swap(index, index + 1);
    }
    if let Some(index) = gone {
        fields.remove(index);
    }
    ui.add_enabled_ui(!locked, |ui| {
        ui.horizontal(|ui| {
            ui.add(row_box(new_label, field_label_w).hint_text(qnc_text::text("sheet.new_field_hint")));
            if button(ui, qnc_text::text("sheet.add_field"), !new_label.trim().is_empty(), theme) {
                let field = new_field_in(fields, new_label);
                fields.push(field);
                new_label.clear();
            }
        });
    });
}

/// Save sheet (rightmost) and Back of the editor; the overlay stays open either way.
pub(crate) fn editor_buttons(ui: &mut egui::Ui, overlay: &mut SheetOverlay, theme: &Theme) -> Option<Answer> {
    let (mut keep, mut back) = (false, false);
    right_row(ui, |ui| {
        keep = qnc_ui_kit::strong_button(ui, qnc_text::text("sheet.save"), true, theme.accent, theme.chrome_control_height).clicked();
        back = button(ui, qnc_text::text("sheet.back"), true, theme);
    });
    if keep {
        if let Some(Err(error)) = overlay.editing.take().map(|sheet| overlay.sheets.keep(sheet)) {
            overlay.message = error;
        }
    } else if back {
        overlay.editing = None;
    }
    None
}

/// The choices of a choice field, typed as names separated by commas.
fn choices_row(ui: &mut egui::Ui, field: &mut qnc_settings_form::FieldSpec, options: &mut qnc_settings_form::OptionSets, width: f32) {
    let key = field.options.clone().unwrap_or_else(|| field.path.clone());
    let mut typed = options.get(&key).map(|choices| choices.iter().map(|(_, label)| label.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        let edited = ui.add(row_box(&mut typed, width - 24.0).hint_text(qnc_text::text("sheet.choices_hint"))).changed();
        if edited || field.options.is_none() {
            let choices = typed.split(',').map(str::trim).filter(|label| !label.is_empty()).map(|label| (code_of(label), label.to_string())).collect();
            options.insert(key.clone(), choices);
            field.options = Some(key);
        }
    });
}

