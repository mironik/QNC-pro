//! Placing a layout in a rectangle: faces in paint order, blocks in draw order.

use egui::{Rect, Vec2};

use crate::{Axis, Layout, Node, Part, Size};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solved {
    pub faces: Vec<(String, Rect)>,
    pub blocks: Vec<(String, Rect)>,
}

impl Solved {
    /// The rectangle of the block `name`.
    pub fn block(&self, name: &str) -> Option<Rect> {
        self.blocks.iter().find(|(block, _)| block == name).map(|(_, rect)| *rect)
    }
}

/// Places the layout in `rect`.
pub fn solve(layout: &Layout, rect: Rect) -> Solved {
    let mut solved = Solved::default();
    if let Some(face) = &layout.face {
        solved.faces.push((face.clone(), rect));
    }
    place_node(&layout.root, rect, &mut solved);
    solved
}

fn place_node(node: &Node, rect: Rect, solved: &mut Solved) {
    match node {
        Node::Block { name } => {
            if !name.is_empty() {
                solved.blocks.push((name.clone(), rect));
            }
        }
        Node::Split { axis, parts } => {
            let (total, cross) = match axis {
                Axis::Horizontal => (rect.width(), rect.height()),
                Axis::Vertical => (rect.height(), rect.width()),
            };
            let mut at = 0.0;
            for (part, length) in parts.iter().zip(part_lengths(parts, total, cross)) {
                let area = match axis {
                    Axis::Horizontal => Rect::from_min_size(egui::pos2(rect.left() + at, rect.top()), Vec2::new(length, rect.height())),
                    Axis::Vertical => Rect::from_min_size(egui::pos2(rect.left(), rect.top() + at), Vec2::new(rect.width(), length)),
                };
                at += length;
                if let Some(face) = &part.face {
                    solved.faces.push((face.clone(), area));
                }
                if let Some(node) = &part.node {
                    place_node(node, area, solved);
                }
            }
        }
    }
}

/// Lengths of the parts along a split of `total`: fixed parts in order, each at most what is
/// left, then the rest shared by the rest parts.
pub(crate) fn part_lengths(parts: &[Part], total: f32, cross: f32) -> Vec<f32> {
    let mut left = total;
    let lengths: Vec<Option<f32>> = parts
        .iter()
        .map(|part| {
            let wanted = match part.size {
                Size::Px { px, max_share } => max_share.map_or(px, |share| px.min(total * share)),
                Size::Share { ratio, min, rest_min } => {
                    let usable = total.max(min + rest_min);
                    (usable * ratio).clamp(min, usable - rest_min)
                }
                Size::Aspect(aspect) => aspect.height(cross, total),
                Size::Across { ratio } => cross * ratio.max(0.0),
                Size::Rest => return None,
            };
            let length = wanted.clamp(0.0, left.max(0.0));
            left -= length;
            Some(length)
        })
        .collect();
    let rest_count = lengths.iter().filter(|length| length.is_none()).count().max(1) as f32;
    let rest = left.max(0.0) / rest_count;
    lengths.into_iter().map(|length| length.unwrap_or(rest)).collect()
}
