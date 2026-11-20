//! The one QNC board (user rule 2026-09-30, mirrored from the old QNC): every form uses the
//! same board; a layout says how the window is split and which block goes where, and forms
//! only draw blocks by name.
//!
//! A layout is a tree of splits (data, [`Layout`]): each part of a split has a size (pixels,
//! a share with minimum widths, a picture shape, or the rest) and holds either a further split
//! or one block by name. The standard layout of every form is one such tree
//! ([`BoardSizes::standard_layout`]):
//!
//! ```text
//! +-----------+---+--------------------+
//! | monitor   |   |                    |
//! +-----------+ d |                    |
//! | head row  | i |  right             |
//! +-----------+ v |                    |
//! | body      |   |                    |
//! +-----------+---+--------------------+
//! | dock                               |
//! +------------------------------------+
//! ```
//!
//! Passive paint only: the board keeps no state, knows no form and reads nothing.

use egui::{Color32, Rect, Sense, Vec2};
use serde::{Deserialize, Serialize};

mod solve;
pub use solve::{solve, Solved};

/// The measures of the standard layout, from the form's layout contract.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct BoardSizes {
    /// Share of the width the left column takes.
    pub left_ratio: f32,
    pub divider_width: f32,
    pub left_min_width: f32,
    pub right_min_width: f32,
    /// Monitor height at the top of the left column; see [`BoardSizes::monitor_aspect`].
    pub monitor_height: f32,
    /// The row under the monitor (tabs, transport).
    pub head_height: f32,
    /// The dock along the bottom; never more than [`DOCK_MAX_SHARE`] of the height.
    pub dock_height: f32,
    /// When set, the monitor keeps this picture shape in the column width instead of
    /// `monitor_height`.
    #[serde(default)]
    pub monitor_aspect: Option<MonitorAspect>,
}

/// A monitor that keeps its picture shape.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct MonitorAspect {
    /// Width over height, e.g. 16/9.
    pub ratio: f32,
    /// Height the column keeps under the monitor.
    pub reserve_below: f32,
    pub min_height: f32,
    /// Picture width is the column width less this, never below `min_width`.
    pub width_inset: f32,
    pub min_width: f32,
}

impl MonitorAspect {
    /// The monitor height in a column of `width` x `height`.
    pub fn height(&self, width: f32, height: f32) -> f32 {
        let picture_width = (width - self.width_inset).max(self.min_width);
        let available = (height - self.reserve_below).max(self.min_height);
        (picture_width / self.ratio.max(0.1)).clamp(self.min_height, available)
    }
}

/// The dock never takes more than this share of the board height.
pub const DOCK_MAX_SHARE: f32 = 0.42;

/// The faces of the standard layout, by the names it uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardFaces {
    pub bg: Color32,
    pub left: Color32,
    pub right: Color32,
    pub divider: Color32,
}

impl BoardFaces {
    pub fn named(&self, name: &str) -> Option<Color32> {
        match name {
            "bg" => Some(self.bg),
            "left" => Some(self.left),
            "right" => Some(self.right),
            "divider" => Some(self.divider),
            _ => None,
        }
    }
}

/// The block names a form gives the five places of the standard layout; an empty name
/// leaves the place empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardNames<'a> {
    pub monitor: &'a str,
    pub head: &'a str,
    pub body: &'a str,
    pub right: &'a str,
    pub dock: &'a str,
}

/// Which way a split lays its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// The size of one part along its split.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    /// Fixed pixels, never more than `max_share` of the split.
    Px { px: f32, max_share: Option<f32> },
    /// A share of the split; never below `min`, and leaves `rest_min` to the rest.
    Share { ratio: f32, min: f32, rest_min: f32 },
    /// Keeps a picture shape in the width of a vertical split.
    Aspect(MonitorAspect),
    /// As long as the split's cross length times `ratio` (a picture beside other parts).
    Across { ratio: f32 },
    /// What the other parts leave.
    Rest,
}

/// One part of a split: its size, its face and what it holds.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Part {
    pub size: Size,
    #[serde(default)]
    pub face: Option<String>,
    /// Nothing for a plain face (a divider).
    #[serde(default)]
    pub node: Option<Node>,
}

/// A split or one block by name.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    Split { axis: Axis, parts: Vec<Part> },
    Block { name: String },
}

/// A whole layout: the face of the board and its tree.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Layout {
    #[serde(default)]
    pub face: Option<String>,
    pub root: Node,
}

fn block(name: &str) -> Option<Node> {
    Some(Node::Block { name: name.to_string() })
}

fn px(px: f32) -> Size {
    Size::Px { px, max_share: None }
}

