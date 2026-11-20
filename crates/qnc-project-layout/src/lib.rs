//! The layout of the Project form is data (`contracts/ui/project.layout.json`, mirrored from the
//! old QNC Project contract and the look the user set): the window, the board measures, the
//! measures of the project list and of the settings panel. The form and its panels draw with
//! these; no measure is written in code.

use std::sync::OnceLock;

use serde::Deserialize;

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

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct List {
    pub panel_pad: f32,
    pub title_row_height: f32,
    pub below_title_pad: f32,
    pub row_height: f32,
    pub row_gap: f32,
    pub row_text_inset: f32,
    pub delete_column_width: f32,
    pub column_gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Settings {
    pub panel_pad: f32,
    pub inner_pad_x: f32,
    pub inner_pad_y: f32,
    pub section_gap: f32,
    pub label_font_size: f32,
    pub row_height: f32,
    pub picker_pad_x: f32,
    pub picker_pad_y: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectLayout {
    pub window: Window,
    pub board: Board,
    pub list: List,
    pub settings: Settings,
}

impl ProjectLayout {
    pub fn parse(json: &str) -> Result<ProjectLayout, String> {
        serde_json::from_str(json).map_err(|e| format!("project layout: {e}"))
    }
}

static LAYOUT: OnceLock<ProjectLayout> = OnceLock::new();

/// The layout of this process.
pub fn layout() -> &'static ProjectLayout {
    LAYOUT.get_or_init(|| ProjectLayout::parse(CONTRACT).expect("the built-in project layout is valid; its test says so"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_layout_is_valid_and_mirrors_the_old_project_board() {
        let layout = ProjectLayout::parse(CONTRACT).unwrap();
        assert_eq!((layout.board.left_ratio, layout.board.divider_width, layout.board.monitor_height), (0.31, 5.0, 1.0));
        assert_eq!((layout.list.row_height, layout.list.row_gap, layout.list.below_title_pad), (60.0, 10.0, 40.0));
        assert_eq!(layout.window.width, 640.0);
        assert!(ProjectLayout::parse("{}").is_err());
    }
}
