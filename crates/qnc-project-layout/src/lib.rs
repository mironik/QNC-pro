//! The layout of the Project form is data (`contracts/ui/project.layout.json`, the old QNC's
//! Project contract): the window, the board measures, the project list, the settings panel,
//! the AI switches, the slots of the settings panel, the Advanced settings as a schema and
//! every text of the form (codes of the dictionary, resolved in the language of this
//! computer). The form and its panels draw with these; no measure or text lives in code. The
//! look of the form (`qnc-form-look`) may put its measures into the contract before it is read.

use serde::Deserialize;
use serde_json::Value;

pub use qnc_field_spec::{FieldSpec, OptionSets};

pub const CONTRACT: &str = include_str!("../../../contracts/ui/project.layout.json");

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Window {
    pub width: f32,
    pub height: f32,
}

/// The measures of the standard board (the same fields the board kit takes).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Board {
    pub left_ratio: f32,
    pub divider_width: f32,
    pub left_min_width: f32,
    pub right_min_width: f32,
    pub monitor_height: f32,
    pub head_height: f32,
    pub dock_height: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectListMetrics {
    pub panel_pad: f32,
    pub title: String,
    pub title_row_height: f32,
    pub below_title_pad: f32,
    pub row_height: f32,
    pub row_gap: f32,
    pub delete_column_width: f32,
    pub column_gap: f32,
    pub empty_text: String,
    pub ready_text: String,
}

/// The measures of the settings panel (the rows kit draws with them).
pub use qnc_field_spec::PanelMetrics as SettingsPanelMetrics;

/// A checkbox of the AI box: a dotted path in the settings and its on/off values.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiSwitch {
    pub path: String,
    pub on: Value,
    pub off: Value,
    pub default_on: bool,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PtsSlots {
    pub fixed_order: Vec<String>,
    pub scroll_order: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedSettings {
    pub title: String,
    pub grid: AdvancedGrid,
    pub options: OptionSets,
    pub sections: Vec<AdvancedSection>,
    /// The keyboard presets shown first, in this order (the others follow by id).
    pub keyboard_preset_order: Vec<String>,
    pub texts: AdvancedTexts,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedGrid {
    pub label_size: f32,
    pub min_cell_width: f32,
    pub gap: f32,
    pub cell_height: f32,
}

/// A section: its fields, or one of the blocks the form composes (`keyboard_preset`,
/// `export_preset`, `applications`, `ui_language`), and fields shown while a condition holds.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedSection {
    pub title: String,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub block: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub then: Option<AdvancedThen>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedThen {
    pub when: Condition,
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub save_preset: bool,
}

/// A setting at `path` equal to `equals` (`default` while the setting is absent).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub path: String,
    pub equals: String,
    #[serde(default)]
    pub default: String,
}

impl Condition {
    pub fn holds(&self, settings: &Value) -> bool {
        qnc_settings_path::path_string(settings, &self.path, &self.default) == self.equals
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedTexts {
    pub manual_preset: String,
    pub new_preset_hint: String,
    pub save_preset: String,
    pub no_selection: String,
    pub group: String,
    pub loading_catalog: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTexts {
    pub templates_empty: String,
    pub delete_template_hint: String,
    pub project_name_label: String,
    pub project_name_hint: String,
    pub new_project: String,
    pub create_project: String,
    pub story_sheet: String,
    pub settings_changed: String,
    pub path_hint: String,
    pub choose: String,
    pub confirm: String,
    pub cancel: String,
    pub new_template: String,
    /// `{base}` is the template the new one starts from.
    pub template_base: String,
    pub template_name_label: String,
    pub template_name_hint: String,
    pub save: String,
    pub short_description_hint: String,
    pub projects_root_label: String,
    pub export_directory_label: String,
    pub sources: String,
    pub up: String,
    pub disks: String,
    pub internet: String,
    pub empty_lan: String,
    pub empty_internet: String,
    pub empty_folder: String,
    pub empty_disks: String,
    pub remove_project_hint: String,
    pub delete_project_question: String,
    pub delete_template_question: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectLayout {
    pub layout_id: String,
    pub window: Window,
    pub board: Board,
    pub left_project_list: ProjectListMetrics,
    pub right_settings_panel: SettingsPanelMetrics,
    pub ai_switches: Vec<AiSwitch>,
    pub pts_slots: PtsSlots,
    pub advanced_settings: AdvancedSettings,
    pub texts: ProjectTexts,
}

impl ProjectLayout {
    /// The built-in contract in the language of this computer; `looked` changes its measures
    /// before it is read (the look of the form, as JSON pointers and values).
    pub fn load(looked: &[(String, f32)]) -> Result<ProjectLayout, String> {
        let mut value = qnc_text::resolve_contract(CONTRACT)?;
        for (pointer, measure) in looked {
            let slot = value.pointer_mut(pointer).ok_or_else(|| format!("project layout: the look points at {pointer}, which the contract lacks"))?;
            *slot = serde_json::json!(measure);
        }
        let layout: ProjectLayout = serde_json::from_value(value).map_err(|e| format!("project layout: {e}"))?;
        if layout.left_project_list.ready_text.trim().is_empty() {
            return Err("project layout: the ready text of the list is required".into());
        }
        if layout.right_settings_panel.field_min_width <= 0.0 || layout.board.left_ratio <= 0.0 || layout.window.width <= 0.0 {
            return Err("project layout: the measures must be positive".into());
        }
        Ok(layout)
    }

    /// Every action id the form may use that the key catalog lacks.
    pub fn actions_missing(ids: &[&str]) -> Vec<String> {
        let catalog = qnc_actions::catalog();
        ids.iter().filter(|id| !catalog.has(id)).map(|id| id.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_layout_is_valid_resolved_and_mirrors_the_old_project_board() {
        let layout = ProjectLayout::load(&[]).unwrap();
        assert_eq!((layout.board.left_ratio, layout.board.divider_width, layout.board.monitor_height), (0.31, 5.0, 1.0));
        assert_eq!((layout.left_project_list.row_height, layout.left_project_list.row_gap, layout.left_project_list.below_title_pad), (60.0, 10.0, 40.0));
        assert_eq!(layout.window.width, 640.0);
        assert!(!layout.left_project_list.title.starts_with('@'), "the texts are resolved: {}", layout.left_project_list.title);
        assert!(!layout.texts.new_project.starts_with('@'));
        assert_eq!(layout.pts_slots.fixed_order, ["TemplatePicker", "ProjectCreate"]);
        assert!(layout.advanced_settings.sections.iter().any(|section| section.block.as_deref() == Some("applications")));
        let when = &layout.advanced_settings.sections[0].then.as_ref().unwrap().when;
        assert!(when.holds(&serde_json::json!({ "input": { "mode": "manual" } })));
        assert!(!when.holds(&serde_json::json!({})));
        let moved = ProjectLayout::load(&[("/left_project_list/row_height".into(), 44.0)]).unwrap();
        assert_eq!(moved.left_project_list.row_height, 44.0);
        assert!(ProjectLayout::load(&[("/nowhere".into(), 1.0)]).is_err());
    }

    #[test]
    fn every_code_of_the_contract_is_in_the_dictionary() {
        let value: Value = serde_json::from_str(CONTRACT).unwrap();
        let texts = qnc_text::Texts::of(qnc_text::BASE_LANGUAGE).unwrap();
        let missing: Vec<String> = qnc_text::codes_in(&value).into_iter().filter(|code| texts.get(code).is_none()).collect();
        assert!(missing.is_empty(), "codes without a text: {missing:?}");
    }
}