impl BoardSizes {
    /// The layout every form uses: left column (monitor, head row, body), divider and
    /// right panel over the dock.
    pub fn standard_layout(&self, names: &BoardNames<'_>) -> Layout {
        let monitor = match self.monitor_aspect {
            Some(aspect) => Size::Aspect(aspect),
            None => px(self.monitor_height),
        };
        let left = Node::Split {
            axis: Axis::Vertical,
            parts: vec![
                Part { size: monitor, face: None, node: block(names.monitor) },
                Part { size: px(self.head_height), face: None, node: block(names.head) },
                Part { size: Size::Rest, face: None, node: block(names.body) },
            ],
        };
        let content = Node::Split {
            axis: Axis::Horizontal,
            parts: vec![
                Part { size: Size::Share { ratio: self.left_ratio, min: self.left_min_width, rest_min: self.right_min_width }, face: Some("left".into()), node: Some(left) },
                Part { size: px(self.divider_width), face: Some("divider".into()), node: None },
                Part { size: Size::Rest, face: Some("right".into()), node: block(names.right) },
            ],
        };
        Layout {
            face: Some("bg".into()),
            root: Node::Split {
                axis: Axis::Vertical,
                parts: vec![
                    Part { size: Size::Rest, face: None, node: Some(content) },
                    Part { size: Size::Px { px: self.dock_height, max_share: Some(DOCK_MAX_SHARE) }, face: None, node: block(names.dock) },
                ],
            },
        }
    }
}

impl Layout {
    /// A layout of one block in the whole board.
    pub fn block(name: &str) -> Self {
        Layout { face: None, root: Node::Block { name: name.to_string() } }
    }

    /// The same layout with the board painted in face `face`.
    pub fn with_face(mut self, face: &str) -> Self {
        self.face = Some(face.to_string());
        self
    }

    /// The place of block `name` split: `above` takes `height` pixels on top.
    pub fn split_place_above(mut self, name: &str, above: &str, height: f32) -> Self {
        if height > 0.0 {
            split_node(&mut self.root, name, above, height, Axis::Vertical, true);
        }
        self
    }

    /// The place of block `name` split: `right` takes `width` pixels on the right.
    pub fn split_place(mut self, name: &str, right: &str, width: f32) -> Self {
        if width > 0.0 {
            split_node(&mut self.root, name, right, width, Axis::Horizontal, false);
        }
        self
    }

    /// The place of block `name` split: `below` takes `height` pixels under it.
    pub fn split_place_below(mut self, name: &str, below: &str, height: f32) -> Self {
        if height > 0.0 {
            split_node(&mut self.root, name, below, height, Axis::Vertical, false);
        }
        self
    }
}

fn split_node(node: &mut Node, name: &str, other: &str, length: f32, axis: Axis, other_first: bool) {
    match node {
        Node::Block { name: found } if found == name => {
            let keep = Part { size: Size::Rest, face: None, node: block(name) };
            let taken = Part { size: px(length), face: None, node: block(other) };
            let parts = if other_first { vec![taken, keep] } else { vec![keep, taken] };
            *node = Node::Split { axis, parts };
        }
        Node::Block { .. } => {}
        Node::Split { parts, .. } => {
            for part in parts.iter_mut() {
                if let Some(node) = part.node.as_mut() {
                    split_node(node, name, other, length, axis, other_first);
                }
            }
        }
    }
}

/// Paints the standard layout with `faces` and calls `block` for every named place in the
/// order monitor, head, body, right, dock; every place is drawn, the first answer is the
/// board's (one intent per frame).
pub fn show<R>(ui: &mut egui::Ui, sizes: &BoardSizes, faces: &BoardFaces, names: &BoardNames<'_>, block: impl FnMut(&mut egui::Ui, &str, Rect) -> Option<R>) -> Option<R> {
    show_layout(ui, &sizes.standard_layout(names), |name| faces.named(name), block)
}

/// Paints any layout (faces by name through `face`) and calls `block` for every block.
pub fn show_layout<R>(ui: &mut egui::Ui, layout: &Layout, face: impl Fn(&str) -> Option<Color32>, mut block: impl FnMut(&mut egui::Ui, &str, Rect) -> Option<R>) -> Option<R> {
    let rect = ui.available_rect_before_wrap();
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return None;
    }
    ui.allocate_rect(rect, Sense::hover());
    let solved = solve(layout, rect);
    for (name, area) in &solved.faces {
        if let Some(color) = face(name) {
            ui.painter().rect_filled(*area, 0.0, color);
        }
    }
    // Every block is drawn even after an earlier one answered (a place left undrawn flashes).
    let mut answer = None;
    for (name, area) in &solved.blocks {
        if area.width() < 1.0 || area.height() < 1.0 {
            continue;
        }
        let this = ui.scope_builder(egui::UiBuilder::new().max_rect(*area).layout(egui::Layout::top_down(egui::Align::Min)), |ui| block(ui, name, *area)).inner;
        if answer.is_none() {
            answer = this;
        }
    }
    let _ = Vec2::ZERO;
    answer
}

#[cfg(test)]
mod tests;
